use super::*;

pub(super) fn scheduler() {
    // addi.n a3,a3,1; addi.n a4,a4,1; j back to the first instruction.
    let program = [0x1b, 0x33, 0x1b, 0x44, 0x06, 0xfe, 0xff];
    let (mut a, mut b) = (cpu(7), cpu(7));
    let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
    ra.ram.mem[..7].copy_from_slice(&program);
    rb.ram.mem[..7].copy_from_slice(&program);
    let mut total = 0;
    for turn in 0..400 {
        // Cut/resume at every possible position; change a hot instruction after compilation.
        if turn == 250 {
            ra.write8(BASE + 1, 0x55).unwrap();
            rb.write8(BASE + 1, 0x55).unwrap();
        }
        let budget = 1 + turn % 7;
        let (done, trap) = crate::block::run_block(&mut b, &mut rb, budget);
        assert!(trap.is_none());
        for _ in 0..done {
            crate::step(&mut a, &mut ra).unwrap();
        }
        total += done;
        same(&a, &b);
    }
    assert!(
        b.blocks.jit_instructions > 100,
        "scheduler did not use compiled blocks ({total})"
    );
    // A timer deadline inside an already hot block must land at the same instruction.
    for c in [&mut a, &mut b] {
        c.ccompare[0] = c.ccount + 2;
        c.intenable = 1 << 6;
    }
    for _ in 0..10 {
        let (done, trap) = crate::block::run_block(&mut b, &mut rb, 32);
        let mut oracle = None;
        for _ in 0..done {
            if let Err(t) = crate::step(&mut a, &mut ra) {
                oracle = Some(t);
                break;
            }
        }
        assert_eq!(trap, oracle);
        same(&a, &b);
        if matches!(trap, Some(Trap::Interrupt(_))) {
            return;
        }
    }
    panic!("timer interrupt not delivered");
}

/// EX172: an exception return into the middle of a decoded block runs that block from the
/// index instead of decoding a new head; an unmarked arrival gets its own head; changed code
/// is never entered through a stale alias.
pub(super) fn interior_alias() {
    // 4 x (addi.n a3,a3,1; addi.n a4,a4,1); j back to the first instruction.
    let mut program = [0x1b, 0x33, 0x1b, 0x44].repeat(4);
    program.extend([0x06, 0xfb, 0xff]);
    let (mut a, mut b) = (cpu(7), cpu(7));
    let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
    ra.ram.mem[..program.len()].copy_from_slice(&program);
    rb.ram.mem[..program.len()].copy_from_slice(&program);
    let check = |a: &mut Cpu, b: &mut Cpu, ra: &mut Ram, rb: &mut Ram, budget: u32| {
        let (done, trap) = crate::block::run_block(b, rb, budget);
        assert!(trap.is_none());
        for _ in 0..done { crate::step(a, ra).unwrap(); }
        same(a, b);
    };
    for _ in 0..40 { check(&mut a, &mut b, &mut ra, &mut rb, 64); }
    assert!(b.blocks.jit_instructions > 0);
    // Execute RFE itself, not just its arrival marker. Place it on another code page.
    for ram in [&mut ra, &mut rb] { ram.ram.mem[256..259].copy_from_slice(&[0x00, 0x30, 0x00]); }
    for c in [&mut a, &mut b] {
        c.pc = BASE + 256;
        c.epc[1] = BASE + 4;
        c.ps |= crate::state::ps::EXCM;
    }
    check(&mut a, &mut b, &mut ra, &mut rb, 1);
    assert_eq!(b.pc, BASE + 4);
    assert_eq!(b.blocks.alias_pc, b.pc);
    let (builds, hits) = (b.blocks.builds, b.blocks.alias_hits);
    check(&mut a, &mut b, &mut ra, &mut rb, 64);
    assert_eq!(b.blocks.builds, builds);
    assert_eq!(b.blocks.alias_hits, hits + 1);
    for round in 0..3 {
        for i in 1..8u32 {
            for budget in [1, 2, 3, 5, 64] {
                a.pc = BASE + 2 * i;
                b.pc = a.pc;
                b.blocks.alias_pc = b.pc;
                let (builds, hits) = (b.blocks.builds, b.blocks.alias_hits);
                check(&mut a, &mut b, &mut ra, &mut rb, budget);
                assert_eq!(b.blocks.builds, builds, "interior arrival decoded a new head");
                // PCs given their own head below hit the entry table instead, until the code changes.
                let own = (round == 1 && i == 3) || (round == 2 && i == 2);
                assert_eq!(b.blocks.alias_hits, hits + u64::from(!own), "round {round} i {i} budget {budget}");
                // finish the block (resumes), then a whole pass from the head
                check(&mut a, &mut b, &mut ra, &mut rb, 64);
                check(&mut a, &mut b, &mut ra, &mut rb, 64);
            }
        }
        if round == 0 {
            // A statically reached interior PC (no exception return) keeps its own head.
            a.pc = BASE + 6;
            b.pc = a.pc;
            let builds = b.blocks.builds;
            check(&mut a, &mut b, &mut ra, &mut rb, 64);
            assert_eq!(b.blocks.builds, builds + 1);
        }
        if round == 1 {
            // Changed code: the aliased block is stale and must not be entered.
            ra.write8(BASE + 1, 0x55).unwrap();
            rb.write8(BASE + 1, 0x55).unwrap();
            a.pc = BASE + 4;
            b.pc = a.pc;
            b.blocks.alias_pc = b.pc;
            let builds = b.blocks.builds;
            check(&mut a, &mut b, &mut ra, &mut rb, 64);
            assert_eq!(b.blocks.builds, builds + 1, "stale alias used");
            a.pc = BASE;
            b.pc = BASE;
            for _ in 0..40 { check(&mut a, &mut b, &mut ra, &mut rb, 64); }
        }
    }
    // Each bypass starts with a fresh hot owner and no budget continuation.
    for bypass in 0..4 {
        b.blocks.flush();
        for _ in 0..40 {
            a.pc = BASE; b.pc = BASE;
            check(&mut a, &mut b, &mut ra, &mut rb, 64);
        }
        a.pc = BASE + 4; b.pc = a.pc;
        b.blocks.alias_pc = b.pc;
        match bypass {
            0 => b.blocks.flush(),
            1 => b.blocks.observed = true,
            2 => { a.price_control = true; b.price_control = true; }
            _ => b.boundary_bloom = crate::block::pc_bit(b.pc),
        }
        let (builds, hits) = (b.blocks.builds, b.blocks.alias_hits);
        check(&mut a, &mut b, &mut ra, &mut rb, 1);
        assert_eq!(b.blocks.builds, builds + 1);
        assert_eq!(b.blocks.alias_hits, hits);
        b.blocks.observed = false;
        a.price_control = false; b.price_control = false;
        b.boundary_bloom = 0;
    }
}

pub(super) fn interior_alias_deferred() {
    let mut c = cpu(7);
    let mut ram = Ram::new(true, false);
    // addi.n a3,a3,1; s32i a3,a4,0; addi.n a5,a5,1; j self
    ram.ram.mem[..10].copy_from_slice(&[0x1b, 0x33, 0x32, 0x64, 0x00, 0x1b, 0x55, 0x06, 0xff, 0xff]);
    c.set_ar(4, SLOW);
    for _ in 0..40 { c.pc = BASE; crate::block::run_block(&mut c, &mut ram, 64); }
    assert!(c.blocks.jit_instructions > 0);
    c.pc = BASE;
    ram.defer_armed = true;
    let (retired, writes, value) = (c.insn_count, ram.slow_writes, c.get_ar(3));
    assert_eq!(crate::block::run_block(&mut c, &mut ram, 64), (1, None));
    assert!(ram.deferred);
    assert_eq!(c.pc, BASE + 2);
    assert_eq!(c.insn_count, retired + 1);
    assert_eq!(ram.slow_writes, writes);
    assert_eq!(c.get_ar(3), value.wrapping_add(1));
    assert_eq!(c.blocks.alias_pc, c.pc);
    ram.defer_armed = false; ram.deferred = false;
    let (builds, hits) = (c.blocks.builds, c.blocks.alias_hits);
    assert_eq!(crate::block::run_block(&mut c, &mut ram, 1), (1, None));
    assert_eq!(c.pc, BASE + 5);
    assert_eq!(c.insn_count, retired + 2);
    assert_eq!(ram.slow_writes, writes + 1);
    assert_eq!(&ram.slow[..4], &value.wrapping_add(1).to_le_bytes());
    assert_eq!(c.blocks.builds, builds);
    assert_eq!(c.blocks.alias_hits, hits + 1);
}

pub(super) fn retention() {
    use Op::*;
    let mut cc = CodeCache::new(0).unwrap();
    let mut code = [insn(Add), insn(S32i), insn(Xor)];
    let first = queue(&mut cc, &mut code, BASE, true);
    for _ in 0..HOT {
        ready(&cc, first, 0);
    }
    let slot = cc.blocks[first as usize].slot.get();
    assert!(slot != 0 && slot != NONE);
    cc.reset();
    let reused = queue(&mut cc, &mut code, BASE, true);
    assert_eq!(cc.blocks[reused as usize].slot.get(), slot);
    // Force the retained module through its slow helper: the embedded instruction
    // pointer must still be live after cache compaction, and the store must run once.
    let mut c = cpu(15);
    c.set_ar(4, BASE + 0x200 - 3);
    let mut ram = Ram::new(false, false);
    let result = unsafe {
        run(
            &cc,
            reused,
            &mut c,
            &mut ram,
            &Helpers::new::<Ram>(),
            3,
            0,
            None,
        )
    };
    assert_eq!(result & 0xffff, 2);
    assert_eq!(ram.versions[2], 1);
    assert_eq!(ram.read32(BASE + 0x200).unwrap(), c.get_ar(5));
    code[0].insn.imm = 123;
    let changed = queue(&mut cc, &mut code, BASE, true);
    assert_ne!(
        changed, reused,
        "decoded fields must all participate in identity"
    );
    assert_eq!(cc.blocks[changed as usize].slot.get(), NONE);
    assert_ne!(
        queue(&mut cc, &mut code[..2], BASE, true),
        changed,
        "boundary split"
    );
    assert_ne!(
        queue(&mut cc, &mut code, BASE, false),
        changed,
        "fast-memory contract"
    );
    for _ in 0..3 {
        cc.reset();
    }
    assert!(cc.blocks.is_empty(), "unused generations must expire");
    for pc in 0..RETAIN_BLOCKS + 7 {
        queue(&mut cc, &mut code, pc as u32, false);
    }
    cc.reset();
    assert_eq!(cc.blocks.len(), RETAIN_BLOCKS);
}
