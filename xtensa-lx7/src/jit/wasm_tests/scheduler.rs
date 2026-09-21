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

pub(super) fn interior_alias_instruction_bytes() {
    // A three-byte ADDI between narrow instructions. RFE may target any byte;
    // only its first byte is an instruction boundary in the existing owner.
    let mut program = asm::nop_n();
    program.extend([0x22, 0xc2, 0x01]); // addi a2,a2,1
    program.extend(asm::nop_n());
    program.extend([0x06, 0xff, 0xff]); // j .
    for offset in [2, 3, 4, 5] {
        let (mut a, mut b) = (cpu(7), cpu(7));
        let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
        for ram in [&mut ra, &mut rb] {
            ram.ram.mem[..program.len()].copy_from_slice(&program);
            ram.ram.mem[256..259].copy_from_slice(&[0x00, 0x30, 0x00]); // rfe
        }
        for _ in 0..40 {
            a.pc = BASE; b.pc = BASE;
            let (done, trap) = crate::block::run_block(&mut b, &mut rb, 4);
            assert!(trap.is_none());
            for _ in 0..done { crate::step(&mut a, &mut ra).unwrap(); }
        }
        assert!(b.blocks.jit_instructions > 0);
        for c in [&mut a, &mut b] {
            c.pc = BASE + 256;
            c.epc[1] = BASE + offset;
            c.ps |= crate::state::ps::EXCM;
        }
        crate::step(&mut a, &mut ra).unwrap();
        assert!(crate::block::run_block(&mut b, &mut rb, 1).1.is_none());
        assert_eq!(b.blocks.alias_pc, BASE + offset);
        let (builds, hits) = (b.blocks.builds, b.blocks.alias_hits);
        let oracle = crate::step(&mut a, &mut ra).err();
        let (done, trap) = crate::block::run_block(&mut b, &mut rb, 1);
        assert_eq!(done, 1);
        assert_eq!(trap, oracle);
        same(&a, &b);
        let boundary = offset == 2 || offset == 5;
        assert_eq!(b.blocks.alias_hits, hits + u64::from(boundary));
        assert_eq!(b.blocks.builds, builds + u64::from(!boundary), "interior bytes need their own decode");
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

/// Computed edges cannot form regions, so this proves wrapper chaining itself.
pub(super) fn wrapper_chain() {
    for mode in 0..3 {
        let counter_successor = mode == 1;
        let store_successor = mode == 2;
        let mut program = vec![0; 128];
        let mut first = asm::addi_n(2, 2, 1);
        if store_successor { first.extend(asm::s8i(7, 8, 0)); }
        first.extend([0xa0, 0x04, 0x00]); // jx a4
        program[..first.len()].copy_from_slice(&first);
        let mut second = Vec::new();
        if counter_successor { second.extend(asm::rsr(6, 234)); }
        for _ in 0..10 { second.extend(asm::addi_n(3, 3, 1)); }
        second.extend([0xa0, 0x05, 0x00]); // jx a5
        program[64..64 + second.len()].copy_from_slice(&second);
        let (mut a, mut b) = (cpu(0), cpu(0));
        let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
        for r in [&mut ra, &mut rb] { r.ram.mem[..program.len()].copy_from_slice(&program); }
        for c in [&mut a, &mut b] { c.pc = BASE; c.ps = 0; c.set_ar(4, BASE + 64); c.set_ar(5, BASE); c.set_ar(7, 0x42); c.set_ar(8, BASE + 0x2000); }
        let jump = BASE + if store_successor { 5 } else { 2 };
        assert_eq!(crate::decode::decode(jump, ra.fetch(jump).unwrap()).op, Op::Jx);
        let check = |a: &mut Cpu, b: &mut Cpu, ra: &mut Ram, rb: &mut Ram, budget| {
            let (done, trap) = crate::block::run_block(b, rb, budget);
            let mut oracle = None;
            for _ in 0..done { if let Err(t) = crate::step(a, ra) { oracle = Some(t); break; } }
            assert_eq!(trap, oracle);
            same(a, b);
            (done, trap)
        };
        for _ in 0..1200 { check(&mut a, &mut b, &mut ra, &mut rb, 1); }
        assert!(b.blocks.jit_instructions > 100);
        for c in [&mut a, &mut b] { c.pc = BASE; }
        if store_successor {
            // The inline store invalidates the already-compiled successor before lookup.
            for c in [&mut a, &mut b] { c.set_ar(8, BASE + 127); }
            assert_eq!(check(&mut a, &mut b, &mut ra, &mut rb, 32).0, 3, "modified successor must redispatch");
            assert_eq!(ra.ram.mem, rb.ram.mem);
            assert_eq!(ra.versions, rb.versions);
            continue;
        }
        let (done, _) = check(&mut a, &mut b, &mut ra, &mut rb, 5);

        if counter_successor {
            assert_eq!(done, 2, "RSR CCOUNT successor must redispatch");
            check(&mut a, &mut b, &mut ra, &mut rb, 5);
            continue;
        }
        assert_eq!(done, 5, "must cut inside the chained successor");
        assert_ne!(b.blocks.chain_ei, NONE, "must retain chained CUT entry");
        check(&mut a, &mut b, &mut ra, &mut rb, 4); // resume that CUT
        for c in [&mut a, &mut b] { c.pc = BASE; c.boundary_bloom = crate::block::pc_bit(BASE + 64); }
        assert_eq!(check(&mut a, &mut b, &mut ra, &mut rb, 32).0, 2, "probed successor must redispatch");
        for c in [&mut a, &mut b] {
            c.pc = BASE; c.boundary_bloom = 0;
            c.ccompare[0] = c.ccount.wrapping_add(5); c.intenable = 1 << 6;
        }
        let mut interrupted = false;
        for _ in 0..4 {
            if matches!(check(&mut a, &mut b, &mut ra, &mut rb, 32).1, Some(Trap::Interrupt(_))) { interrupted = true; break; }
        }
        assert!(interrupted, "deadline inside chained successor");
    }    for underflow in [false, true] {
        let (mut a, mut b) = (cpu(0), cpu(0));
        let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
        for r in [&mut ra, &mut rb] {
            r.ram.mem[..5].copy_from_slice(&[0x3d, 0xf0, 0xa0, 0x04, 0x00]); // nop.n; jx a4
            r.ram.mem[64..66].copy_from_slice(&asm::nop_n());
            r.ram.mem[66..68].copy_from_slice(&asm::retw_n());
        }
        let reset = |c: &mut Cpu, pc, missing| {
            c.pc = pc; c.ps = ps::WOE; c.windowbase = 3;
            c.windowstart = (1 << 3) | if missing { 0 } else { 1 << 2 };
            c.set_ar(0, BASE); c.set_ar(4, BASE + 64);
        };
        let check = |a: &mut Cpu, b: &mut Cpu, ra: &mut Ram, rb: &mut Ram, budget| {
            let (done, trap) = crate::block::run_block(b, rb, budget);
            let mut oracle = None;
            for _ in 0..done { if let Err(t) = crate::step(a, ra) { oracle = Some(t); break; } }
            assert_eq!(trap, oracle);
            same(a, b);
            (done, trap)
        };
        for _ in 0..40 {
            for pc in [BASE, BASE + 64] {
                for c in [&mut a, &mut b] { reset(c, pc, false); }
                check(&mut a, &mut b, &mut ra, &mut rb, 2);
            }
        }
        for c in [&mut a, &mut b] { reset(c, BASE, underflow); }
        let (done, trap) = check(&mut a, &mut b, &mut ra, &mut rb, 6);
        assert_eq!(done, if underflow { 4 } else { 6 }, "only a successful RETW may continue the chain");
        assert_ne!(b.blocks.chain_ei, NONE);
        assert_eq!(trap, if underflow { Some(Trap::Exception(0x301)) } else { None });
    }

}
