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

pub(super) fn retention() {
    use Op::*;
    let mut cc = CodeCache::new(0).unwrap();
    let mut code = [insn(Add), insn(S32i), insn(Xor)];
    let first = queue(&mut cc, &mut code, BASE, true);
    for _ in 0..HOT {
        ready(&cc, first);
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
