use super::*;

/// Run `program` through the block scheduler with regions and compare against
/// instruction-by-instruction execution under varying credit, probes, code changes
/// and timer deadlines. Returns the largest single-call retirement, which proves that
/// a region ran past its head block.
fn region_program(name: &str, program: &[u8], expected: &[(u32, Op, u32)], data: &[u8], head_len: u32, interior: u32, setup: impl Fn(&mut Cpu), turns: usize) -> u32 {
    region_program_on(name, program, expected, data, head_len, interior, false, setup, turns)
}
fn region_program_on(name: &str, program: &[u8], expected: &[(u32, Op, u32)], data: &[u8], head_len: u32, interior: u32, readonly: bool, setup: impl Fn(&mut Cpu), turns: usize) -> u32 {
    let (mut a, mut b) = (cpu(3), cpu(3));
    let (mut ra, mut rb) = (Ram::new(true, readonly), Ram::new(true, readonly));
    for r in [&mut ra, &mut rb] {
        r.ram.mem[..program.len()].copy_from_slice(program);
        r.ram.mem[0x1000..0x1000 + data.len()].copy_from_slice(data);
    }
    CONTEXT.with(|c| *c.borrow_mut() = format!("region program {name}"));
    for &(off, op, target) in expected {
        let i = crate::decode::decode(BASE + off, ra.fetch(BASE + off).unwrap());
        assert_eq!(i.op, op, "{name}: encoding at +{off}: {}", crate::disasm::format(&i));
        if target != 0 { assert_eq!(i.imm as u32, BASE + target, "{name}: target at +{off}: {}", crate::disasm::format(&i)); }
    }
    for c in [&mut a, &mut b] {
        c.pc = BASE;
        c.ps = 0;
        setup(c);
    }
    let mut max_done = 0;
    for turn in 0..turns {
        let budget = 1 + (turn * 7) as u32 % 71;
        let head_probe = (40..45).contains(&(turn % 50));
        for c in [&mut a, &mut b] {
            // A probe on an interior chunk head must stop the region without a flush;
            // a probe on the head itself must stop internal backedges to it.
            if turn % 50 == 25 { c.boundary_bloom = emu_core::core::pc_bit(BASE + interior); }
            if turn % 50 == 35 { c.boundary_bloom = 0; }
            if turn % 50 == 40 { c.boundary_bloom = emu_core::core::pc_bit(BASE); }
            if turn % 50 == 45 { c.boundary_bloom = 0; }
            // A timer deadline inside the region must land on the same instruction.
            if turn % 90 == 60 { c.ccompare[0] = c.ccount.wrapping_add(1 + (turn % 13) as u32); c.intenable = 1 << 6; }
        }
        let start = b.pc;
        CONTEXT.with(|c| *c.borrow_mut() = format!("region program {name} turn {turn} start {start:x} budget {budget}"));
        let (done, trap) = crate::block::run_block(&mut b, &mut rb, budget);
        assert!(done <= budget, "{name}: {done} > budget {budget}");
        if head_probe && start == BASE { assert!(done <= head_len, "{name}: region ran through a probed head ({done})"); }
        max_done = max_done.max(done);
        let mut oracle = None;
        for _ in 0..done {
            ra.note_pc(a.pc);
            if let Err(t) = crate::step(&mut a, &mut ra) { oracle = Some(t); break; }
        }
        assert_eq!(trap, oracle, "{name}: turn {turn} budget {budget}");
        same(&a, &b);
        assert_eq!(ra.ram.mem, rb.ram.mem, "{name}: memory after turn {turn}");
        assert_eq!(ra.versions, rb.versions, "{name}: versions after turn {turn}");
        if done > 0 && trap.is_none() { assert_eq!(ra.noted, rb.noted, "{name}: noted PC after turn {turn}"); }
        if let Some(Trap::Interrupt(_)) = trap {
            // Return from the interrupt by hand: both continue at the interrupted PC.
            for c in [&mut a, &mut b] { c.pc = c.epc[1]; c.ps &= !ps::EXCM; c.interrupt = 0; c.ccompare[0] = 0; }
        }
    }
    max_done
}

pub(super) fn regions() -> u32 {
    use Op::*;
    let mut cases = 0;
    // The ROM memmove byte loop: a 6-instruction body ending in J, a one-instruction
    // BNE block branching back, then a boundary the region cannot cross.
    let mut p = Vec::new();
    p.extend(asm::add_n(9, 3, 8));       // 0  a9 = src + i
    p.extend(asm::l8ui(10, 9, 0));       // 2
    p.extend(asm::add_n(9, 2, 8));       // 5  a9 = dst + i
    p.extend(asm::s8i(10, 9, 0));        // 7
    p.extend(asm::addi_n(8, 8, 1));      // 10
    p.extend(asm::j(BASE + 12, BASE + 17)); // 12
    p.extend(asm::movi_n(8, 0));         // 15  (never executed)
    p.extend(asm::bcc(9, BASE + 17, 4, 8, BASE)); // 17 bne a4, a8, 0
    p.extend(asm::rsr(13, 234));         // 20  rsr a13, ccount: a block boundary
    p.extend(asm::movi_n(8, 0));         // 23  restart the copy
    p.extend(asm::j(BASE + 25, BASE));   // 25
    let memmove = [(0, AddN, 0), (2, L8ui, 0), (5, AddN, 0), (7, S8i, 0), (10, AddiN, 0), (12, J, 17), (17, Bne, 0), (20, Rsr, 0), (23, MoviN, 0), (25, J, 0)];
    {
        let mut ram = Ram::new(true, false);
        ram.ram.mem[..p.len()].copy_from_slice(&p);
        let c = cpu(0);
        let head: Vec<BlockInsn> = (0..6).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, straddle: false, off: 0 }) }).collect();
        let formed = emitter::region::form(&c, &mut ram, BASE, &head, true).expect("memmove region");
        assert_eq!(formed.chunks.iter().map(|c| (c.pc - BASE, c.instructions.len())).collect::<Vec<_>>(), vec![(0, 6), (17, 1)]);
        let (bytes, sites) = emitter::region::generate(&formed.chunks, &formed.pages, &formed.loops, true);
        let slot = unsafe { host_jit_compile(bytes.as_ptr(), bytes.len()) };
        assert!(slot != 0, "memmove region module must compile ({} bytes, {} sites)", bytes.len(), sites.len());
        unsafe { host_jit_release(slot) };
    }
    for (src, dst, whole) in [
        (BASE + 0x1000, BASE + 0x2000, true),
        // Copying the program onto itself and over its own tail bumps the code page the
        // region was decoded from: every store sets DIRTY, so the region leaves at each
        // edge, and the dispatcher must re-validate and rebuild exactly.
        (BASE, BASE, false),
        (BASE + 0x1000, BASE + 0x10, false),
        // Slow loads and stores leave generated code through the helper in a successor chunk.
        (SLOW, BASE + 0x2000, false),
        (BASE + 0x1000, SLOW, false),
    ] {
        let max = region_program("memmove", &p, &memmove, &[], 6, 17, |c| {
            c.set_ar(2, dst); c.set_ar(3, src); c.set_ar(4, 40); c.set_ar(8, 0);
        }, 700);
        let stats: Vec<u32> = REGION_STATS.iter().map(|s| s.load(std::sync::atomic::Ordering::Relaxed)).collect();
        assert!(!whole || max >= 40, "memmove region retired at most {max} per call; formed/failed/maxdone/exits[END,LEFT,TRAP,CUT,PRE,REJ,_,_]/maxbudget {stats:?}");
        cases += 1;
    }
    // A tile scan: two chunks in a loop with a conditional exit to a third. The second
    // chunk rewrites the first immediate of the chunk it falls into, so the region
    // must leave at that edge and the rebuilt block must see the new instruction.
    let mut tp = Vec::new();
    tp.extend(asm::l16ui(11, 8, 0));                 // 0
    tp.extend(asm::bcc(9, BASE + 3, 11, 6, BASE + 35)); // 3  bne a11, a6, 35
    tp.extend(asm::addi_n(8, 8, 2));                 // 6
    tp.extend(asm::addi_n(10, 10, -1));              // 8
    tp.extend(asm::bz(1, BASE + 10, 10, BASE));      // 10 bnez a10, 0
    tp.extend(asm::movi_n(10, 8));                   // 13 reset: the immediate byte at 14 gets toggled 8 <-> 9
    tp.extend(asm::mov_n(8, 12));                    // 15
    tp.extend(asm::l8ui(14, 15, 0));                 // 17
    tp.extend(asm::xor(14, 14, 5));                  // 20
    tp.extend(asm::s8i(14, 15, 0));                  // 23 a store into the region's own code page
    tp.extend(asm::j(BASE + 26, BASE + 43));         // 26 j 43: forward, non-contiguous
    tp.extend(asm::rsr(13, 234));                    // 29 (never executed)
    tp.extend(asm::rsr(13, 234));                    // 32 (never executed)
    tp.extend(asm::bcc(1, BASE + 35, 6, 7, BASE + 41)); // 35 beq a6, a7, 41 (always taken)
    tp.extend(asm::rsr(13, 234));                    // 38 (never executed)
    tp.extend(asm::addi_n(8, 8, 2));                 // 41 skip the odd halfword
    tp.extend(asm::j(BASE + 43, BASE));              // 43 j 0
    let tile = [(0, L16ui, 0), (3, Bne, 35), (6, AddiN, 0), (8, AddiN, 0), (10, Bnez, 0), (13, MoviN, 0), (15, MovN, 0), (17, L8ui, 0), (20, Xor, 0), (23, S8i, 0), (26, J, 43), (35, Beq, 41), (38, Rsr, 0), (41, AddiN, 0), (43, J, 0)];
    // A zero buffer with one odd halfword: the scan matches until it reaches it.
    let mut data = [0u8; 32];
    data[14] = 0x34; data[15] = 0x12;
    let stat = |i: usize| REGION_STATS[i].load(std::sync::atomic::Ordering::Relaxed);
    let (formed0, dropped0, covered0) = (stat(0), stat(9), stat(10));
    let max = region_program("tile", &tp, &tile, &data, 2, 6, |c| {
        c.set_ar(12, BASE + 0x1000); c.set_ar(8, BASE + 0x1000); c.set_ar(10, 8);
        c.set_ar(6, 0); c.set_ar(7, 0); c.set_ar(15, BASE + 14); c.set_ar(5, 0x10);
    }, 900);
    assert!(max >= 30, "tile region retired at most {max} per call");
    // Four heads of this loop become hot, but a head inside a live region gets no region
    // of its own: only the first, plus one re-formation per code change.
    assert!(stat(10) > covered0, "no covered head was skipped");
    assert!(stat(0) - formed0 <= stat(9) - dropped0 + 1, "overlapping regions: formed {} dropped {}", stat(0) - formed0, stat(9) - dropped0);
    cases += 1;
    // A hardware loop with a branch inside its body: the LOOPNEZ, the body, the skip
    // target and the loop exit are all region chunks; the backedge is an internal edge,
    // and a credit cut inside the loop re-enters with the loop active.
    let mut p = Vec::new();
    p.extend(asm::mov_n(8, 14));                    // 0  a8 = buffer
    p.extend(asm::movi_n(10, 12));                  // 2
    p.extend(asm::lp(9, BASE + 4, 10, BASE + 17));  // 4  loopnez a10, 17
    p.extend(asm::l8ui(11, 8, 0));                  // 7
    p.extend(asm::bz(0, BASE + 10, 11, BASE + 15)); // 10 beqz a11, 15
    p.extend(asm::addi_n(9, 9, 1));                 // 13
    p.extend(asm::addi_n(8, 8, 1));                 // 15 (ends at LEND)
    p.extend(asm::addi_n(12, 12, 1));               // 17 loop exit
    p.extend(asm::rsr(13, 234));                    // 19
    p.extend(asm::j(BASE + 22, BASE));              // 22
    let hwloop = [(0, MovN, 0), (2, MoviN, 0), (4, Loopnez, 17), (7, L8ui, 0), (10, Beqz, 15), (13, AddiN, 0), (15, AddiN, 0), (17, AddiN, 0), (19, Rsr, 0), (22, J, 0)];
    let data: Vec<u8> = (0..16u8).map(|i| i & 1).collect();
    let formed_before = REGION_STATS[0].load(std::sync::atomic::Ordering::Relaxed);
    let max = region_program("hwloop", &p, &hwloop, &data, 3, 7, |c| {
        c.set_ar(14, BASE + 0x1000); c.set_ar(9, 0); c.set_ar(12, 0);
    }, 900);
    assert!(max >= 40, "hardware-loop region retired at most {max} per call");
    assert!(REGION_STATS[0].load(std::sync::atomic::Ordering::Relaxed) > formed_before, "hwloop formed no region");
    cases += 1;
    {
        let mut ram = Ram::new(true, false);
        ram.ram.mem[..p.len()].copy_from_slice(&p);
        let c = cpu(0);
        let head: Vec<BlockInsn> = (0..3).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, straddle: false, off: 0 }) }).collect();
        let formed = emitter::region::form(&c, &mut ram, BASE, &head, true).expect("hwloop region");
        assert_eq!(formed.loops, vec![(BASE + 17, BASE + 7)]);
        assert_eq!(formed.chunks.iter().map(|c| (c.pc - BASE, c.instructions.len())).collect::<Vec<_>>(),
            vec![(0, 3), (7, 2), (17, 1), (15, 1), (13, 2)]);
        cases += 1;
    }
    // Calls and returns end chunks and leave; a function entry heads a region whose
    // window proof is redone after ENTRY; RETW.N runs through the terminal helper.
    let mut p = Vec::new();
    p.extend(asm::call8(BASE, BASE + 12));          // 0  call8 F
    p.extend(asm::addi_n(2, 2, 1));                 // 3  (return address)
    p.extend(asm::addi_n(3, 3, 1));                 // 5
    p.extend(asm::j(BASE + 7, BASE));               // 7
    p.extend(asm::nop_n());                         // 10
    p.extend(asm::entry(1, 32));                    // 12 F: entry a1, 32
    p.extend(asm::movi_n(3, 5));                    // 15
    p.extend(asm::add_n(2, 2, 3));                  // 17
    p.extend(asm::bz(2, BASE + 19, 2, BASE + 24));  // 19 bltz a2, 24
    p.extend(asm::retw_n());                        // 22
    p.extend(asm::movi_n(2, 0));                    // 24
    p.extend(asm::retw_n());                        // 26
    let calls = [(0, Call8, 12), (3, AddiN, 0), (5, AddiN, 0), (7, J, 0), (10, NopN, 0), (12, Entry, 0), (15, MoviN, 0), (17, AddN, 0), (19, Bltz, 24), (22, RetwN, 0), (24, MoviN, 0), (26, RetwN, 0)];
    let formed_before = REGION_STATS[0].load(std::sync::atomic::Ordering::Relaxed);
    let max = region_program("calls", &p, &calls, &[], 1, 12, |c| {
        c.ps = ps::WOE; c.windowstart = 1 << c.windowbase;
        c.set_ar(10, (-100i32) as u32); c.set_ar(2, 0); c.set_ar(3, 0);
    }, 900);
    assert!(max >= 4, "call region retired at most {max} per call");
    assert!(REGION_STATS[0].load(std::sync::atomic::Ordering::Relaxed) >= formed_before + 2, "call/entry regions did not form");
    cases += 1;
    {
        let mut ram = Ram::new(true, false);
        ram.ram.mem[..p.len()].copy_from_slice(&p);
        let c = cpu(0);
        let head: Vec<BlockInsn> = (0..4).scan(BASE + 12, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, straddle: false, off: 0 }) }).collect();
        let formed = emitter::region::form(&c, &mut ram, BASE + 12, &head, true).expect("entry region");
        assert_eq!(formed.chunks.iter().map(|c| (c.pc - BASE, c.instructions.len())).collect::<Vec<_>>(),
            vec![(12, 4), (24, 2), (22, 1)]);
        assert!(emitter::region::form(&c, &mut ram, BASE, &head[..1], true).is_none(), "a lone call is not a region");
        cases += 1;
    }
    // A malformed `entry a4` heading a region: its own operand must still be proved free
    // before the interpreter helper runs it, so an occupied frame raises the overflow.
    let mut p = Vec::new();
    p.extend(asm::entry(4, 32));                    // 0
    p.extend(asm::bz(1, BASE + 3, 2, BASE + 9));    // 3  bnez a2, 9
    p.extend(asm::j(BASE + 6, BASE));               // 6
    p.extend(asm::addi_n(2, 2, -1));                // 9
    p.extend(asm::j(BASE + 11, BASE));              // 11
    let bad = [(0, Entry, 0), (3, Bnez, 9), (6, J, 0), (9, AddiN, 0), (11, J, 0)];
    for occupied in [false, true] {
        region_program("entry-a4", &p, &bad, &[], 1, 9, |c| {
            c.ps = ps::WOE | (2 << ps::CALLINC_SHIFT);
            c.windowstart = (1 << c.windowbase) | if occupied { 1 << ((c.windowbase + 1) % 16) } else { 0 };
            c.set_ar(2, 3); c.set_ar(4, BASE + 0x4000);
        }, 60);
        cases += 1;
    }
    // An ENTRY-headed region entered at an interior chunk that touches a8, with the
    // frame a8 lives in occupied: the module must reject, so the block path raises the
    // overflow exception. Called directly: the ENTRY block itself never gets hot here.
    let mut p = Vec::new();
    p.extend(asm::entry(1, 32));                    // 0
    p.extend(asm::addi_n(8, 8, 1));                 // 3
    p.extend(asm::addi_n(2, 2, -1));                // 5
    p.extend(asm::bz(1, BASE + 7, 2, BASE + 3));    // 7  bnez a2, 3
    p.extend(asm::j(BASE + 10, BASE + 3));          // 10 j 3
    {
        let mut ram = Ram::new(true, false);
        ram.ram.mem[..p.len()].copy_from_slice(&p);
        let c0 = cpu(0);
        let head: Vec<BlockInsn> = (0..4).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: crate::exec::max_ar(&i), straddle: false, off: 0 }) }).collect();
        let formed = emitter::region::form(&c0, &mut ram, BASE, &head, true).expect("entry-interior region");
        let interior = formed.chunks.iter().position(|c| c.pc == BASE + 3).expect("interior chunk") as u32;
        let (bytes, _) = emitter::region::generate(&formed.chunks, &formed.pages, &formed.loops, true);
        let slot = unsafe { host_jit_compile(bytes.as_ptr(), bytes.len()) };
        assert!(slot != 0);
        type Run = extern "C" fn(*mut Cpu, *mut Ram, *const Helpers, u32, u32, *const TlbEntry, *mut u32) -> u32;
        let f: Run = unsafe { std::mem::transmute(slot as usize) };
        for occupied in [false, true] {
            let mut c = cpu(3);
            c.pc = BASE + 3;
            c.ps = ps::WOE;
            c.windowstart = (1 << c.windowbase) | if occupied { 1 << ((c.windowbase + 2) % 16) } else { 0 };
            c.set_ar(2, 5);
            let before = c.ar;
            let fm = ram.fast_mem().unwrap();
            let result = f(&mut c, &mut ram, &Helpers::new::<Ram>(), 64, interior, fm.tlb, fm.page_ver);
            if occupied {
                assert_eq!(result >> 16, CODE_REJECT, "interior entry over an occupied frame must reject");
                assert_eq!(c.ar, before);
            } else {
                assert_eq!((result >> 16) & 7, CODE_LEFT);
                assert!(result & 0xffff >= 3, "interior entry ran {} instructions", result & 0xffff);
            }
            cases += 1;
        }
        unsafe { host_jit_release(slot) };
    }
    // A hardware loop whose body is exactly one ENTRY: when the post-ENTRY window proof
    // fails, the side exit must still take the backedge that ends at ENTRY's successor.
    let mut p = Vec::new();
    p.extend(asm::movi_n(10, 3));                   // 0
    p.extend(asm::lp(9, BASE + 2, 10, BASE + 8));   // 2  loopnez a10, 8
    p.extend(asm::entry(1, 32));                    // 5  (ends at LEND)
    p.extend(asm::addi_n(8, 8, 1));                 // 8
    p.extend(asm::j(BASE + 10, BASE));              // 10
    let loop_entry = [(0, MoviN, 0), (2, Loopnez, 8), (5, Entry, 0), (8, AddiN, 0), (10, J, 0)];
    for occupied in [false, true] {
        region_program("loop-entry", &p, &loop_entry, &[], 2, 5, |c| {
            c.ps = ps::WOE | (2 << ps::CALLINC_SHIFT);
            c.windowstart = (1 << c.windowbase) | if occupied { 1 << ((c.windowbase + 3) % 16) } else { 0 };
            c.set_ar(1, BASE + 0x4000);
        }, 60);
        cases += 1;
    }
    // The TinyDraw tile-uniform kernel shape: Q0 filled from a register, a 128-bit load
    // and lane compare, a LOOPNEZ over load/compare/and, then a store and scalar reduce.
    // Every PIE instruction is emitted on WASM SIMD and the loop is a region.
    use crate::pie::Role::*;
    let mut p = Vec::new();
    for sel in 0..4 { p.extend(asm::pie("ee.movi.32.q", &[(Qu, 0), (As, 7), (Sel, sel)])); } // 0,3,6,9
    p.extend(asm::mov_n(8, 12));                                             // 12
    p.extend(asm::pie("ee.vld.128.ip", &[(Qu, 1), (As, 8), (Imm, 16)]));     // 14
    p.extend(asm::pie("ee.vcmp.eq.s16", &[(Qa, 3), (Qx, 1), (Qy, 0)]));      // 17
    p.extend(asm::movi_n(10, 3));                                            // 20
    p.extend(asm::lp(9, BASE + 22, 10, BASE + 34));                          // 22 loopnez a10, 34
    p.extend(asm::pie("ee.vld.128.ip", &[(Qu, 1), (As, 8), (Imm, 16)]));     // 25
    p.extend(asm::pie("ee.vcmp.eq.s16", &[(Qa, 2), (Qx, 1), (Qy, 0)]));      // 28
    p.extend(asm::pie("ee.andq", &[(Qa, 3), (Qx, 3), (Qy, 2)]));             // 31
    p.extend(asm::mov_n(10, 13));                                            // 34
    p.extend(asm::pie("ee.vst.128.ip", &[(Qv, 3), (As, 10), (Imm, 16)]));    // 36
    p.extend(asm::l32i_n(10, 13, 0));                                        // 39
    p.extend(asm::l32i_n(11, 13, 4));                                        // 41
    p.extend(asm::and(10, 10, 11));                                          // 43
    p.extend(asm::s32i_n(10, 13, 8));                                        // 46
    p.extend(asm::j(BASE + 48, BASE + 12));                                  // 48: an internal edge after the stores
    let uniform = [(0, Pie, 0), (3, Pie, 0), (12, MovN, 0), (14, Pie, 0), (17, Pie, 0), (20, MoviN, 0), (22, Loopnez, 34), (25, Pie, 0), (28, Pie, 0), (31, Pie, 0), (34, MovN, 0), (36, Pie, 0), (39, L32iN, 0), (41, L32iN, 0), (43, And, 0), (46, S32iN, 0), (48, J, 12)];
    let mut data = vec![0x42u8, 0x00].repeat(40);
    data[50] = 0x43;
    // Variants: the plain kernel; CP3 disabled; the store landing in the last 16 bytes of
    // the mapping; loads running off the end of the mapping (slow, then a fault); a
    // read-only mapping (every fast store misses); the store into the region's own code
    // page followed by the internal edge back into the loop; the slow window; and an
    // occupied AR frame together with CP3 disabled, where the window overflow must win.
    let last16 = BASE + 65536 - 16;
    for (label, cp3, src, dst, readonly, occupied, turns, whole) in [
        ("uniform", 8, BASE + 0x1000, BASE + 0x2000, false, false, 600, true),
        ("uniform-cp3-off", 0, BASE + 0x1000, BASE + 0x2000, false, false, 40, false),
        ("uniform-last16", 8, BASE + 0x1000, last16, false, false, 300, true),
        ("uniform-off-end", 8, BASE + 65536 - 32, BASE + 0x2000, false, false, 40, false),
        ("uniform-readonly", 8, BASE + 0x1000, BASE + 0x2000, true, false, 40, false),
        // The 128-bit store lands in the region's own code page (past the program), so the
        // J edge after it must leave and the dispatcher must re-validate.
        ("uniform-self-modify", 8, BASE + 0x1000, BASE + 64, false, false, 300, false),
        ("uniform-slow", 8, SLOW, BASE + 0x2000, false, false, 300, false),
        ("uniform-overflow", 0, BASE + 0x1000, BASE + 0x2000, false, true, 40, false),
    ] {
        let max = region_program_on(label, &p, &uniform, &data, 9, 25, readonly, |c| {
            c.cpenable = cp3;
            if occupied { c.ps = ps::WOE; c.windowstart = (1 << c.windowbase) | (1 << ((c.windowbase + 2) % 16)); }
            c.set_ar(7, 0x0042_0042); c.set_ar(12, src); c.set_ar(13, dst);
        }, turns);
        assert!(!whole || max >= 20, "{label}: region retired at most {max} per call");
        cases += 1;
    }
    // The dot-product kernel of on-device inference (pocket-tank's 4-bit matmul): the ACCX reset,
    // 128-bit loads, signed 8- and 16-bit multiply-accumulate with and without their load, and the
    // RUR of ACCX that follows each dot product (without it the block would stay interpreted), in
    // a LOOPNEZ with an internal edge back. Variants: mixed lanes; lanes that drive ACCX into its
    // upper and lower saturation bound (starting near it, past the reset); CP3 disabled; a
    // read-only mapping, where loads stay on the fast path; loads running off the mapping; and
    // the slow window, where every load misses and re-executes in the interpreter.
    let mut dp = Vec::new();
    dp.extend(asm::pie("ee.zero.accx", &[]));                                                          // 0
    dp.extend(asm::movi_n(10, 3));                                                                     // 3
    dp.extend(asm::lp(9, BASE + 5, 10, BASE + 34));                                                    // 5 loopnez a10, 34
    dp.extend(asm::pie("ee.vld.128.ip", &[(Qu, 0), (As, 8), (Imm, 16)]));                              // 8
    dp.extend(asm::pie("ee.vld.128.ip", &[(Qu, 4), (As, 9), (Imm, 16)]));                              // 11
    dp.extend(asm::pie("ee.vmulas.s8.accx.ld.ip", &[(Qu, 5), (As, 9), (Imm, 16), (Qx, 0), (Qy, 4)]));  // 14
    dp.extend(asm::pie("ee.vmulas.s8.accx", &[(Qx, 0), (Qy, 5)]));                                     // 18
    dp.extend(asm::rur(11, 0));                                                                        // 21 rur.accx_0 a11
    dp.extend(asm::pie("ee.vmulas.s16.accx", &[(Qx, 4), (Qy, 5)]));                                    // 24
    dp.extend(asm::rur(15, 1));                                                                        // 27 rur.accx_1 a15
    dp.extend(asm::pie("ee.vmulas.s16.accx.ld.ip", &[(Qu, 1), (As, 8), (Imm, 16), (Qx, 0), (Qy, 5)])); // 30
    dp.extend(asm::mov_n(8, 12));                                                                      // 34
    dp.extend(asm::mov_n(9, 13));                                                                      // 36
    dp.extend(asm::j(BASE + 38, BASE + 3));                                                            // 38
    let dot = [(0, Pie, 0), (3, MoviN, 0), (5, Loopnez, 34), (8, Pie, 0), (11, Pie, 0), (14, Pie, 0), (18, Pie, 0),
               (21, Rur, 0), (24, Pie, 0), (27, Rur, 0), (30, Pie, 0), (34, MovN, 0), (36, MovN, 0), (38, J, 3)];
    let mixed: Vec<u8> = (0..0x100u32).map(|i| (i.wrapping_mul(0x9e37_79b9) >> 24) as u8).collect();
    let positive = vec![0x80u8; 0x100];                          // every product positive
    let negative = [vec![0x7fu8; 0x80], vec![0x80u8; 0x80]].concat(); // the a8 stream against the a9 stream: net negative
    let (near_high, near_low) = ((1i64 << 39) - 1 - 5_000_000, -(1i64 << 39) + 5_000_000);
    for (label, cp3, data, accx, src, readonly, turns, whole) in [
        ("dot", 8, &mixed, 0i64, BASE + 0x1000, false, 600, true),
        ("dot-saturate-high", 8, &positive, near_high, BASE + 0x1000, false, 300, true),
        ("dot-saturate-low", 8, &negative, near_low, BASE + 0x1000, false, 300, true),
        ("dot-cp3-off", 0, &mixed, 0, BASE + 0x1000, false, 40, false),
        ("dot-readonly", 8, &mixed, 0, BASE + 0x1000, true, 300, true),
        ("dot-off-end", 8, &mixed, 0, BASE + 65536 - 48, false, 40, false),
        ("dot-slow", 8, &mixed, 0, SLOW, false, 300, false),
    ] {
        let max = region_program_on(label, &dp, &dot, data, 3, 8, readonly, |c| {
            c.cpenable = cp3;
            c.accx = [accx as u32, ((accx >> 32) & 0xff) as u32];
            if accx != 0 { c.pc = BASE + 3; } // past the reset, so ACCX starts near its bound
            c.set_ar(8, src); c.set_ar(9, src + 0x80); c.set_ar(12, src); c.set_ar(13, src + 0x80);
        }, turns);
        assert!(!whole || max >= 20, "{label}: region retired at most {max} per call");
        cases += 1;
    }
    // Region formation itself for the tile scan: every static successor, nothing past RSR.
    let mut ram = Ram::new(true, false);
    ram.ram.mem[..tp.len()].copy_from_slice(&tp);
    let c = cpu(0);
    let head: Vec<BlockInsn> = (0..2).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, straddle: false, off: 0 }) }).collect();
    let formed = emitter::region::form(&c, &mut ram, BASE, &head, true).expect("tile region");
    assert_eq!(formed.chunks.iter().map(|c| (c.pc - BASE, c.instructions.len())).collect::<Vec<_>>(),
        vec![(0, 2), (35, 1), (6, 3), (41, 2), (13, 6), (43, 1)]);
    assert_eq!(formed.pages, vec![(0, 0)]);
    assert!(emitter::region::form(&c, &mut ram, BASE + 38, &head, true).is_none(), "RSR head");
    cases + 2
}
