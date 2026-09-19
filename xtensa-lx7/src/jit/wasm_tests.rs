//! Differential tests executed by tools/wasm-jit-test.mjs in an actual WASM runtime.
//! Constructed instructions exercise the emitter independently of encoding; the scheduler
//! suites use real encoded instructions and prove that hot dispatch actually happens.
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
    defer_armed: bool,
    deferred: bool,
    #[cfg(feature = "wasm-cache-inline")]
    inline_cache: Option<(Vec<emu_core::bus::FastCacheLine>, u64)>,
    #[cfg(feature = "wasm-cache-inline")]
    helper_accesses: u32,
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
            defer_armed: false,
            deferred: false,
            #[cfg(feature = "wasm-cache-inline")]
            inline_cache: None,
            #[cfg(feature = "wasm-cache-inline")]
            helper_accesses: 0,
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
        #[cfg(feature = "wasm-cache-inline")]
        if self.inline_cache.is_some() { self.helper_accesses += 1; }
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
        #[cfg(feature = "wasm-cache-inline")]
        if self.inline_cache.is_some() { self.helper_accesses += 1; }
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
    fn defer_armed(&self) -> bool { self.defer_armed }
    fn defer_access(&mut self, addr: u32) -> bool {
        if self.defer_armed && (SLOW..SLOW + 256).contains(&addr) {
            self.deferred = true; true
        } else { false }
    }
    fn deferred(&self) -> bool { self.deferred }
    #[cfg(feature = "wasm-cache-inline")]
    fn fast_cache(&mut self) -> Option<emu_core::bus::FastCache> {
        self.inline_cache.as_mut().map(|(lines, hits)| emu_core::bus::FastCache { lines: lines.as_mut_ptr(), hits })
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
    assert_eq!(a.pc, b.pc, "PC");
    assert_eq!(a.ps, b.ps);
    assert_eq!(a.sar, b.sar);
    assert_eq!(a.windowbase, b.windowbase);
    assert_eq!(a.windowstart, b.windowstart);
    assert_eq!((a.lbeg, a.lend, a.lcount), (b.lbeg, b.lend, b.lcount));
    assert_eq!((a.interrupt, a.intenable), (b.interrupt, b.intenable));
    assert_eq!((a.scompare1, a.vecbase, a.prid, a.depc), (b.scompare1, b.vecbase, b.prid, b.depc));
    assert_eq!((a.eps, a.excsave, a.misc), (b.eps, b.excsave, b.misc));
    assert_eq!(a.epc, b.epc);
    assert_eq!(a.exccause, b.exccause);
    assert_eq!(a.insn_count, b.insn_count);
    assert_eq!(a.ccount, b.ccount);
    assert_eq!(a.timing_extra, b.timing_extra, "timing extras [{cx}]");
    assert_eq!(a.excvaddr, b.excvaddr, "EXCVADDR [{cx}]");
    assert_eq!(a.qr, b.qr, "PIE Q registers [{cx}]");
    assert_eq!(a.accx, b.accx, "PIE ACCX [{cx}]");
    assert_eq!((a.qacc_h, a.qacc_l, a.sar_byte), (b.qacc_h, b.qacc_l, b.sar_byte), "PIE QACC / SAR_BYTE [{cx}]");
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
        straddle: false,
        off: 0,
    }
}
/// Inputs shared by the constructed-instruction differential suites. Omitted flags
/// select ordinary RAM, no loop end and no forced window overflow.
#[derive(Clone, Copy, Default)]
struct Case {
    seed: u32,
    entry: u32,
    budget: u32,
    addr: Option<u32>,
    fast: bool,
    readonly: bool,
    loop_end: bool,
    overflow: bool,
}

fn compare(block: &mut [BlockInsn], case: Case, configure: impl Fn(&mut Cpu)) {
    let Case { seed, entry, budget, addr, fast, readonly, loop_end, overflow } = case;
    let priced = PRICED.load(std::sync::atomic::Ordering::Relaxed);
    CONTEXT.with(|c| *c.borrow_mut() = format!("{:?} seed={seed} entry={entry} budget={budget} fast={fast} loop_end={loop_end} overflow={overflow} priced={priced}",
        block.iter().map(|b| b.insn.op).collect::<Vec<_>>()));
    let (mut ra, mut rb) = (Ram::new(fast, readonly), Ram::new(fast, readonly));
    for bi in block.iter_mut() {
        bi.straddle = priced && crate::exec::static_target(&bi.insn).is_some_and(|pc| crate::exec::straddles(&mut ra, pc));
    }
    let extras = crate::exec::static_extras(block.iter().map(|bi| &bi.insn));
    let mut cc = CodeCache::new(0).unwrap();
    let code = queue(&mut cc, block, BASE, fast);
    for _ in 0..HOT {
        ready(&cc, code);
    }
    assert!(ready(&cc, code), "compiled module must execute");
    let (mut a, mut b) = (cpu(seed), cpu(seed));
    for c in [&mut a, &mut b] {
        c.price_control = priced;
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
    let exit = (result >> 16) & 7;
    if exit == CODE_CUT {
        let next = (result >> 19) as usize;
        assert!(next < block.len(), "cut continuation must be inside the block");
        let pc = block[..next].iter().fold(BASE, |pc, bi| pc.wrapping_add(bi.insn.len as u32));
        assert_eq!(b.pc, pc, "cut continuation index must match the architectural PC");
    }
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
        if priced && r.is_ok() {
            // The same accounting boundary as the ordinary block interpreter.
            // h_exec prices helper control flow; the emitter must not charge it twice.
            let taken = crate::exec::control_taken(&a, &instruction.insn);
            a.timing_extra += crate::exec::control_price(instruction.insn.op, taken)
                + u32::from(extras[index as usize])
                + u32::from(taken && crate::exec::transfers(instruction.insn.op) && crate::exec::straddles(&mut ra, a.pc));
        }
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

#[path = "wasm_tests/arithmetic.rs"]
mod arithmetic;
#[path = "wasm_tests/control.rs"]
mod control;
#[path = "wasm_tests/float.rs"]
mod float;
#[path = "wasm_tests/loops.rs"]
mod loops;
#[path = "wasm_tests/memory.rs"]
mod memory;
#[path = "wasm_tests/scheduler.rs"]
mod scheduler;
#[path = "wasm_tests/timing.rs"]
mod timing;
#[path = "wasm_tests/regions.rs"]
mod regions;
#[path = "wasm_tests/asm.rs"]
mod asm;

pub fn run_tests() -> u32 {
    let mut tests = 0;
    #[cfg(feature = "wasm-cache-inline")]
    { tests += memory::inline_cache_hits(); }
    tests += arithmetic::basic_ops() + arithmetic::division()
        + memory::loads_and_stores() + control::helper_continuation();
    scheduler::scheduler();
    tests += 1;
    tests += memory::extension_deferral() + memory::flat_ram_bounds() + regions::regions();
    scheduler::retention();
    tests += 1;
    loops::hardware_loop_scheduler();
    tests += 1;
    crate::block::ownership_tests::compiled_helpers_follow_the_current_bus_type();
    tests += 1;
    tests + arithmetic::integer_ops() + float::floating_point() + float::floating_point_guard_proof()
        + loops::hardware_loops() + control::window_masks() + control::terminal_helpers()
        + control::special_register_blocks() + control::whole_block_guards()
        + control::entry_and_shifts() + timing::priced_cases()
}
