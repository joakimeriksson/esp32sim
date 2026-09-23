use super::*;

pub(super) fn hardware_loops() -> u32 {
    // A load/store prefix ending before the decoded block end, like panel transport.
    // The trailing ADD must run once after LCOUNT expires, never on a backedge.
    let mut block = [insn(Op::L32i), insn(Op::S32i), insn(Op::Add)];
    block[0].insn.imm = 0;
    block[1].insn.imm = 0;
    block[1].insn.s = 6;
    let mut cases = 0;
    for lcount in [0, 1, 2, 9] {
        for entry in 0..3 {
            for budget in 0..=25 {
                compare(&mut block, Case { seed: 15, entry, budget, fast: true, ..Case::default() }, |c| {
                        c.set_ar(4, BASE + 0x1000);
                        c.set_ar(6, BASE + 0x2000);
                        c.lbeg = BASE; c.lend = BASE + 6; c.lcount = lcount;
                    });
                cases += 1;
            }
        }
    }
    // The next iteration can leave fast RAM and fault; completed iterations, spills
    // and the slow fault's PC must all agree with instruction-by-instruction execution.
    let mut crossing = [insn(Op::L32i), insn(Op::Addi), insn(Op::S32i), insn(Op::Addi)];
    crossing[0].insn.imm = 0;
    crossing[1].insn.t = 4; crossing[1].insn.imm = 4;
    crossing[2].insn.s = 6; crossing[2].insn.imm = 0;
    crossing[3].insn.s = 6; crossing[3].insn.t = 6; crossing[3].insn.imm = 4;
    for bi in &mut crossing { bi.max_ar = crate::exec::max_ar(&bi.insn); }
    for entry in 0..4 {
        for budget in 1..=20 {
            compare(&mut crossing, Case { seed: 15, entry, budget, fast: true, ..Case::default() }, |c| {
                    c.set_ar(4, BASE + 65_532); c.set_ar(6, BASE + 0x2000);
                    c.lbeg = BASE; c.lend = BASE + 12; c.lcount = 9;
                });
            cases += 1;
        }
    }
    // Implicit CALL return registers are not initialized until the suffix executes.
    // A cut or return at a hardware backedge must not spill those uninitialized locals.
    for call in [Op::Call4, Op::Call8, Op::Call12] {
        let mut suffix = [insn(Op::Add), insn(Op::MovN), insn(call)];
        suffix[2].insn.imm = (BASE + 0x100) as i32;
        for budget in 1..=10 {
            compare(&mut suffix, Case { budget, ..Case::default() }, |c| { c.lbeg = BASE; c.lend = BASE + 6; c.lcount = 2; });
            cases += 1;
        }
    }
    // SR-writing suffixes compile, but cannot retain a hardware-loop prefix: the
    // helper could overwrite LCOUNT or change which instruction takes a backedge.
    for op in [Op::Wsr, Op::Xsr] {
        for number in [crate::state::sr::LBEG, crate::state::sr::LEND, crate::state::sr::LCOUNT] {
            let mut block = [insn(Op::Add), insn(Op::MovN), insn(op)];
            block[2].insn.imm = number as i32;
            let mut cc = CodeCache::new(0).unwrap();
            let code = compile(&mut cc, &mut block, BASE, true).expect("SR suffix compiles");
            assert_eq!(cc.blocks[code as usize].loop_prefix, 0, "loop state written by helper");
            cases += 1;
        }
    }
    // Review spike: LOOP* suffixes compile, but such a block never gets a retained prefix.
    for op in [Op::Loop, Op::Loopnez, Op::Loopgtz] {
        let mut block = [insn(Op::Add), insn(Op::MovN), insn(op)];
        block[2].insn.imm = (BASE + 0x40) as i32; // LEND target
        block[2].insn.s = 6;
        block[2].max_ar = crate::exec::max_ar(&block[2].insn);
        let mut cc = CodeCache::new(0).unwrap();
        let code = compile(&mut cc, &mut block, BASE, true).expect("LOOP suffix compiles");
        assert_eq!(cc.blocks[code as usize].loop_prefix, 0, "LOOP suffix must not be a retained prefix");
        for count in [0u32, 1, 2, 0x8000_0000, u32::MAX] {
            for entry in 0..3 {
                for budget in 1..=3 {
                    for loop_end in [false, true] {
                        compare(&mut block, Case { seed: 9, entry, budget, fast: true, loop_end, ..Case::default() }, |c| { c.set_ar(6, count); });
                        cases += 1;
                    }
                }
            }
        }
    }
    // Stores to either code page must stop at the first loop end, including aliases.
    // An observer head also stops there; a slow access exits after that instruction.
    let mut cc = CodeCache::new(0).unwrap();
    let code = queue(&mut cc, &mut block, BASE, true);
    for _ in 0..HOT { ready(&cc, code, BASE + 6); }
    for (destination, observed, fast, expected) in [
        (BASE + 0x2000, false, true, 21),
        (BASE + 0x2000, true, true, 2),
        (BASE + 32, false, true, 2),
        (BASE + 0x2000, false, false, 1),
    ] {
        let mut c = cpu(0);
        let mut ram = Ram::new(fast, false);
        c.set_ar(4, BASE + 0x1000); c.set_ar(6, destination);
        c.lbeg = BASE; c.lend = BASE + 6; c.lcount = 9;
        if observed { c.boundary_bloom = emu_core::core::pc_bit(BASE); }
        let fm = ram.fast_mem();
        let result = unsafe { run(&cc, code, &mut c, &mut ram, &Helpers::new::<Ram>(), 100, 0, fm) };
        assert_eq!(result & 0xffff, expected, "hardware loop admission/exit");
        cases += 1;
    }
    // shell-s1: LEND moves between dispatches of one block. The memoised retained-loop answer
    // must agree with a fresh cache every time, and a rebuild reusing the block forgets it.
    let run_at = |cc: &CodeCache, code: u32, lend: u32| {
        let mut c = cpu(0);
        let mut ram = Ram::new(true, false);
        c.set_ar(4, BASE + 0x1000); c.set_ar(6, BASE + 0x2000);
        c.lbeg = BASE; c.lend = lend; c.lcount = 9;
        let fm = ram.fast_mem();
        let result = unsafe { run(cc, code, &mut c, &mut ram, &Helpers::new::<Ram>(), 100, 0, fm) };
        (result, c, ram)
    };
    for lend in [BASE + 6, BASE + 3, BASE + 0x40, BASE + 6, BASE + 9, BASE + 3] {
        let mut fresh = CodeCache::new(0).unwrap();
        let f = queue(&mut fresh, &mut block, BASE, true);
        for _ in 0..HOT { ready(&fresh, f, 0); }
        let (want, a, ra) = run_at(&fresh, f, lend);
        let (got, b, rb) = run_at(&cc, code, lend);
        assert_eq!(got, want, "memoised loop length at LEND {lend:#x}");
        same(&a, &b);
        assert_eq!((ra.versions, ra.noted), (rb.versions, rb.noted));
        cases += 1;
    }
    assert_ne!(cc.recs[code as usize].looped.get(), LOOP_UNKNOWN);
    assert_eq!(queue(&mut cc, &mut block, BASE, true), code);
    assert_eq!(cc.recs[code as usize].looped.get(), LOOP_UNKNOWN, "a rebuild must forget the loop answer");
    // Attaching/removing a block observer affects an already published module without
    // flushing it; ordinary JIT execution stays active while repeated callbacks stop.
    let mut c = cpu(0);
    for observed in [true, false, true] {
        let mut ram = Ram::new(true, false);
        c.pc = BASE;
        c.set_ar(4, BASE + 0x1000); c.set_ar(6, BASE + 0x2000);
        c.lbeg = BASE; c.lend = BASE + 6; c.lcount = 9;
        crate::Core::set_block_observation(&mut c, observed);
        let fm = ram.fast_mem();
        let result = unsafe { run(&cc, code, &mut c, &mut ram, &Helpers::new::<Ram>(), 100, 0, fm) };
        assert_eq!(result & 0xffff, if observed { 2 } else { 21 });
        assert_eq!(c.blocks.clone().observed, observed);
        c.blocks.flush();
        assert_eq!(c.blocks.observed, observed);
        cases += 1;
    }
    #[cfg(feature = "wasm-jit-profile")]
    assert!(c.blocks.profile.report().contains("[wasm-loop] pc=40370000 calls=1 retained_backedges=9"));
    // Packed return counts remain bounded even when a direct caller offers > u16::MAX.
    let mut c = cpu(0);
    let mut ram = Ram::new(true, false);
    c.set_ar(4, BASE + 0x1000); c.set_ar(6, BASE + 0x2000);
    c.lbeg = BASE; c.lend = BASE + 6; c.lcount = 50_000;
    let fm = ram.fast_mem();
    let result = unsafe { run(&cc, code, &mut c, &mut ram, &Helpers::new::<Ram>(), 65_536, 0, fm) };
    assert_eq!(result & 0xffff, 65_535);
    assert_eq!(c.lcount, 50_000 - 32_767);
    assert_eq!(ram.versions[32], 32_767);
    cases += 1;
    // A suffix branch to LBEG is not another hardware backedge. Its own PC must
    // be attributed, even though its destination matches the hardware loop head.
    block[2].insn.op = Op::J;
    block[2].insn.imm = BASE as i32;
    let code = queue(&mut cc, &mut block, BASE, true);
    for _ in 0..HOT { ready(&cc, code, BASE + 6); }
    for budget in 1..=10 {
        let mut c = cpu(0);
        let mut ram = Ram::new(true, false);
        c.set_ar(4, BASE + 0x1000); c.set_ar(6, BASE + 0x2000);
        c.lbeg = BASE; c.lend = BASE + 6; c.lcount = 2;
        let fm = ram.fast_mem();
        let result = unsafe { run(&cc, code, &mut c, &mut ram, &Helpers::new::<Ram>(), budget, 0, fm) };
        let done = budget.min(7);
        assert_eq!(result & 0xffff, done);
        assert_eq!(ram.noted, if done == 7 { BASE + 6 } else { BASE + ((done - 1) % 2) * 3 });
        cases += 1;
    }
    // A block straddling version pages must check the second page as well.
    block[2].insn.op = Op::Add;
    let start = BASE + 254;
    let code = queue(&mut cc, &mut block, start, true);
    for _ in 0..HOT { ready(&cc, code, start + 6); }
    let mut c = cpu(0);
    let mut ram = Ram::new(true, false);
    c.pc = start; c.lbeg = start; c.lend = start + 6; c.lcount = 9;
    c.set_ar(4, BASE + 0x1000); c.set_ar(6, BASE + 0x110);
    let fm = ram.fast_mem();
    let result = unsafe { run(&cc, code, &mut c, &mut ram, &Helpers::new::<Ram>(), 100, 0, fm) };
    assert_eq!(result & 0xffff, 2);
    assert_eq!(ram.versions[0], 0); assert_eq!(ram.versions[1], 1);
    cases + 1
}

pub(super) fn hardware_loop_scheduler() {
    // l32i.n a3,a4,0; s32i.n a3,a5,0; addi.n a4,a4,4;
    // addi.n a5,a5,4; j self. Hardware loop ends immediately before J.
    let programs: &[(&[u8], u8, u8, u32)] = &[
        (&[0x38, 0x04, 0x39, 0x05, 0x4b, 0x44, 0x4b, 0x55, 0x06, 0xff, 0xff], 4, 5, 8),
        // Actual mixed-width panel prefix from 0x40383349: l16ui/addi.n/s16i/addi.n.
        (&[0xb2, 0x1a, 0, 0x2b, 0xaa, 0xb2, 0x59, 0, 0x2b, 0x99, 0x06, 0xff, 0xff], 10, 9, 10),
    ];
    for &(program, source, destination, span) in programs {
        let (mut a, mut b) = (cpu(0), cpu(0));
        let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
        ra.ram.mem[..program.len()].copy_from_slice(program);
        rb.ram.mem[..program.len()].copy_from_slice(program);
        let mut repeated = false;
        for turn in 0..160 {
            for c in [&mut a, &mut b] {
                c.pc = BASE;
                c.lbeg = BASE; c.lend = BASE + span; c.lcount = 100;
                c.set_ar(source, BASE + 0x1000); c.set_ar(destination, BASE + 0x2000);
            }
            let budget = 1 + turn % 31;
            let (done, trap) = crate::block::run_block(&mut b, &mut rb, budget);
            assert!(trap.is_none()); assert!(done <= budget);
            repeated |= done > 5;
            for _ in 0..done { ra.note_pc(a.pc); crate::step(&mut a, &mut ra).unwrap(); }
            same(&a, &b);
            assert_eq!(ra.ram.mem, rb.ram.mem);
            assert_eq!(ra.versions, rb.versions);
            assert_eq!(ra.noted, rb.noted);
            // Consume a cut continuation without resetting PC, exercising arena resume lookup.
            let (done, trap) = crate::block::run_block(&mut b, &mut rb, 3);
            assert!(trap.is_none());
            for _ in 0..done { ra.note_pc(a.pc); crate::step(&mut a, &mut ra).unwrap(); }
            same(&a, &b);
        }
        assert!(repeated, "hardware loop did not retire multiple iterations in one dispatch");
        // A hot loop must stop at every timer position, including a wrapping CCOUNT.
        for distance in 1..=17 {
            for c in [&mut a, &mut b] {
                c.pc = BASE; c.lcount = 100; c.ccount = u32::MAX - 8;
                c.ccompare = [c.ccount.wrapping_add(distance), 0, 0];
                c.refresh_event();
                c.interrupt = 0; c.intenable = 0;
            }
            let (done, trap) = crate::block::run_block(&mut b, &mut rb, 32);
            // CCOMPARE1/2=0 may cut earlier across wrap too.
            let expected = distance.min(9);
            assert_eq!(done, expected); assert!(trap.is_none());
            for _ in 0..done { ra.note_pc(a.pc); crate::step(&mut a, &mut ra).unwrap(); }
            same(&a, &b);
            assert_eq!(a.interrupt, b.interrupt);
        }
    }
}

/// store-s1: store-run loop bodies (the render span fill and the ROM memset) retire whole
/// iterations in bulk only at the loop head of an admitted loop, into one writable mapping,
/// moving exactly the page versions the stores would (the previous page's for stores in a
/// page's first bytes) and never over the looping block's own code pages (page 0 here, or the
/// page after it through the previous-page rule, which the runs from 0x40 and 0x104 never leave). A read-only mapping, a misaligned pointer, a
/// range past the mapping end, a resume inside the body, another loop's LEND or LBEG and short
/// credit all take the ordinary body, with identical memory, versions, LCOUNT and counts.
pub(super) fn store_runs() -> u32 {
    use std::sync::atomic::Ordering::Relaxed;
    // review B1: a sparse store (offset 32 of a 64-byte step) is declined before any shift, even with overflow checks.
    let mut sparse = [insn(Op::S8i), insn(Op::Addi)];
    sparse[0].insn.imm = 32;
    sparse[0].insn.t = 6;
    sparse[1].insn.t = 4;
    sparse[1].insn.imm = 64;
    for bi in &mut sparse { bi.max_ar = crate::exec::max_ar(&bi.insn); }
    compare_hinted(&mut sparse, Case { seed: 23, budget: 8, fast: true, unwatched: true, ..Case::default() },
        &|c: &mut Cpu| { c.set_ar(4, BASE + 0x1000); c.lbeg = BASE; c.lend = BASE + 6; c.lcount = 4; }, BASE + 6);
    let mut span = [insn(Op::S16i), insn(Op::AddiN), insn(Op::Add)];
    span[0].insn.imm = 0;
    span[0].insn.t = 6;
    span[1].insn.r = 4;
    span[1].insn.imm = 2;
    let mut memset = [insn(Op::S32i), insn(Op::S32i), insn(Op::S32i), insn(Op::S32i), insn(Op::Addi), insn(Op::Add)];
    for (bi, off) in memset.iter_mut().zip([4, 0, 12, 8]) { bi.insn.imm = off; bi.insn.t = 6; }
    memset[4].insn.t = 4;
    memset[4].insn.imm = 16;
    let mut cases = 0;
    let (mut taken, mut watched) = (0, 0);
    for (block, lend, width, stride) in [(&mut span[..], BASE + 6, 2, 2), (&mut memset[..], BASE + 15, 4, 16)] {
        for bi in block.iter_mut() { bi.max_ar = crate::exec::max_ar(&bi.insn); }
        for (off, lcount) in [(0x1000, 40), (0x1000, 3), (0x1000, 1), (0x1000, u32::MAX), (0x1001, 40), (0x1002, 40),
            (0xffc0, 40), (0x2f0, 40), (0x2fa, 9), (0x1fe, 9), (0x200, 9), (0x40, 9), (0x104, 9)] {
            let addr = BASE + off;
            for (unwatched, readonly) in [(true, false), (false, false), (true, true)] {
                for entry in [0, 1] {
                    for budget in [1, 2, 5, 6, 7, 8, 12, 13, 29, 30, 64, 65, 200] {
                        for (lbeg, end) in [(BASE, lend), (BASE, lend + 3), (BASE + 3, lend)] {
                            STORE_RUN_TAKEN.store(0, Relaxed);
                            compare_hinted(block, Case { seed: 23, entry, budget, fast: true, unwatched, readonly, ..Case::default() },
                                &|c: &mut Cpu| { c.set_ar(4, addr); c.lbeg = lbeg; c.lend = end; c.lcount = lcount; }, lend);
                            let hit = STORE_RUN_TAKEN.load(Relaxed) != 0;
                            let own = !unwatched && off + 9 * stride <= 0x200;
                            assert!(!hit || (!readonly && off % width == 0 && lbeg == BASE && end == lend && !own), "store run taken outside its proof");
                            taken += hit as u32;
                            watched += (hit && !unwatched) as u32;
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(taken > 100 && watched > 50, "store runs were taken only {taken} times ({watched} watched)");
    cases
}
