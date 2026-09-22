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
    // A PS-writing leaf exits the region so interrupt/window proofs are rebuilt.
    // Count only region lowerings: hot standalone blocks cannot satisfy this check.
    for (op, word) in [(Rsil, 0x006030u32), (Wsr, 0x130000 | (crate::state::sr::PS << 8) | 0x30),
        (Xsr, 0x610000 | (crate::state::sr::PS << 8) | 0x30)] {
        let mut p = asm::addi_n(2, 2, 1);
        p.extend(asm::bz(1, BASE + 2, 2, BASE + 8));
        p.extend(asm::j(BASE + 5, BASE + 11));
        p.extend([word as u8, (word >> 8) as u8, (word >> 16) as u8]);
        p.extend(asm::movi_n(2, 0));
        p.extend(asm::j(BASE + 13, BASE));
        for flags in [0, ps::WOE] {
            let before = PS_REGION_TAKEN.load(std::sync::atomic::Ordering::Relaxed);
            region_program("PS-terminal-leaf", &p,
                &[(0, AddiN, 0), (2, Bnez, 8), (5, J, 11), (8, op, 0), (11, MoviN, 0), (13, J, 0)],
                &[], 2, 8, |c| {
                    c.ps = flags;
                    c.windowstart = 1 << c.windowbase;
                    c.set_ar(2, 0);
                    c.set_ar(3, flags);
                }, 600);
            assert!(PS_REGION_TAKEN.load(std::sync::atomic::Ordering::Relaxed) > before,
                "{op:?} PS={flags:x} must execute inline inside a region");
            cases += 1;
        }
    }
    // Forty non-contiguous chunks exercise a large br_table and five version pages.
    // Enter every chunk with both short credit and hundreds of instructions of credit.
    let mut large = vec![0; 40 * 32];
    for k in 0..40 {
        let mut chunk = Vec::new();
        for _ in 0..6 { chunk.extend(asm::addi_n(2, 2, 1)); }
        chunk.extend(asm::s8i(3, 4, 0));
        chunk.extend(asm::j(BASE + (k * 32 + 15) as u32, BASE + ((k + 1) % 40 * 32) as u32));
        large[k * 32..k * 32 + chunk.len()].copy_from_slice(&chunk);
    }
    {
        let mut ram = Ram::new(true, false);
        ram.ram.mem[..large.len()].copy_from_slice(&large);
        let c = cpu(0);
        let head: Vec<BlockInsn> = (0..8).scan(BASE, |pc, _| {
            let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap());
            *pc += i.len as u32;
            Some(BlockInsn { insn: i, max_ar: crate::exec::max_ar(&i), straddle: false, off: 0 })
        }).collect();
        let formed = emitter::region::form(&c, &mut ram, BASE, &head, true).expect("large region");
        assert_eq!(formed.chunks.len(), 40);
        assert_eq!(formed.chunks.iter().map(|c| c.instructions.len()).sum::<usize>(), 320);
        assert_eq!(formed.pages.len(), 5);
        let (bytes, sites) = emitter::region::generate(&formed.chunks, &formed.pages, &formed.loops, true);
        let slot = unsafe { host_jit_compile(bytes.as_ptr(), bytes.len()) };
        assert_ne!(slot, 0, "large region module with {} exit sites", sites.len());
        type Run = extern "C" fn(*mut Cpu, *mut Ram, *const Helpers, u32, u32, *const TlbEntry, *mut u32) -> u32;
        let f: Run = unsafe { std::mem::transmute(slot as usize) };
        for entry in 0..40 {
            for budget in [8, 15, 63, 300, 511] {
                for dst in [BASE + 0x2000, BASE + 4 * 256 + 31] {
                    let (mut a, mut b) = (cpu(0), cpu(0));
                    let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
                    for r in [&mut ra, &mut rb] { r.ram.mem[..large.len()].copy_from_slice(&large); }
                    for c in [&mut a, &mut b] {
                        c.pc = formed.chunks[entry].pc; c.ps = 0;
                        c.set_ar(3, 0x42); c.set_ar(4, dst);
                    }
                    CONTEXT.with(|c| *c.borrow_mut() = format!("large region entry {entry} budget {budget} dst {dst:x}"));
                    let fm = rb.fast_mem().unwrap();
                    let result = f(&mut b, &mut rb, &Helpers::new::<Ram>(), budget, entry as u32, fm.tlb, fm.page_ver);
                    let done = result & 0xffff;
                    assert!(done > 0 && done <= budget);
                    if dst == BASE + 0x2000 && budget >= 300 { assert!(done >= 296, "large DONE credit: {done}"); }
                    for _ in 0..done {
                        let i = crate::decode::decode(a.pc, ra.fetch(a.pc).unwrap());
                        exec_insn(&mut a, &mut ra, &i).unwrap();
                    }
                    same(&a, &b);
                    assert_eq!(ra.ram.mem, rb.ram.mem);
                    assert_eq!(ra.versions, rb.versions);
                    cases += 1;
                }
            }
        }
        unsafe { host_jit_release(slot) };
    }
    for dst in [BASE + 0x2000, BASE + 4 * 256 + 31] {
        let max = region_program("large-region-dispatch", &large, &[], &[], 8, 32, |c| {
            c.set_ar(3, 0x42); c.set_ar(4, dst);
        }, 1200);
        assert!(dst != BASE + 0x2000 || max > 8, "large region never passed its head");
        cases += 1;
    }
    // Put LEND inside the second chunk of the same large graph: formation must split
    // it and preserve the backedge even when many dispatch targets precede the split.
    let mut loop_large = large.clone();
    let mut prefix = asm::movi_n(10, 3);
    prefix.extend(asm::lp(9, BASE + 2, 10, BASE + 38));
    for _ in 0..6 { prefix.extend(asm::addi_n(2, 2, 1)); }
    prefix.extend(asm::s8i(3, 4, 0));
    prefix.extend(asm::j(BASE + 20, BASE + 32));
    loop_large[..prefix.len()].copy_from_slice(&prefix);
    {
        let mut ram = Ram::new(true, false);
        ram.ram.mem[..loop_large.len()].copy_from_slice(&loop_large);
        let head: Vec<BlockInsn> = (0..2).scan(BASE, |pc, _| {
            let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32;
            Some(BlockInsn { insn: i, max_ar: crate::exec::max_ar(&i), straddle: false, off: 0 })
        }).collect();
        let formed = emitter::region::form(&cpu(0), &mut ram, BASE, &head, true).expect("large loop region");
        assert!(formed.chunks.len() > 40);
        assert!(formed.chunks.iter().any(|c| c.pc == BASE + 38));
        assert_eq!(formed.loops, vec![(BASE + 38, BASE + 5)]);
        assert_eq!(formed.pages.len(), 5);
    }
    let max = region_program("large-loop-region", &loop_large, &[], &[], 2, 5, |c| {
        c.set_ar(3, 0x42); c.set_ar(4, BASE + 0x2000);
    }, 1200);
    assert!(max > 8, "large loop region never passed its head");
    cases += 1;
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
            vec![(0, 3), (7, 2), (17, 1), (13, 2), (15, 1)]);
        cases += 1;
    }
    // Calls and returns end chunks and leave; a function entry heads a region whose
    // window proof is redone after ENTRY; RETW.N uses its guarded inline path.
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
            vec![(12, 4), (22, 1), (24, 2)]);
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
    let mut data = [0x42u8, 0x00].repeat(40);
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
    // EX178: pocket-tank's q4 matmul group, the shape that lets the emitter hold ACCX in a
    // local: `ee.zero.accx` dominating a straight-line run of accumulates in one chunk, a
    // 128-bit store and an `ee.vld.128.ip` inside the run (neither reads the accumulator),
    // the s16 accumulate with its larger per-step bound, and the `rur.accx_0` that ends the
    // run by forcing the spill. Variants drive an exit from every position of the run: a
    // load fault, a slow-window miss that re-executes in the interpreter, a read-only
    // mapping, a store into the region's own code page (DIRTY), and CP3 disabled.
    // `a7` counts iterations into lane 0 of Q0, an accumulate operand, so every pass
    // produces a different ACCX: a run that wrongly kept the local across `rur.accx_0`,
    // a chunk edge or a miss would read the previous pass's value and be caught.
    // Twenty-seven NOPs put instruction 32 (the chunk length limit) in the middle of the
    // run, so the region's internal edge cuts it and the held accumulator has to be
    // written back on an edge that does not leave.
    let mut q4 = Vec::new();
    q4.extend(asm::addi_n(7, 7, 1));                                                                   // 0
    for _ in 0..27 { q4.extend(asm::nop_n()); }                                                        // 2..56
    q4.extend(asm::pie("ee.movi.32.q", &[(Qu, 0), (As, 7), (Sel, 0)]));                                // 56
    q4.extend(asm::pie("ee.zero.accx", &[]));                                                          // 59
    q4.extend(asm::pie("ee.vld.128.ip", &[(Qu, 4), (As, 8), (Imm, 16)]));                              // 62
    q4.extend(asm::pie("ee.vmulas.s8.accx.ld.ip", &[(Qu, 5), (As, 8), (Imm, 16), (Qx, 0), (Qy, 4)]));  // 65
    q4.extend(asm::pie("ee.vmulas.s8.accx.ld.ip", &[(Qu, 4), (As, 8), (Imm, 16), (Qx, 1), (Qy, 5)]));  // 69 chunk 1 head
    q4.extend(asm::pie("ee.vld.128.ip", &[(Qu, 7), (As, 8), (Imm, 16)]));                              // 73
    q4.extend(asm::pie("ee.vmulas.s8.accx.ld.ip", &[(Qu, 5), (As, 8), (Imm, 16), (Qx, 2), (Qy, 7)]));  // 76
    q4.extend(asm::pie("ee.vmulas.s8.accx", &[(Qx, 2), (Qy, 4)]));                                     // 80
    q4.extend(asm::pie("ee.vst.128.ip", &[(Qv, 3), (As, 10), (Imm, 16)]));                             // 83
    q4.extend(asm::pie("ee.vmulas.s16.accx", &[(Qx, 0), (Qy, 4)]));                                    // 86
    q4.extend(asm::rur(11, 0));                                                                        // 89 rur.accx_0 a11
    q4.extend(asm::pie("ee.zero.accx", &[]));                                                          // 92
    q4.extend(asm::pie("ee.vmulas.s16.accx.ld.ip", &[(Qu, 6), (As, 9), (Imm, 16), (Qx, 0), (Qy, 1)])); // 95
    q4.extend(asm::rur(15, 1));                                                                        // 99 rur.accx_1 a15
    q4.extend(asm::mov_n(8, 12));                                                                      // 102
    q4.extend(asm::mov_n(9, 13));                                                                      // 104
    q4.extend(asm::mov_n(10, 14));                                                                     // 106
    q4.extend(asm::j(BASE + 108, BASE));                                                               // 108
    let group = [(0, AddiN, 0), (56, Pie, 0), (59, Pie, 0), (62, Pie, 0), (65, Pie, 0), (69, Pie, 0), (73, Pie, 0),
                 (76, Pie, 0), (80, Pie, 0), (83, Pie, 0), (86, Pie, 0), (89, Rur, 0), (92, Pie, 0), (95, Pie, 0),
                 (99, Rur, 0), (102, MovN, 0), (108, J, 0)];
    for (label, cp3, data, src, dst, readonly, turns, whole) in [
        ("q4", 8, &mixed, BASE + 0x1000, BASE + 0x2000, false, 600, true),
        ("q4-extreme", 8, &positive, BASE + 0x1000, BASE + 0x2000, false, 600, true),
        ("q4-cp3-off", 0, &mixed, BASE + 0x1000, BASE + 0x2000, false, 40, false),
        ("q4-readonly", 8, &mixed, BASE + 0x1000, BASE + 0x2000, true, 300, false),
        // The coalesced range leaves the mapping: the shared probe fails and the
        // per-access copy must fault on exactly the access that leaves it.
        ("q4-off-end", 8, &mixed, BASE + 65536 - 48, BASE + 0x2000, false, 40, false),
        ("q4-off-end2", 8, &mixed, BASE + 65536 - 64, BASE + 0x2000, false, 40, false),
        // The base is not 16-aligned: every address is masked but the post-increments
        // are not, so the coalesced copy must keep the low bits of the base register.
        ("q4-unaligned", 8, &mixed, BASE + 0x1000 + 5, BASE + 0x2000, false, 300, false),
        ("q4-slow", 8, &mixed, SLOW, BASE + 0x2000, false, 300, false),
        // Past the 111-byte program but inside its version page: DIRTY, no rewritten code.
        ("q4-self-modify", 8, &mixed, BASE + 0x1000, BASE + 128, false, 300, false),
    ] {
        let max = region_program_on(label, &q4, &group, data, 32, 69, readonly, |c| {
            c.cpenable = cp3;
            c.set_ar(7, 0x0100_0000);
            c.set_ar(8, src); c.set_ar(9, src + 0x80); c.set_ar(10, dst);
            c.set_ar(12, src); c.set_ar(13, src + 0x80); c.set_ar(14, dst);
        }, turns);
        assert!(!whole || max >= 40, "{label}: region retired at most {max} per call");
        cases += 1;
    }
    // EX155: the 4-bit weight unpack of pocket-tank's matmul: WUR/RUR SAR_BYTE, the byte shift
    // across two Q registers (every count 0..15, with and without QUP, destination aliasing either
    // source), 32-bit lane shifts for SAR 0..32, and saturating/min/max lane arithmetic.
    let mut up = Vec::new();
    up.extend(asm::pie("ee.vld.128.ip", &[(Qu, 0), (As, 8), (Imm, 16)]));   // 0
    up.extend(asm::pie("ee.vld.128.ip", &[(Qu, 1), (As, 8), (Imm, 16)]));   // 3
    up.extend(asm::addi_n(11, 11, 1));                                      // 6
    up.extend(asm::wur(11, 13));                                            // 8
    up.extend(asm::shift_setup(1, 11));                                     // 11 ssl: SAR 1..32
    up.extend(asm::pie("ee.src.q", &[(Qa, 2), (Qs0, 0), (Qs1, 1)]));        // 14
    up.extend(asm::pie("ee.src.q.qup", &[(Qa, 3), (Qs0, 0), (Qs1, 1)]));    // 17
    up.extend(asm::pie("ee.vsr.32", &[(Qa, 4), (Qs, 2)]));                  // 20
    up.extend(asm::pie("ee.vsl.32", &[(Qa, 5), (Qs, 3)]));                  // 23
    up.extend(asm::shift_setup(0, 11));                                     // 26 ssr: SAR 0..31
    up.extend(asm::pie("ee.vsr.32", &[(Qa, 6), (Qs, 2)]));                  // 29
    up.extend(asm::pie("ee.vsl.32", &[(Qa, 7), (Qs, 3)]));                  // 32
    up.extend(asm::pie("ee.vsubs.s8", &[(Qa, 4), (Qx, 4), (Qy, 5)]));       // 35
    up.extend(asm::pie("ee.vadds.s8", &[(Qa, 5), (Qx, 6), (Qy, 2)]));       // 38
    up.extend(asm::pie("ee.vsubs.s16", &[(Qa, 6), (Qx, 6), (Qy, 3)]));      // 41
    up.extend(asm::pie("ee.vadds.s16", &[(Qa, 7), (Qx, 7), (Qy, 2)]));      // 44
    up.extend(asm::pie("ee.vmin.s8", &[(Qa, 2), (Qx, 4), (Qy, 5)]));        // 47
    up.extend(asm::pie("ee.vmax.s8", &[(Qa, 3), (Qx, 4), (Qy, 5)]));        // 50
    up.extend(asm::pie("ee.vmin.s16", &[(Qa, 4), (Qx, 6), (Qy, 7)]));       // 53
    up.extend(asm::pie("ee.vmax.s16", &[(Qa, 5), (Qx, 6), (Qy, 7)]));       // 56
    up.extend(asm::pie("ee.vmin.s32", &[(Qa, 6), (Qx, 2), (Qy, 3)]));       // 59
    up.extend(asm::pie("ee.vmax.s32", &[(Qa, 7), (Qx, 2), (Qy, 3)]));       // 62
    up.extend(asm::pie("ee.src.q.qup", &[(Qa, 0), (Qs0, 0), (Qs1, 1)]));    // 65 Qa is Qs0
    up.extend(asm::pie("ee.src.q.qup", &[(Qa, 1), (Qs0, 0), (Qs1, 1)]));    // 68 Qa is Qs1
    up.extend(asm::pie("ee.src.q", &[(Qa, 1), (Qs0, 1), (Qs1, 0)]));        // 71
    up.extend(asm::rur(14, 13));                                            // 74
    up.extend(asm::mov_n(8, 12));                                           // 77
    up.extend(asm::j(BASE + 79, BASE));                                     // 79
    let unpack = [(0, Pie, 0), (6, AddiN, 0), (8, Wur, 0), (11, Ssl, 0), (14, Pie, 0), (17, Pie, 0), (26, Ssr, 0), (35, Pie, 0), (65, Pie, 0),
                  (74, Rur, 0), (77, MovN, 0), (79, J, 0)];
    let edges: Vec<u8> = (0..0x100u32).map(|i| [0x80u8, 0x7f, 0xff, 0x00, 0x01, 0x81][(i as usize * 7 + i as usize / 16) % 6]).collect();
    for (label, cp3, data, turns) in [("unpack", 8, &mixed, 900), ("unpack-edges", 8, &edges, 900), ("unpack-cp3-off", 0, &mixed, 40)] {
        let max = region_program_on(label, &up, &unpack, data, 28, 14, false, |c| {
            c.cpenable = cp3;
            c.set_ar(8, BASE + 0x1000); c.set_ar(12, BASE + 0x1000); c.set_ar(11, 0xffff_fff0);
        }, turns);
        assert!(cp3 == 0 || max >= 20, "{label}: region retired at most {max} per call");
        cases += 1;
    }
    // Region formation itself for the tile scan: every static successor, nothing past RSR.
    let mut ram = Ram::new(true, false);
    ram.ram.mem[..tp.len()].copy_from_slice(&tp);
    let c = cpu(0);
    let head: Vec<BlockInsn> = (0..2).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, straddle: false, off: 0 }) }).collect();
    let formed = emitter::region::form(&c, &mut ram, BASE, &head, true).expect("tile region");
    assert_eq!(formed.chunks.iter().map(|c| (c.pc - BASE, c.instructions.len())).collect::<Vec<_>>(),
        vec![(0, 2), (6, 3), (35, 1), (13, 6), (41, 2), (43, 1)]);
    assert_eq!(formed.pages, vec![(0, 0)]);
    assert!(emitter::region::form(&c, &mut ram, BASE + 38, &head, true).is_none(), "RSR head");
    cases + 2 + prev_page_store() + forward_edges() + self_loops()
}

/// EX181 s2: the two shapes whose backedge stays inside one chunk — a `bnez` back to the
/// chunk head, and a hardware loop body that is exactly one chunk after the LEND split.
/// Both are wrapped in a WASM loop, so their backedge is a `br` instead of a br_table hop;
/// the harness cuts credit at every index and probes the head.
fn self_loops() -> u32 {
    use Op::*;
    let mut p = Vec::new();
    p.extend(asm::movi_n(3, 5));                 // 0
    p.extend(asm::addi_n(2, 2, 1));              // 2  the self-looping chunk starts here
    p.extend(asm::addi_n(3, 3, -1));             // 4
    p.extend(asm::bz(1, BASE + 6, 3, BASE + 2)); // 6  bnez a3, 2
    p.extend(asm::addi_n(6, 6, 1));              // 9
    p.extend(asm::j(BASE + 11, BASE));           // 11
    let shape = [(0, MoviN, 0), (2, AddiN, 0), (4, AddiN, 0), (6, Bnez, 2), (9, AddiN, 0), (11, J, 0)];
    {
        let mut ram = Ram::new(true, false);
        ram.ram.mem[..p.len()].copy_from_slice(&p);
        let head: Vec<BlockInsn> = (0..4).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, straddle: false, off: 0 }) }).collect();
        let formed = emitter::region::form(&cpu(0), &mut ram, BASE, &head, true).expect("bnez self-loop region");
        assert_eq!(formed.chunks.iter().map(|c| (c.pc - BASE, c.instructions.len())).collect::<Vec<_>>(), vec![(0, 4), (9, 2), (2, 3)]);
    }
    let before = emitter::region::SELF_LOOP_BRANCHES.load(std::sync::atomic::Ordering::Relaxed);
    let max = region_program("bnez-self-loop", &p, &shape, &[], 4, 2, |_| {}, 900);
    assert!(max > 4, "bnez self-loop region never passed its head ({max})");
    assert!(emitter::region::SELF_LOOP_BRANCHES.load(std::sync::atomic::Ordering::Relaxed) > before, "bnez self-loop emitted no direct backedge");

    let mut p = Vec::new();
    p.extend(asm::lp(9, BASE, 3, BASE + 9));        // 0  loopnez a3, 9
    p.extend(asm::addi_n(2, 2, 1));                 // 3  LBEG: the whole body is one chunk
    p.extend(asm::addi_n(4, 4, 1));                 // 5
    p.extend(asm::addi_n(5, 5, 1));                 // 7  ends exactly at LEND
    p.extend(asm::addi_n(6, 6, 1));                 // 9  loop exit
    p.extend(asm::j(BASE + 11, BASE));              // 11
    let shape = [(0, Loopnez, 9), (3, AddiN, 0), (5, AddiN, 0), (7, AddiN, 0), (9, AddiN, 0), (11, J, 0)];
    {
        let mut ram = Ram::new(true, false);
        ram.ram.mem[..p.len()].copy_from_slice(&p);
        let head: Vec<BlockInsn> = (0..1).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, straddle: false, off: 0 }) }).collect();
        let formed = emitter::region::form(&cpu(0), &mut ram, BASE, &head, true).expect("hardware self-loop region");
        assert_eq!(formed.loops, vec![(BASE + 9, BASE + 3)]);
        assert_eq!(formed.chunks.iter().map(|c| (c.pc - BASE, c.instructions.len())).collect::<Vec<_>>(), vec![(0, 1), (3, 3), (9, 2)]);
    }
    // a3 is never written, so the count is the same on every pass; 0 makes LOOPNEZ skip.
    for count in [4, 1, 0] {
        let before = emitter::region::SELF_LOOP_BRANCHES.load(std::sync::atomic::Ordering::Relaxed);
        let max = region_program("hw-self-loop", &p, &shape, &[], 1, 3, move |c| { c.set_ar(3, count); }, 900);
        assert!(max > 1, "hardware self-loop region never passed its head ({max})");
        assert!(emitter::region::SELF_LOOP_BRANCHES.load(std::sync::atomic::Ordering::Relaxed) > before, "hardware self-loop emitted no direct backedge (count {count})");
    }
    2
}

/// EX181: a graph whose internal forward edges skip chunks, so the emitted `br` labels are
/// not all zero: chunk 2's taken target is chunk 4 and its fallthrough is chunk 5. Two bits
/// of a counter pick the path, so every edge runs; `j 0` keeps the backward edges on the
/// dispatch table.
fn forward_edges() -> u32 {
    use Op::*;
    let mut p = Vec::new();
    p.extend(asm::addi_n(10, 10, 1));            // 0
    p.extend(asm::and(11, 10, 12));              // 2  a11 = a10 & 1
    p.extend(asm::bz(0, BASE + 5, 11, BASE + 23)); // 5  beqz a11, 23
    p.extend(asm::and(11, 10, 13));              // 8  a11 = a10 & 2
    p.extend(asm::bz(0, BASE + 11, 11, BASE + 32)); // 11 beqz a11, 32
    p.extend(asm::addi_n(4, 4, 1));              // 14
    p.extend(asm::j(BASE + 16, BASE + 46));      // 16
    p.extend(asm::nop_n());                      // 19 (never executed)
    p.extend(asm::nop_n());                      // 21
    p.extend(asm::addi_n(5, 5, 1));              // 23
    p.extend(asm::addi_n(5, 5, 1));              // 25
    p.extend(asm::j(BASE + 27, BASE + 39));      // 27
    p.extend(asm::nop_n());                      // 30
    p.extend(asm::addi_n(6, 6, 1));              // 32
    p.extend(asm::j(BASE + 34, BASE + 46));      // 34
    p.extend(asm::nop_n());                      // 37
    p.extend(asm::addi_n(7, 7, 1));              // 39
    p.extend(asm::j(BASE + 41, BASE));           // 41
    p.extend(asm::nop_n());                      // 44
    p.extend(asm::and(9, 10, 12));               // 46
    p.extend(asm::bz(0, BASE + 49, 9, BASE));    // 49 beqz a9, 0
    p.extend(asm::addi_n(8, 8, 1));              // 52
    p.extend(asm::j(BASE + 54, BASE));           // 54
    let shape = [(0, AddiN, 0), (2, And, 0), (5, Beqz, 23), (8, And, 0), (11, Beqz, 32), (14, AddiN, 0), (16, J, 46),
                 (23, AddiN, 0), (27, J, 39), (32, AddiN, 0), (34, J, 46), (39, AddiN, 0), (41, J, 0),
                 (46, And, 0), (49, Beqz, 0), (52, AddiN, 0), (54, J, 0)];
    {
        let mut ram = Ram::new(true, false);
        ram.ram.mem[..p.len()].copy_from_slice(&p);
        let head: Vec<BlockInsn> = (0..3).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, straddle: false, off: 0 }) }).collect();
        let formed = emitter::region::form(&cpu(0), &mut ram, BASE, &head, true).expect("forward-edge region");
        // Breadth first from the head: fallthrough, then taken target (EX181 s3).
        assert_eq!(formed.chunks.iter().map(|c| c.pc - BASE).collect::<Vec<_>>(), vec![0, 8, 23, 14, 32, 39, 46, 52]);
    }
    let before = emitter::region::FORWARD_BRANCHES.load(std::sync::atomic::Ordering::Relaxed);
    let max = region_program("forward-edges", &p, &shape, &[], 3, 8, |c| {
        c.set_ar(12, 1); c.set_ar(13, 2);
    }, 1200);
    assert!(max > 3, "forward-edge region never passed its head ({max})");
    assert!(emitter::region::FORWARD_BRANCHES.load(std::sync::atomic::Ordering::Relaxed) > before, "forward-edge region emitted no direct forward branch");
    1
}

/// EX180: a region whose code lives entirely in page 0 stores into page 1 at offset zero.
/// The bumped page is outside the region's page range, so `region_store_check` does not
/// see it, but the bus also bumps page 0 — a code page of this very region. Both engines
/// must record the same pages and retire the same instructions.
fn prev_page_store() -> u32 {
    use Op::*;
    let mut p = Vec::new();
    for _ in 0..6 { p.extend(asm::addi_n(2, 2, 1)); }        // 0..12
    p.extend(asm::s8i(3, 4, 0));                             // 12: into page 1, offset 0
    p.extend(asm::j(BASE + 15, BASE + 18));                  // 15: edge to the next chunk
    for _ in 0..4 { p.extend(asm::addi_n(5, 5, 1)); }         // 18..26
    p.extend(asm::j(BASE + 26, BASE));                       // 26: back to the head
    let shape = [(0, AddiN, 0), (12, S8i, 0), (15, J, 18), (18, AddiN, 0), (26, J, 0)];
    let mut cases = 0;
    for off in [0u32, 1, 2, 3] {
        let max = region_program("prev-page-store", &p, &shape, &[], 8, 18, |c| {
            c.set_ar(3, 0x5a);
            c.set_ar(4, BASE + 0x100 + off);
        }, 300);
        assert!(max > 8, "prev-page-store: region never passed its head ({max})");
        cases += 1;
    }
    cases
}
