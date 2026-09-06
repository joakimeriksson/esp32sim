//! Differential tests executed by tools/wasm-jit-test.mjs in an actual WASM runtime.
//! Constructed instructions exercise the emitter independently of encoding; the scheduler
//! case below uses real encoded instructions and proves that hot dispatch actually happens.
use super::*;
use crate::bus::{tlb_index, TLB_ENTRIES};
use crate::{Fault, FlatRam, Insn, Op, Trap};
const BASE: u32 = 0x4037_0000;
/// A small window above the fast mapping that only the slow bus path can reach.
const SLOW: u32 = BASE + 0x1_0000;
struct Ram {
    ram: FlatRam,
    versions: Vec<u32>,
    tlb: Vec<TlbEntry>,
    fast: bool,
    readonly: bool,
    noted: u32,
    slow: [u8; 256],
}
impl Ram {
    fn new(fast: bool, readonly: bool) -> Self {
        let mut ram = FlatRam::new(BASE, 65536);
        for (i, b) in ram.mem.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37);
        }
        let mut tlb = vec![TlbEntry::EMPTY; TLB_ENTRIES];
        tlb[tlb_index(BASE)] = TlbEntry {
            lo: BASE,
            hi: BASE + 65536,
            base: ram.mem.as_mut_ptr(),
            vbase: 0,
            writable: (!readonly) as u32,
            off: 0,
            src: 0,
        };
        Self {
            ram,
            versions: vec![0; 256],
            tlb,
            fast,
            readonly,
            noted: 0,
            slow: [0x5a; 256],
        }
    }
    fn wrote(&mut self, a: u32, n: u32) {
        for p in (a - BASE) / 256..=(a - BASE + n - 1) / 256 {
            self.versions[p as usize] += 1;
        }
    }
}
impl Bus for Ram {
    fn read8(&mut self, a: u32) -> Result<u8, Fault> {
        if (SLOW..SLOW + 256).contains(&a) { return Ok(self.slow[(a - SLOW) as usize]); }
        self.ram.read8(a)
    }
    fn read16(&mut self, a: u32) -> Result<u16, Fault> {
        self.ram.read16(a)
    }
    fn read32(&mut self, a: u32) -> Result<u32, Fault> {
        self.ram.read32(a)
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), Fault> {
        if self.readonly {
            return Err(Fault::Prohibited);
        }
        if (SLOW..SLOW + 256).contains(&a) { self.slow[(a - SLOW) as usize] = v; return Ok(()); }
        self.ram.write8(a, v)?;
        self.wrote(a, 1);
        Ok(())
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), Fault> {
        if self.readonly {
            return Err(Fault::Prohibited);
        }
        self.ram.write16(a, v)?;
        self.wrote(a, 2);
        Ok(())
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), Fault> {
        if self.readonly {
            return Err(Fault::Prohibited);
        }
        self.ram.write32(a, v)?;
        self.wrote(a, 4);
        Ok(())
    }
    fn fetch(&mut self, a: u32) -> Result<[u8; 4], Fault> {
        self.ram.fetch(a)
    }
    fn page_versions(&self) -> &[u32] {
        &self.versions
    }
    fn code_page(&mut self, a: u32) -> u32 {
        a.wrapping_sub(BASE) / 256
    }
    fn fast_mem(&mut self) -> Option<FastMem> {
        self.fast.then_some(FastMem {
            tlb: self.tlb.as_ptr(),
            page_ver: self.versions.as_mut_ptr(),
        })
    }
    fn note_pc(&mut self, pc: u32) {
        self.noted = pc;
    }
}
fn cpu(seed: u32) -> Cpu {
    let mut c = Cpu::new(0);
    c.pc = BASE;
    c.ps = 0;
    c.vecbase = BASE + 0x8000;
    c.windowbase = seed % 16;
    c.sar = seed % 64;
    let mut x = seed;
    for r in &mut c.ar {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        *r = x;
    }
    c
}
thread_local! { static CONTEXT: std::cell::RefCell<String> = std::cell::RefCell::new(String::new()); }
fn same(a: &Cpu, b: &Cpu) {
    let cx = CONTEXT.with(|c| c.borrow().clone());
    assert_eq!(a.ar, b.ar, "registers at {:x} [{cx}]", a.pc);
    assert_eq!(a.fr, b.fr, "float register bits");
    assert_eq!(a.br, b.br, "boolean registers");
    assert_eq!(a.cpenable, b.cpenable);
    assert_eq!(a.fcr, b.fcr);
    assert_eq!(a.fsr, b.fsr);
    assert_eq!(a.ar, b.ar, "registers at {:x}", a.pc);
    assert_eq!(a.pc, b.pc, "PC");
    assert_eq!(a.ps, b.ps);
    assert_eq!(a.sar, b.sar);
    assert_eq!(a.windowbase, b.windowbase);
    assert_eq!(a.windowstart, b.windowstart);
    assert_eq!(a.lcount, b.lcount);
    assert_eq!(a.epc, b.epc);
    assert_eq!(a.exccause, b.exccause);
    assert_eq!(a.insn_count, b.insn_count);
    assert_eq!(a.ccount, b.ccount);
}
fn insn(op: Op) -> BlockInsn {
    let i = Insn {
        op,
        r: 3,
        s: 4,
        t: 5,
        imm: 3,
        imm2: 7,
        len: 3,
        raw: 0,
    };
    BlockInsn {
        insn: i,
        max_ar: crate::exec::max_ar(&i),
        off: 0,
    }
}
fn compare(
    block: &mut [BlockInsn],
    seed: u32,
    entry: u32,
    budget: u32,
    addr: Option<u32>,
    fast: bool,
    readonly: bool,
    loop_end: bool,
    overflow: bool,
) {
    compare_configured(block, seed, entry, budget, addr, fast, readonly, loop_end, overflow, |_| {});
}
fn compare_configured(
    block: &mut [BlockInsn], seed: u32, entry: u32, budget: u32, addr: Option<u32>,
    fast: bool, readonly: bool, loop_end: bool, overflow: bool, configure: impl Fn(&mut Cpu),
) {
    CONTEXT.with(|c| *c.borrow_mut() = format!("{:?} seed={seed} entry={entry} budget={budget} fast={fast} loop_end={loop_end} overflow={overflow}",
        block.iter().map(|b| b.insn.op).collect::<Vec<_>>()));
    let mut cc = CodeCache::new(0).unwrap();
    let code = queue(&mut cc, block, BASE, fast);
    for _ in 0..HOT {
        ready(&cc, code);
    }
    assert!(ready(&cc, code), "compiled module must execute");
    let (mut a, mut b) = (cpu(seed), cpu(seed));
    let (mut ra, mut rb) = (Ram::new(fast, readonly), Ram::new(fast, readonly));
    for c in [&mut a, &mut b] {
        c.pc = BASE + entry * 3;
        if let Some(addr) = addr {
            c.set_ar(4, addr.wrapping_sub(3));
        }
        if loop_end {
            c.lend = BASE + 6;
            c.lbeg = BASE;
            c.lcount = 2;
        }
        if overflow {
            c.ps = ps::WOE;
            c.windowstart = 1 << ((c.windowbase + 1) % 16);
        }
        configure(c);
    }
    let fm = rb.fast_mem();
    let result = unsafe {
        run(
            &cc,
            code,
            &mut b,
            &mut rb,
            &Helpers::new::<Ram>(),
            budget,
            entry,
            fm,
        )
    };
    let done = result & 0xffff;
    let exit = result >> 16;
    let mut count = 0;
    let mut trap = None;
    let mut pre = false;
    let repeat = loop_len(&cc, code, &a).is_some();
    for _ in 0..budget {
        let index = a.pc.wrapping_sub(BASE) / 3;
        let Some(instruction) = block.get(index as usize) else { break; };
        if let Some(t) = a.check_overflow(instruction.max_ar) {
            trap = Some(t);
            pre = true;
            break;
        }
        let pc = a.pc;
        ra.note_pc(pc);
        let r = exec_insn(&mut a, &mut ra, &instruction.insn);
        count += 1;
        if let Err(t) = r {
            trap = Some(t);
            break;
        }
        // A pre-instruction trap after a completed prefix must still be checked
        // on the next iteration; it retires no additional instruction.
        if (a.pc != pc + 3 && !(repeat && a.pc == a.lbeg)) || (count == done && exit != CODE_TRAP_PRE) {
            break;
        }
    }
    assert_eq!(count, done);
    assert_eq!(pre, exit == CODE_TRAP_PRE);
    assert_eq!(trap, b.jit_trap.take());
    same(&a, &b);
    assert_eq!(ra.ram.mem, rb.ram.mem);
    assert_eq!(ra.versions, rb.versions);
    if done > 0 {
        assert_eq!(ra.noted, rb.noted);
    }
}

fn terminal_helpers() -> u32 {
    use Op::*;
    let mut tests = 0;
    for op in [Call0, Call4, Call8, Call12, Callx0, Callx4, Callx8, Callx12, Ret, RetN, Retw, RetwN] {
        let mut block = [insn(Add), insn(MovN), insn(op)];
        if matches!(op, Callx0 | Callx4 | Callx8 | Callx12) {
            block[2].insn.s = match op { Callx0 => 0, Callx4 => 4, Callx8 => 8, _ => 12 };
            block[2].max_ar = crate::exec::max_ar(&block[2].insn);
        }
        // Dirty the implicit return register: the helper must see its spilled value,
        // and helper writes/window rotations must not be overwritten after it returns.
        block[1].insn.t = 0;
        block[1].max_ar = crate::exec::max_ar(&block[1].insn);
        let mut cc = CodeCache::new(0).unwrap();
        assert!(compile(&mut cc, &mut block, BASE, false).is_some());
        assert!(compile(&mut cc, &mut [insn(op), insn(Add)], BASE, false).is_none());
        assert!(compile(&mut cc, &mut [insn(op)], BASE, false).is_none());
        for wb in [0, 7, 15] {
            for flags in [0, ps::WOE, ps::WOE | ps::EXCM] {
                for windows in [0, 1 << 2, 0xffff] {
                    for inc in 0..4 {
                        for entry in 0..3 {
                            for budget in 1..=3 {
                                compare_configured(&mut block, wb, entry, budget, None, false, false,
                                    false, false, |c| {
                                        c.ps = flags;
                                        c.windowbase = wb;
                                        c.windowstart = windows;
                                        let ret = (inc << 30) | ((BASE + 0x400) & 0x3fff_ffff);
                                        c.set_ar(0, ret);
                                        c.set_ar(4, ret);
                                    });
                                tests += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    tests
}

fn whole_block_guards() -> u32 {
    let mut tests = 0;
    for offset in [-3i32, 0, 1, 3, 4, 6, 9, 10, 12] {
        for count in [0, 1, 0xffff_ffff] {
            for flags in [0, ps::WOE, ps::WOE | ps::EXCM] {
                for windows in [0, 0xffff] {
                    for entry in 0..3 {
                        for budget in 1..=3 {
                            compare_configured(&mut [insn(Op::Add), insn(Op::MovN), insn(Op::Xor)],
                                15, entry, budget, None, false, false, false, false, |c| {
                                    c.lend = BASE.wrapping_add(offset as u32);
                                    c.lbeg = BASE + 0x100;
                                    c.lcount = count;
                                    c.ps = flags;
                                    c.windowstart = windows;
                                });
                            tests += 1;
                        }
                    }
                }
            }
        }
    }
    tests
}
fn scheduler() {
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
fn hardware_loops() -> u32 {
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
                compare_configured(&mut block, 15, entry, budget, None, true, false,
                    false, false, |c| {
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
            compare_configured(&mut crossing, 15, entry, budget, None, true, false,
                false, false, |c| {
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
            compare_configured(&mut suffix, 0, 0, budget, None, false, false,
                false, false, |c| { c.lbeg = BASE; c.lend = BASE + 6; c.lcount = 2; });
            cases += 1;
        }
    }
    // LCOUNT-writing suffixes cannot enter production compilation, so accounting
    // may use LCOUNT deltas as backedge counts even after the suffix executes.
    for op in [Op::Wsr, Op::Xsr] {
        let mut rejected = [insn(Op::Add), insn(Op::MovN), insn(op)];
        rejected[2].insn.imm = crate::state::sr::LCOUNT as i32;
        assert!(compile(&mut CodeCache::new(0).unwrap(), &mut rejected, BASE, true).is_none());
        cases += 1;
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
                        compare_configured(&mut block, 9, entry, budget, None, true, false, loop_end,
                            false, |c| { c.set_ar(6, count); });
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
    for _ in 0..HOT { ready(&cc, code); }
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
    for _ in 0..HOT { ready(&cc, code); }
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
    for _ in 0..HOT { ready(&cc, code); }
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

fn hardware_loop_scheduler() {
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

fn retention() {
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

fn window_masks() -> u32 {
    let mut cc = CodeCache::new(0).unwrap();
    let mut cases = 0;
    for high in [3, 7, 11, 15] {
        let mut low = insn(Op::Movi);
        low.insn.t = 1;
        low.max_ar = 1;
        let mut upper = insn(Op::Add);
        upper.insn.r = high;
        upper.insn.s = 2;
        upper.insn.t = 3;
        upper.max_ar = crate::exec::max_ar(&upper.insn);
        let mut block = [low, upper];
        let id = queue(&mut cc, &mut block, BASE, false);
        for _ in 0..HOT {
            ready(&cc, id);
        }
        for wb in 0..16 {
            for frame in 1..=3 {
                for status in [0, ps::WOE, ps::WOE | ps::EXCM] {
                    for entry in 0..2 {
                        for budget in 1..=2 {
                            let (mut a, mut b) = (cpu(wb), cpu(wb));
                            for c in [&mut a, &mut b] {
                                c.pc = BASE + entry * 3;
                                c.ps = status;
                                c.windowstart = 1 << ((wb + frame) & 15);
                            }
                            let (mut ra, mut rb) = (Ram::new(false, false), Ram::new(false, false));
                            let actual = unsafe {
                                run(
                                    &cc,
                                    id,
                                    &mut b,
                                    &mut rb,
                                    &Helpers::new::<Ram>(),
                                    budget,
                                    entry,
                                    None,
                                )
                            };
                            let (mut done, mut trap) = (0, None);
                            for bi in block.iter().skip(entry as usize).take(budget as usize) {
                                if let Some(t) = a.check_overflow(bi.max_ar) {
                                    trap = Some(t);
                                    break;
                                }
                                exec_insn(&mut a, &mut ra, &bi.insn).unwrap();
                                done += 1;
                            }
                            assert_eq!(actual & 0xffff, done);
                            assert_eq!(trap, b.jit_trap.take());
                            same(&a, &b);
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    cases
}

fn entry_and_shifts() -> u32 {
    use Op::*;
    let mut tests = 0;
    for op in [Sll, Srl] {
        for sar in (0..=64).chain([127, u32::MAX]) {
            for value in [0, 1, 0x8000_0000, 0xffff_ffff, 0xa5a5_5a5a] {
                for alias in [false, true] {
                    let mut shift = insn(op);
                    if alias { shift.insn.r = if op == Sll { 4 } else { 5 }; }
                    shift.max_ar = crate::exec::max_ar(&shift.insn);
                    let mut block = [insn(Nop), shift, insn(Xor)];
                    let mut cc = CodeCache::new(0).unwrap();
                    assert!(compile(&mut cc, &mut block, BASE, false).is_some());
                    compare_configured(&mut block, 15, 0, 3, None, false, false,
                        false, false, |c| {
                            c.sar = sar;
                            c.set_ar(4, value);
                            c.set_ar(5, value);
                        });
                    tests += 1;
                }
            }
        }
    }
    for wb in [0, 14, 15] {
        for flags in [0, ps::WOE, ps::WOE | ps::EXCM] {
            // The +4 frame is outside the initial a15 guard, but can collide
            // after ENTRY rotates. This catches reuse of the whole-block proof.
            for windows in [0, 1 << ((wb + 4) & 15), 0xffff] {
                for inc in 0..4 {
                    for s in [0, 1, 3, 4] {
                        let mut prefix = insn(Movi);
                        prefix.insn.t = 1;
                        prefix.insn.imm = -1;
                        let mut enter = insn(Entry);
                        enter.insn.s = s;
                        enter.insn.imm = 32;
                        let mut upper = insn(Add);
                        upper.insn.r = 15;
                        let mut block = [prefix, enter, upper, enter, insn(Xor)];
                        for bi in &mut block { bi.max_ar = crate::exec::max_ar(&bi.insn); }
                        let mut cc = CodeCache::new(0).unwrap();
                        assert!(compile(&mut cc, &mut block, BASE, false).is_some());
                        for entry in 0..5 {
                            for budget in 0..=5 {
                                compare_configured(&mut block, wb, entry, budget, None, false,
                                    false, false, false, |c| {
                                        c.ps = flags | (inc << ps::CALLINC_SHIFT);
                                        c.windowstart = windows;
                                        // Alternate active loop ends directly after ENTRY.
                                        c.lcount = inc & 1;
                                        c.lend = BASE + 6;
                                        c.lbeg = BASE;
                                    });
                                tests += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    tests
}

fn floating_point_guard_proof() -> u32 {
    use Op::*;
    let mut tests = 0;
    // Ineligible helper-containing blocks still test the emitter's continuation
    // machinery. A helper may disable or enable CP before the scalar instruction.
    for op in [Wsr, Xsr] {
        for enabled in [0, 1] {
            for entry in 0..4 {
                for budget in [1, 2, 4] {
                    let mut block = [insn(Add), insn(op), insn(AddS), insn(Rfr)];
                    block[1].insn.imm = crate::state::sr::CPENABLE as i32;
                    compare_configured(&mut block, 0, entry, budget, None, true, false, false, false, |c| {
                        c.cpenable = enabled; c.set_ar(5, enabled ^ 1);
                        c.fr[3] = 2f32.to_bits(); c.fr[4] = 3f32.to_bits(); c.fr[5] = 4f32.to_bits();
                    });
                    tests += 1;
                }
            }
        }
    }
    // Disabled CP must not prevent a prefix store or cause a trap when a budget
    // cut or taken branch exits before the FP instruction.
    for prefix in [S32i, Bt] {
        for enabled in [0, 1] {
            for entry in 0..3 {
                for budget in [1, 3] {
                    let mut block = [insn(prefix), insn(AddS), insn(Rfr)];
                    if prefix == Bt { block[0].insn.imm = (BASE + 48) as i32; }
                    compare_configured(&mut block, 0, entry, budget, Some(BASE + 512), true, false, false, false, |c| {
                        c.cpenable = enabled; c.br = 1 << 4;
                    });
                    tests += 1;
                }
            }
        }
    }
    tests
}

fn floating_point() -> u32 {
    use Op::*;
    let ops = [AddS, SubS, MulS, MaddS, MsubS, MovS, AbsS, NegS, Rfr, Wfr, ConstS,
        FloatS, UfloatS, RoundS, TruncS, FloorS, CeilS, UtruncS, UnS, OeqS, UeqS, OltS,
        UltS, OleS, UleS, MoveqzS, MovnezS, MovltzS, MovgezS, MovfS, MovtS,
        MaddnS, DivnS, Div0S, Nexp01S, Recip0S, Rsqrt0S, Sqrt0S, AddexpS,
        MkdadjS, MksadjS, AddexpmS, Movf, Movt, Bf, Bt];
    let values = [0, 0x8000_0000, 1, 0x007f_ffff, 0x0080_0000, 0x3f80_0001,
        0xbf80_0000, 0x3fc0_0000, 0xc020_0000, 0x4eff_ffff, 0x4f00_0000,
        0x4f7f_ffff, 0x4f80_0000, 0xcf00_0000, 0x7f7f_ffff, 0x7f80_0000,
        0xff80_0000, 0x7fc1_2345, 0xffc5_4321, 0x7f81_2345];
    let mut tests = 0;
    for op in ops {
        assert!(super::emitter::supported(op, true));
        for (n, &bits) in values.iter().enumerate() {
            for entry in 0..3 {
                for budget in [1, 3] {
                    let mut block = [insn(Add), insn(op), insn(Xor)];
                    block[1].insn.imm = if matches!(op, Bf | Bt) { (BASE + 48) as i32 } else { (n % 16) as i32 };
                    compare_configured(&mut block, n as u32, entry, budget, None, true, false, n % 3 == 0, n % 7 == 0, |c| {
                        c.cpenable = if n % 9 == 0 { 0 } else { 1 };
                        c.br = if n % 2 == 0 { 0xaaaa } else { 0x5555 };
                        c.fr[3] = 0xbf80_0000;
                        c.fr[4] = bits;
                        c.fr[5] = values[(n + 5) % values.len()];
                    });
                    tests += 1;
                }
            }
        }
    }
    // A drawing-like bundle connects float, boolean and integer state through a
    // conversion/coverage decision, including partial execution and aliased operands.
    let raster_ops = [Wfr, FloatS, SubS, MulS, MaddS, OltS, MovtS, TruncS, Movf, Bt];
    for entry in 0..raster_ops.len() as u32 {
        for budget in [1, 4, 12] {
            let mut block: Vec<_> = raster_ops.into_iter().map(insn).collect();
            for bi in &mut block {
                if matches!(bi.insn.op, MovtS | Movf) { bi.insn.t = 3; }
                if bi.insn.op == Bt { bi.insn.s = 3; bi.insn.imm = (BASE + 48) as i32; }
                bi.max_ar = crate::exec::max_ar(&bi.insn);
            }
            compare_configured(&mut block, 15, entry, budget, None, true, false, false, false, |c| {
                c.cpenable = 1; c.fr[3] = 0.5f32.to_bits();
                c.fr[4] = 2.25f32.to_bits(); c.fr[5] = 3.5f32.to_bits();
            });
            tests += 1;
        }
    }
    // Cancellation distinguishes one fused rounding from multiply followed by add.
    for op in [MaddS, MsubS] {
        compare_configured(&mut [insn(op), insn(Rfr)], 0, 0, 2, None, true, false, false, false, |c| {
            c.cpenable = 1; c.fr[3] = (-1f32).to_bits();
            c.fr[4] = if op == MaddS { 0x3f80_0001 } else { 0xbf80_0001 };
            c.fr[5] = 0x3f7f_fffe;
        });
        tests += 1;
    }
    // Both native TLB accesses and helper paths must preserve raw FP bits and code versions.
    for op in [Lsi, Ssi] {
        for addr in [BASE + 512, BASE + 513, BASE + 65536] {
            for fast in [false, true] {
                for readonly in [false, true] {
                    for enabled in [0, 1] {
                        compare_configured(&mut [insn(Add), insn(op), insn(Xor)], 1, 0, 3,
                            Some(addr), fast, readonly, false, false, |c| {
                                c.cpenable = enabled; c.fr[5] = 0x7f81_2345;
                            });
                        tests += 1;
                    }
                }
            }
        }
    }
    tests
}

fn integer_ops() -> u32 {
    use Op::*;
    let mut tests = 0;
    let values = [0, 1, 0xffff_ffff, 0x8000_0000, 0x7fff_ffff, 0xa5a5_5a5a];
    for op in [Abs, Sra, Src, Muluh, Mulsh] {
        // Production admission is separate from queue(), used by the differential harness.
        let mut admitted = [insn(Nop), insn(op), insn(Xor)];
        let mut cc = CodeCache::new(0).unwrap();
        assert!(compile(&mut cc, &mut admitted, BASE, false).is_some());
        for dest in [3, 4, 5] {
            let mut arithmetic = insn(op);
            arithmetic.insn.r = dest;
            arithmetic.max_ar = crate::exec::max_ar(&arithmetic.insn);
            let pairs: Vec<(u32, u32)> = if matches!(op, Muluh | Mulsh) {
                values.iter().flat_map(|&left| values.iter().map(move |&right| (left, right))).collect()
            } else {
                // Distinct halves expose reversed SRC concatenation and extension errors.
                values.iter().map(|&right| (!right, right)).collect()
            };
            for (left, right) in pairs {
                let counts: Vec<u32> = if matches!(op, Sra | Src) {
                    (0..=64).chain([127, u32::MAX]).collect()
                } else { vec![0] };
                for sar in counts {
                    compare_configured(&mut [insn(Nop), arithmetic, insn(Nop)],
                        15, 0, 3, None, false, false, false, false, |c| {
                            c.sar = sar;
                            c.set_ar(4, left);
                            c.set_ar(5, right);
                        });
                    tests += 1;
                }
            }
        }
        // A low-register prefix completes before an overflow at the new instruction.
        let mut prefix = insn(Movi);
        prefix.insn.t = 1;
        prefix.max_ar = crate::exec::max_ar(&prefix.insn);
        // Cover both whole-block and checked execution, including pre-instruction traps.
        for entry in 0..3 {
            for budget in 0..=3 {
                for overflow in [false, true] {
                    for loop_end in [false, true] {
                        compare_configured(&mut [prefix, insn(op), insn(Nop)],
                            15, entry, budget, None, false, false, loop_end, overflow, |c| {
                                c.sar = 32;
                                c.set_ar(4, 0x8000_0000);
                                c.set_ar(5, 0xffff_ffff);
                            });
                        tests += 1;
                    }
                }
            }
        }
    }
    // This opcode remains deliberately outside production admission.
    let mut cc = CodeCache::new(0).unwrap();
    assert!(compile(&mut cc, &mut [insn(Add), insn(Nsa), insn(Xor)], BASE, false).is_none());
    tests
}

/// Tiny assembler for the region programs: each encoder is checked against the decoder
/// so a wrong encoding fails the suite instead of silently testing something else.
mod asm {
    pub fn j(pc: u32, target: u32) -> Vec<u8> { w24(0x6 | ((target.wrapping_sub(pc + 4) & 0x3ffff) << 6)) }
    pub fn rri8(op0: u32, r: u32, s: u32, t: u32, imm8: u32) -> Vec<u8> { w24(op0 | (t << 4) | (s << 8) | (r << 12) | ((imm8 & 0xff) << 16)) }
    /// beq=1 bne=9 blt=2 bge=0xa bltu=3 bgeu=0xb
    pub fn bcc(r: u32, pc: u32, s: u32, t: u32, target: u32) -> Vec<u8> { rri8(7, r, s, t, target.wrapping_sub(pc + 4)) }
    /// beqz=0 bnez=1 bltz=2 bgez=3
    pub fn bz(m: u32, pc: u32, s: u32, target: u32) -> Vec<u8> { w24(0x6 | 0x10 | (m << 6) | (s << 8) | ((target.wrapping_sub(pc + 4) & 0xfff) << 12)) }
    pub fn l8ui(t: u32, s: u32, imm: u32) -> Vec<u8> { rri8(2, 0, s, t, imm) }
    pub fn l16ui(t: u32, s: u32, imm: u32) -> Vec<u8> { rri8(2, 1, s, t, imm / 2) }
    pub fn s8i(t: u32, s: u32, imm: u32) -> Vec<u8> { rri8(2, 4, s, t, imm) }
    pub fn movi(t: u32, imm: u32) -> Vec<u8> { w24(0x2 | (t << 4) | (((imm >> 8) & 0xf) << 8) | (0xa << 12) | ((imm & 0xff) << 16)) }
    pub fn xor(r: u32, s: u32, t: u32) -> Vec<u8> { w24((t << 4) | (s << 8) | (r << 12) | (3 << 20)) }
    pub fn rsr(t: u32, sr: u32) -> Vec<u8> { w24((3 << 16) | (sr << 8) | (t << 4)) }
    /// loop=8 loopnez=9 loopgtz=10; the end is pc + 4 + imm8
    pub fn lp(r: u32, pc: u32, s: u32, end: u32) -> Vec<u8> { w24(0x76 | (s << 8) | (r << 12) | ((end - pc - 4) << 16)) }
    pub fn entry(s: u32, frame: u32) -> Vec<u8> { w24(0x36 | (s << 8) | ((frame >> 3) << 12)) }
    pub fn call8(pc: u32, target: u32) -> Vec<u8> { w24(0x25 | (((target.wrapping_sub((pc & !3) + 4) >> 2) & 0x3ffff) << 6)) }
    pub fn retw_n() -> Vec<u8> { w16(0xf01d) }
    pub fn nop_n() -> Vec<u8> { w16(0xf03d) }
    pub fn addi_n(r: u32, s: u32, imm: i32) -> Vec<u8> { w16(0xb | (((if imm == -1 { 0 } else { imm as u32 }) & 0xf) << 4) | (s << 8) | (r << 12)) }
    pub fn add_n(r: u32, s: u32, t: u32) -> Vec<u8> { w16(0xa | (t << 4) | (s << 8) | (r << 12)) }
    pub fn mov_n(t: u32, s: u32) -> Vec<u8> { w16(0xd | (t << 4) | (s << 8)) }
    pub fn movi_n(s: u32, imm: u32) -> Vec<u8> { w16(0xc | (((imm >> 4) & 7) << 4) | (s << 8) | ((imm & 0xf) << 12)) }
    fn w24(w: u32) -> Vec<u8> { vec![w as u8, (w >> 8) as u8, (w >> 16) as u8] }
    fn w16(w: u32) -> Vec<u8> { vec![w as u8, (w >> 8) as u8] }
}

/// Run `program` through the block scheduler with regions and compare against
/// instruction-by-instruction execution under varying credit, probes, code changes
/// and timer deadlines. Returns the largest single-call retirement, which proves that
/// a region ran past its head block.
fn region_program(name: &str, program: &[u8], expected: &[(u32, Op, u32)], data: &[u8], head_len: u32, interior: u32, setup: impl Fn(&mut Cpu), turns: usize) -> u32 {
    let (mut a, mut b) = (cpu(3), cpu(3));
    let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
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

fn regions() -> u32 {
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
        let head: Vec<BlockInsn> = (0..6).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, off: 0 }) }).collect();
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
        let head: Vec<BlockInsn> = (0..3).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, off: 0 }) }).collect();
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
        let head: Vec<BlockInsn> = (0..4).scan(BASE + 12, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, off: 0 }) }).collect();
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
    // Region formation itself for the tile scan: every static successor, nothing past RSR.
    let mut ram = Ram::new(true, false);
    ram.ram.mem[..tp.len()].copy_from_slice(&tp);
    let c = cpu(0);
    let head: Vec<BlockInsn> = (0..2).scan(BASE, |pc, _| { let i = crate::decode::decode(*pc, ram.fetch(*pc).unwrap()); *pc += i.len as u32; Some(BlockInsn { insn: i, max_ar: 0, off: 0 }) }).collect();
    let formed = emitter::region::form(&c, &mut ram, BASE, &head, true).expect("tile region");
    assert_eq!(formed.chunks.iter().map(|c| (c.pc - BASE, c.instructions.len())).collect::<Vec<_>>(),
        vec![(0, 2), (35, 1), (6, 3), (41, 2), (13, 6), (43, 1)]);
    assert_eq!(formed.pages, vec![(0, 0)]);
    assert!(emitter::region::form(&c, &mut ram, BASE + 38, &head, true).is_none(), "RSR head");
    cases + 2
}

pub fn run_tests() -> u32 {
    use Op::*;
    let ops = [
        Nop, NopN, Memw, Extw, Movi, MoviN, Mov, MovN, Add, AddN, Sub, And, Or, Xor, Mull, Salt,
        Saltu, Addi, AddiN, Addmi, Addx2, Addx4, Addx8, Subx2, Subx4, Subx8, Neg, Slli, Srli, Srai,
        Sll, Srl, Extui, Sext, Ssr, Ssl, Ssa8l, Ssa8b, Ssai, Abs, Sra, Src, Muluh, Mulsh, Nsa, Min, Max, Minu, Maxu, Moveqz, Movnez,
        Movltz, Movgez, Nsau, J, Jx, Beqz, BeqzN, Bnez, BnezN, Bltz, Bgez, Beqi, Bnei, Blti, Bgei,
        Bltui, Bgeui, Beq, Bne, Blt, Bge, Bltu, Bgeu, Bbci, Bbsi, Bbc, Bbs,
    ];
    let mut tests = 0;
    for op in ops {
        for seed in [0, 1, 15, 0xffff_ffff] {
            for entry in 0..3 {
                for budget in 1..=3 {
                    let mut block = [insn(Add), insn(op), insn(Xor)];
                    compare(
                        &mut block, seed, entry, budget, None, false, false, false, false,
                    );
                    tests += 1;
                }
            }
        }
    }
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
                    compare(
                        &mut block,
                        15,
                        0,
                        3,
                        Some(addr),
                        fast,
                        readonly,
                        false,
                        false,
                    );
                    tests += 1;
                }
            }
        }
    }
    for overflow in [false, true] {
        for entry in 0..3 {
            compare(
                // Keep an unsupported opcode here to exercise helper continuation.
                &mut [insn(Add), insn(Nsa), insn(Xor)],
                15,
                entry,
                3,
                None,
                false,
                false,
                true,
                overflow,
            );
            tests += 1;
        }
    }
    scheduler();
    tests += regions();
    retention();
    hardware_loop_scheduler();
    crate::block::ownership_tests::compiled_helpers_follow_the_current_bus_type();
    tests + integer_ops() + floating_point() + floating_point_guard_proof() + 4 + hardware_loops() + window_masks() + terminal_helpers() + whole_block_guards() + entry_and_shifts()
}
