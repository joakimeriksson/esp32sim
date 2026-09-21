use super::*;

#[cfg(feature = "wasm-cache-inline")]
pub(super) fn inline_cache_hits() -> u32 {
    use emu_core::bus::FastCacheLine;
    super::CACHE_PROBES.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut tests = 0;
    for store in [false, true] {
        for way in 0..9 { // Every way, then a miss that must run the helper.
            let mut ram = Ram::new(true, false);
            ram.tlb[tlb_index(BASE)].src = 3;
            let tag = (0x3000_0000u32 | 0x100) >> 6;
            let slot = ((tag & 63) * 8) as usize;
            let mut lines = vec![FastCacheLine::default(); 512];
            if way < 8 { lines[slot + way] = FastCacheLine { tag, dirty: 0, valid: 1 }; }
            ram.inline_cache = Some((lines, 0));
            let mut block = [insn(if store { Op::S32i } else { Op::L32i })];
            block[0].insn.imm = 0;
            let mut cc = CodeCache::new(0).unwrap();
            let code = queue(&mut cc, &mut block, BASE, true);
            for _ in 0..HOT { ready(&cc, code, 0); }
            assert!(ready(&cc, code, 0));
            let mut c = cpu(0);
            c.set_ar(4, BASE + 0x100);
            c.set_ar(5, 0x1234_5678);
            let fm = ram.fast_mem();
            let result = unsafe { run(&cc, code, &mut c, &mut ram, &Helpers::new::<Ram>(), 1, 0, fm) };
            assert_eq!(result & 0xffff, 1);
            let (lines, hits) = ram.inline_cache.as_ref().unwrap();
            assert_eq!(*hits, u64::from(way < 8));
            assert_eq!(ram.helper_accesses, u32::from(way == 8));
            if store {
                if way < 8 { assert_eq!(lines[slot + way].dirty, 1); }
                assert_eq!(ram.versions[1], 1);
                assert_eq!(ram.ram.read32(BASE + 0x100).unwrap(), 0x1234_5678);
            } else {
                assert_eq!(c.get_ar(5), ram.ram.read32(BASE + 0x100).unwrap());
            }
            tests += 1;
        }
    }
    super::CACHE_PROBES.store(false, std::sync::atomic::Ordering::Relaxed);
    tests
}

pub(super) fn extension_deferral() -> u32 {
    let mut c = cpu(0);
    let mut ram = Ram::new(true, false);
    ram.defer_armed = true;
    let mut tests = 0;
    for wb in [0, 15] {
        c.windowbase = wb;
        for ar in 0..16 {
            for (op, raw) in [(Op::Pie, 0), (Op::Mac16, 0), (Op::Mac16, 1 << 20),
                (Op::Mac16, 4 << 20), (Op::Mac16, 5 << 20), (Op::Mac16, 8 << 20), (Op::Mac16, 9 << 20)] {
                let mut bi = insn(op);
                bi.insn.raw = raw;
                c.ar.fill(0);
                assert!(!crate::exec::defer_instruction(&c, &mut ram, &bi.insn));
                c.set_ar(ar, SLOW);
                ram.deferred = false;
                assert!(crate::exec::defer_instruction(&c, &mut ram, &bi.insn));
                assert!(ram.deferred);
                tests += 1;
            }
        }
    }
    for op2 in [2, 3, 6, 7] {
        let mut bi = insn(Op::Mac16);
        bi.insn.raw = op2 << 20;
        assert!(!crate::exec::defer_instruction(&c, &mut ram, &bi.insn), "pure MAC16 does not access memory");
        tests += 1;
    }
    ram.defer_armed = false;
    assert!(!crate::exec::defer_instruction(&c, &mut ram, &insn(Op::Pie).insn));
    ram.defer_armed = true;
    c.windowbase = 0;
    c.ar.fill(0);
    c.set_ar(0, SLOW);
    c.cpenable = 8;
    c.qr[0] = u128::from_le_bytes([1; 16]);
    c.accx = [9, 0];
    // ee.vmulas.s8.accx.ld.ip would modify ACCX before its slow load. Both
    // execution paths must defer before that partial architectural mutation.
    let bytes = 0xf002_000eu32.to_le_bytes();
    let i = crate::decode::decode(BASE, bytes);
    let bi = BlockInsn { insn: i, max_ar: crate::exec::max_ar(&i), straddle: false, off: 0 };
    ram.deferred = false;
    assert_eq!(h_exec::<Ram>(&mut c, &mut ram, &bi, BASE), 1);
    assert!(ram.deferred);
    assert_eq!(c.accx, [9, 0]);
    assert!(c.jit_trap.is_none());
    ram.ram.mem[..4].copy_from_slice(&bytes);
    ram.ram.mem[4..7].copy_from_slice(&asm::j(BASE + 4, BASE));
    c.blocks.jit_enabled = false;
    ram.deferred = false;
    assert_eq!(crate::block::run_block(&mut c, &mut ram, 64), (0, None));
    assert!(ram.deferred);
    assert_eq!(c.accx, [9, 0]);
    assert_eq!(c.pc, BASE);
    tests + 3
}

pub(super) fn flat_ram_bounds() -> u32 {
    // On wasm32, a below-base offset plus its width used to wrap usize to zero.
    let mut ram = FlatRam::new(BASE, 16);
    assert_eq!(ram.read8(BASE - 1), Err(Fault::Unmapped));
    assert_eq!(ram.read16(BASE - 2), Err(Fault::Unmapped));
    assert_eq!(ram.read32(BASE - 4), Err(Fault::Unmapped));
    assert_eq!(ram.write8(BASE - 1, 1), Err(Fault::Unmapped));
    assert_eq!(ram.write16(BASE - 2, 1), Err(Fault::Unmapped));
    assert_eq!(ram.write32(BASE - 4, 1), Err(Fault::Unmapped));
    assert!(!ram.read_bulk(BASE - 4, &mut [0; 4]));
    assert_eq!(ram.fetch(BASE - 1), Err(Fault::Unmapped));
    assert_eq!(ram.ver, 0);
    assert_eq!(ram.mem, vec![0; 16]);
    8
}

/// EX180: an Xtensa instruction can begin up to three bytes before a page boundary (PIE
/// admits 4-byte encodings, `pie::decode`), so a store into the first three bytes of a
/// version page also changes instructions whose code page is the previous one. The bus
/// bumps that page too (`esp32s3/src/bus.rs` `bump`) and the aarch64 JIT leaves its fast
/// path for those offsets (`jit/mod.rs`, S8i..S32ri). Every store width and page offset a
/// generated fast store can reach must record exactly the same pages as the interpreter.
pub(super) fn page_boundary_stores() -> u32 {
    use Op::*;
    let mut tests = 0;
    for (op, width) in [(S8i, 1u32), (S16i, 2), (S32i, 4), (S32iN, 4)] {
        // Page 0 exercises the bus's `p > 0` guard: nothing precedes the first page.
        for page in [0u32, 1, 17] {
            for off in [0u32, 1, 2, 3, 252, 253, 254, 255] {
                if off % width != 0 { continue; }               // the fast path is aligned only
                let addr = BASE + page * 256 + off;
                for fast in [false, true] {
                    let mut block = [insn(Add), insn(op), insn(Xor)];
                    compare(&mut block, Case { seed: 21, budget: 3, addr: Some(addr), fast, ..Case::default() }, |_| {});
                    tests += 1;
                }
            }
        }
    }
    tests
}

/// EX180, the consequence: a 3-byte ADDI at page offset 254 keeps its immediate byte in
/// the next page. A hot block on a third page rewrites that byte through a generated fast
/// store, so the decoded block holding the ADDI must be thrown away every time and both
/// engines must agree on the architectural result and on the recorded pages.
pub(super) fn straddling_instruction_rewrite() -> u32 {
    // +0x0fe addi a3,a3,imm8   bytes 0x0fe,0x0ff and the immediate at 0x100 (page 1)
    // +0x101 j +0x200
    // +0x200 s8i a5,a4,0       a4 = BASE + 0x100: rewrites that immediate
    // +0x203 addi.n a5,a5,1    a different immediate every iteration
    // +0x205 addi.n a2,a2,1
    // +0x207 j +0x0fe          this block stays in page 2, so it stays hot and compiled
    let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
    for ram in [&mut ra, &mut rb] {
        let m = &mut ram.ram.mem;
        m[0x0fe..0x101].copy_from_slice(&asm::rri8(2, 0xc, 3, 3, 1));
        m[0x101..0x104].copy_from_slice(&asm::j(BASE + 0x101, BASE + 0x200));
        m[0x200..0x203].copy_from_slice(&asm::s8i(5, 4, 0));
        m[0x203..0x205].copy_from_slice(&asm::addi_n(5, 5, 1));
        m[0x205..0x207].copy_from_slice(&asm::addi_n(2, 2, 1));
        m[0x207..0x20a].copy_from_slice(&asm::j(BASE + 0x207, BASE + 0x0fe));
    }
    assert_eq!(crate::decode::decode(BASE + 0x0fe, ra.fetch(BASE + 0x0fe).unwrap()).op, Op::Addi);
    let (mut a, mut b) = (cpu(5), cpu(5));
    for c in [&mut a, &mut b] {
        c.pc = BASE + 0x200;
        c.ps = 0;
        c.set_ar(2, 0);
        c.set_ar(3, 0);
        c.set_ar(4, BASE + 0x100);
        c.set_ar(5, 1);
    }
    CONTEXT.with(|c| *c.borrow_mut() = String::from("straddling instruction rewrite"));
    for turn in 0..300 {
        let budget = 1 + turn % 9;
        let (done, trap) = crate::block::run_block(&mut b, &mut rb, budget);
        assert!(trap.is_none());
        for _ in 0..done { crate::step(&mut a, &mut ra).unwrap(); }
        same(&a, &b);
        assert_eq!(ra.ram.mem, rb.ram.mem, "memory after turn {turn}");
        assert_eq!(ra.versions, rb.versions, "page versions after turn {turn}");
    }
    assert!(b.blocks.jit_instructions > 100, "the rewriting block never ran compiled");
    assert!(a.get_ar(3) > 100, "the straddling ADDI never accumulated");
    1
}

pub(super) fn loads_and_stores() -> u32 {
    use Op::*;
    let mut tests = 0;
    for op in [
        L8ui, L16ui, L16si, L32i, L32iN, L32r, S8i, S16i, S32i, S32iN,
    ] {
        for addr in [BASE + 0x100, BASE + 0x1ff, BASE + 65535, BASE - 16] {
            for fast in [false, true] {
                for readonly in [false, true] {
                    let mut block = [insn(Add), insn(op), insn(Xor)];
                    if op == L32r {
                        block[1].insn.imm = addr as i32;
                    }
                    compare(&mut block, Case { seed: 15, budget: 3, addr: Some(addr), fast, readonly, ..Case::default() }, |_| {});
                    tests += 1;
                }
            }
        }
    }
    tests
}
