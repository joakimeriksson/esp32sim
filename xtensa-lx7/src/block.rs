//! Basic-block interpreter.
//!
//! `step()` pays the full per-instruction price — interrupt check, decode-cache probe and
//! validation, cycle/instruction accounting — on every instruction. Measured on real firmware,
//! that scaffolding is a third of run time and no single piece of it is removable, so this
//! module amortises it: a *block* is a straight-line run of pre-decoded instructions ending
//! at a control transfer or at anything that changes interrupt/timer state, and the checks
//! run once per block.
//!
//! What stays exact:
//! - **Timer interrupts**: a block never runs past the point where a `CCOMPARE` would match
//!   (the distance bounds the block), so the interrupt is flagged at the same instruction
//!   boundary as before. Reads and writes of `CCOUNT`/`CCOMPARE*` are forced to start a
//!   block, so they always see exact time.
//! - **Peripheral interrupts**: the bus reports when a register write may have changed an
//!   interrupt line (`Bus::block_break`) and the block ends there, so delivery latency is
//!   unchanged. Instructions that alter `PS`, `INTENABLE` or interrupt state end blocks.
//! - **Control flow**: after each instruction the actual `pc` is compared with the fall-through
//!   address, so taken branches and exceptions leave the block immediately. The WASM JIT
//!   may retain a safe hardware-loop prefix across backedges within the same budget; it
//!   validates code versions and observer boundaries before each repeat.
//! - **Self-modifying code**: a block remembers the write-version of the (at most two) pages
//!   it was decoded from and is rebuilt when either changes. Stores from within a block
//!   into its own remaining instructions take effect at the next block entry — on silicon
//!   that case needs `isync` anyway, and `isync` ends a block.
//! - **Window overflow** checks stay per instruction; they are the one check that matters.
use crate::bus::Bus;
use crate::decode::{decode, Insn, Op};
use crate::exec::{exec_insn, max_ar, Trap};
use crate::state::{sr, Cpu};

pub use emu_core::core::pc_bit;

#[derive(Clone, Copy)]
pub struct BlockInsn { pub insn: Insn, pub max_ar: u8, /// Backend entry: native byte offset or WASM instruction index
 pub off: u32 }

#[derive(Clone, Copy)]
struct Entry { pc: u32, start: u32, n: u16, vidx: [u32; 2], ver: [u32; 2], code: u32 }
impl Entry { const EMPTY: Entry = Entry { pc: 1, start: 0, n: 0, vidx: [0; 2], ver: [0; 2], code: crate::jit::NONE }; }

#[cfg(not(target_arch = "wasm32"))] const ENTRIES: usize = 1 << 17;
#[cfg(target_arch = "wasm32")] const ENTRIES: usize = 1 << 15;
/// Instructions per block. At most 3 bytes each, so a block spans at most two version pages.
pub const MAX_LEN: usize = 32;
/// Arena size at which decoded entries are rebuilt. The arena never reallocates:
/// native code holds pointers into it; WASM code owns separate retained instruction storage.
#[cfg(not(target_arch = "wasm32"))] const ARENA_MAX: usize = 1 << 20;
#[cfg(target_arch = "wasm32")] const ARENA_MAX: usize = 1 << 17;   // 4 MB per core in the browser
/// Native code cache size; flushed together with the blocks when full.
const CODE_SIZE: usize = 256 << 20;   // address space; pages are only committed as code is written

pub struct BlockCache {
    #[cfg(all(target_arch = "wasm32", feature = "wasm-jit-profile"))]
    pub profile: crate::jit::profile::Profile,
    entries: Vec<Entry>,
    arena: Vec<BlockInsn>,
    /// A block cut short by the caller's budget or a timer deadline resumes here rather than
    /// spawning a new block at the cut point: (entry index, arena index, pc at that index).
    resume: (u32, u32, u32),
    pub builds: u64,
    pub flushes: u64,
    /// native code for blocks, when the host supports it and `jit_enabled`
    code: Option<crate::jit::CodeCache>,
    pub jit_enabled: bool,
    /// A machine observer requires one callback for each individual block execution.
    pub observed: bool,
    pub compiled: u64,
    /// Instructions retired through compiled blocks (including their interpreter helpers).
    pub jit_instructions: u64,
}

impl BlockCache {
    pub fn new() -> Self {
        BlockCache {
                     #[cfg(all(target_arch = "wasm32", feature = "wasm-jit-profile"))]
                     profile: crate::jit::profile::Profile::default(),
                     entries: vec![Entry::EMPTY; ENTRIES], arena: Vec::with_capacity(ARENA_MAX + MAX_LEN), resume: (0, 0, 1), builds: 0, flushes: 0,
                     code: crate::jit::CodeCache::new(CODE_SIZE), jit_enabled: crate::jit::AVAILABLE, observed: false, compiled: 0, jit_instructions: 0 }
    }
    pub fn flush(&mut self) {
        for e in self.entries.iter_mut() { *e = Entry::EMPTY; }
        self.arena.clear(); self.resume = (0, 0, 1); self.flushes += 1;
        if let Some(c) = &mut self.code { c.reset(); }
    }
    /// Bytes of native code currently in use.
    pub fn code_bytes(&self) -> usize { self.code.as_ref().map(|c| c.used()).unwrap_or(0) }
    pub fn jit_active(&self) -> bool { self.jit_enabled && self.code.is_some() }
    #[inline(always)]
    fn index(pc: u32) -> usize { ((pc >> 1) ^ (pc >> 16)) as usize & (ENTRIES - 1) }
    #[inline(always)]
    fn valid(e: &Entry, pv: &[u32]) -> bool {
        pv.get(e.vidx[0] as usize).copied().unwrap_or(0) == e.ver[0] && pv.get(e.vidx[1] as usize).copied().unwrap_or(0) == e.ver[1]
    }
}

impl Default for BlockCache { fn default() -> Self { Self::new() } }
/// A clone of a CPU starts with an empty cache; compiled code is tied to the original's arena.
impl Clone for BlockCache { fn clone(&self) -> Self { let mut b = Self::new(); b.jit_enabled = self.jit_enabled; b.observed = self.observed; b } }

/// The instruction ends a block: control transfer, or a change to interrupt/timer/window state
/// that the per-block checks depend on.
fn ends_block(i: &Insn) -> bool {
    use Op::*;
    match i.op {
        Ill | IllN | Break | BreakN | Syscall | Simcall | Waiti | Rsil | Isync | Rsync | Esync | Dsync | Excw
        | J | Jx | Call0 | Call4 | Call8 | Call12 | Callx0 | Callx4 | Callx8 | Callx12
        | Ret | RetN | Retw | RetwN | Rotw | Rfe | Rfue | Rfde | Rfwo | Rfwu | Rfi | Rfme
        | Beqz | Bnez | Bltz | Bgez | BeqzN | BnezN | Beqi | Bnei | Blti | Bgei | Bltui | Bgeui
        | Bnone | Beq | Blt | Bltu | Ball | Bbc | Bbci | Bany | Bne | Bge | Bgeu | Bnall | Bbs | Bbsi | Bf | Bt
        | Loop | Loopnez | Loopgtz | Wsr | Xsr => true,
        _ => i.len == 0,
    }
}

/// The instruction must be the first of its block: it reads or writes state that is only exact
/// at a block boundary (`CCOUNT`, `CCOMPARE*`, `INTERRUPT`, `INTENABLE`, `PS`).
fn must_start_block(i: &Insn) -> bool {
    matches!(i.op, Op::Rsr | Op::Wsr | Op::Xsr)
        && matches!(i.imm as u32, sr::CCOUNT | sr::INTERRUPT | sr::INTCLEAR | sr::INTENABLE | sr::PS | sr::ICOUNT | 240..=242)
}

/// Decode a block starting at `pc0` and register it. Only the first fetch can fault: a later
/// unmapped instruction simply ends the block and faults when it is reached as a block start.
fn build<B: Bus>(cpu: &mut Cpu, bus: &mut B, pc0: u32) -> Result<(u32, u32, u16), Trap> {
    #[cfg(all(
        target_arch = "aarch64",
        any(target_os = "macos", target_os = "linux")
    ))]
    let code_short = cpu.blocks.jit_active() && cpu.blocks.code.as_ref().unwrap().remaining() < crate::jit::MAX_BLOCK_CODE;
    #[cfg(not(all(
        target_arch = "aarch64",
        any(target_os = "macos", target_os = "linux")
    )))]
    let code_short = false;
    if cpu.blocks.arena.len() + MAX_LEN > ARENA_MAX || code_short { cpu.blocks.flush(); }
    let start = cpu.blocks.arena.len() as u32;
    let (mut pc, mut n, mut last) = (pc0, 0u16, pc0);
    loop {
        let bytes = match bus.fetch(pc) {
            Ok(b) => b,
            Err(_) => { if n == 0 { return Err(cpu.raise_mem(crate::state::exc::IFETCH_ERROR, pc)); } break; }
        };
        let i = decode(pc, bytes);
        if n > 0 && (must_start_block(&i) || cpu.boundary_bloom & pc_bit(pc) != 0) { break; }
        cpu.blocks.arena.push(BlockInsn { insn: i, max_ar: max_ar(&i), off: 0 });
        n += 1; last = pc;
        pc = pc.wrapping_add(i.len as u32);
        if ends_block(&i) || n as usize == MAX_LEN { break; }
    }
    let last_byte = last.wrapping_add(cpu.blocks.arena[(start + n as u32 - 1) as usize].insn.len.max(1) as u32 - 1);
    let vidx0 = bus.code_page(pc0);
    let vidx1 = if last_byte >> 7 != pc0 >> 7 { bus.code_page(last_byte) } else { vidx0 };   // pages are >= 128 B
    let pv = bus.page_versions();
    let ver = [pv.get(vidx0 as usize).copied().unwrap_or(0), pv.get(vidx1 as usize).copied().unwrap_or(0)];
    let ei = BlockCache::index(pc0);
    let mut code = crate::jit::NONE;
    let fast = bus.fast_mem().is_some();
    if cpu.blocks.jit_active() {
        let b = &mut cpu.blocks;
        let (s, e) = (start as usize, start as usize + n as usize);
        if let Some(c) = crate::jit::compile(b.code.as_mut().unwrap(), &mut b.arena[s..e], pc0, fast) { code = c; b.compiled += 1; }
    }
    cpu.blocks.entries[ei] = Entry { pc: pc0, start, n, vidx: [vidx0, vidx1], ver, code };
    cpu.blocks.builds += 1;
    Ok((ei as u32, start, n))
}

/// Run a block (or a cut continuation) at `cpu.pc`, at most `budget` instructions.
/// WASM may repeat an admitted hardware-loop prefix within that same budget.
/// Returns `(iterations, trap)` where iterations is what a loop over `step()` would have
/// consumed: executed instructions, plus one for a trap taken before an instruction ran.
pub fn run_block<B: Bus>(cpu: &mut Cpu, bus: &mut B, budget: u32) -> (u32, Option<Trap>) {
    #[cfg(all(target_arch = "wasm32", feature = "wasm-jit-profile"))]
    {
        if cpu.blocks.profile.sample() {
            let pc = cpu.pc;
            let ei = if cpu.blocks.resume.2 == pc { cpu.blocks.resume.0 as usize } else { BlockCache::index(pc) };
            let before = cpu.blocks.jit_instructions;
            let start = crate::jit::profile::now();
            let result = run_block_inner(cpu, bus, budget);
            let elapsed = crate::jit::profile::now() - start;
            let e = cpu.blocks.entries[ei];
            let ops = &cpu.blocks.arena[e.start as usize..(e.start + e.n as u32) as usize];
            let fast = bus.fast_mem().is_some();
            // Attribute resumed execution to its decoder block head, matching JIT names.
            cpu.blocks.profile.record(e.pc, before != cpu.blocks.jit_instructions, result.0, elapsed, ops, fast);
            return result;
        }
    }
    run_block_inner(cpu, bus, budget)
}

// Keep this boundary visible to a sampling profiler without adding per-block clocks.
#[cfg_attr(all(target_arch = "wasm32", feature = "wasm-cpu-profile"), inline(never))]
fn run_block_inner<B: Bus>(cpu: &mut Cpu, bus: &mut B, budget: u32) -> (u32, Option<Trap>) {
    if let Some(t) = cpu.check_interrupts() { return (1, Some(t)); }
    if cpu.waiting { cpu.advance_ccount(1); return (1, None); }
    let pc = cpu.pc;

    // find the block: a pending continuation, a cached block, or a fresh decode
    let (ei, mut k, end) = {
        let (rei, rk, rpc) = cpu.blocks.resume;
        let e = cpu.blocks.entries[rei as usize];
        if rpc == pc && e.pc != 1 && BlockCache::valid(&e, bus.page_versions()) && rk >= e.start && rk < e.start + e.n as u32 {
            (rei, rk, e.start + e.n as u32)
        } else {
            let ei = BlockCache::index(pc);
            let e = cpu.blocks.entries[ei];
            if e.pc == pc && BlockCache::valid(&e, bus.page_versions()) { (ei as u32, e.start, e.start + e.n as u32) }
            else { match build(cpu, bus, pc) { Ok((ei, s, n)) => (ei, s, s + n as u32), Err(t) => return (1, Some(t)) } }
        }
    };
    cpu.blocks.resume.2 = 1;

    // never run past a CCOMPARE match: the timer interrupt must land on the same instruction
    let mut limit = (end - k).min(budget);
    for i in 0..3 { let d = cpu.ccompare[i].wrapping_sub(cpu.ccount); if d != 0 && d < limit { limit = d; } }

    let code = cpu.blocks.entries[ei as usize].code;
    if code != crate::jit::NONE && cpu.blocks.jit_enabled && crate::jit::ready(cpu.blocks.code.as_ref().unwrap(), code) {
        #[cfg(target_arch = "wasm32")]
        if crate::jit::loop_len(cpu.blocks.code.as_ref().unwrap(), code, cpu).is_some() {
            limit = budget.min(0xffff);
            for i in 0..3 { let d = cpu.ccompare[i].wrapping_sub(cpu.ccount); if d != 0 && d < limit { limit = d; } }
        }
        let entry = cpu.blocks.arena[k as usize].off;
        let fm = bus.fast_mem();
        #[cfg(not(target_arch = "wasm32"))]
        let r = {
            // Copy the executable pointer, ending the cache borrow before borrowing Cpu.
            // Native execution needs no cache metadata while generated code is running.
            let target = cpu.blocks.code.as_ref().unwrap().entry_point(code);
            let helpers = crate::jit::Helpers::shared::<B>();
            // SAFETY: `code` and `entry` identify published code in this CPU's live cache.
            // Neither generated code nor its interpreter helpers reset or compile the cache.
            // The arena stays unmoved, and fallback helpers copy instructions before borrowing
            // Cpu. Helpers are static for B; `fm` describes this exclusive bus borrow.
            unsafe { crate::jit::run(target, cpu, bus, helpers, limit, entry, fm) }
        };
        #[cfg(target_arch = "wasm32")]
        let r = {
            // WASM needs retained block metadata during execution, so move its owning cache
            // outside Cpu before holding that shared reference alongside the exclusive CPU.
            let cache = cpu.blocks.code.take().unwrap();
            let helpers = crate::jit::Helpers::new::<B>();
            // SAFETY: `code` and `entry` identify live code in this locally owned cache;
            // helpers match B and `fm` describes this exclusive bus borrow.
            let r = unsafe { crate::jit::run(&cache, code, cpu, bus, &helpers, limit, entry, fm) };
            cpu.blocks.code = Some(cache);
            r
        };
        let (done, exit) = (r & 0xffff, r >> 16);
        cpu.blocks.jit_instructions += done as u64;
        cpu.insn_count += done as u64;
        cpu.advance_ccount(done);
        return match exit {
            crate::jit::CODE_TRAP => (done, cpu.jit_trap.take()),
            crate::jit::CODE_TRAP_PRE => (done + 1, cpu.jit_trap.take()),
            crate::jit::CODE_CUT => {
                #[cfg(target_arch = "wasm32")]
                {
                    // A repeated hardware prefix makes retired count differ from arena offset.
                    let e = cpu.blocks.entries[ei as usize];
                    let mut at = e.pc;
                    for index in e.start..end {
                        if at == cpu.pc { cpu.blocks.resume = (ei, index, cpu.pc); break; }
                        at = at.wrapping_add(cpu.blocks.arena[index as usize].insn.len as u32);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                if k + done < end { cpu.blocks.resume = (ei, k + done, cpu.pc); }
                (done, None)
            }
            _ => (done, None),
        };
    }

    let (mut done, mut trap, mut pre, mut broke) = (0u32, None, false, false);
    while done < limit {
        let e = cpu.blocks.arena[k as usize];
        if let Some(t) = cpu.check_overflow(e.max_ar) { trap = Some(t); pre = true; break; }
        let at = cpu.pc;
        bus.note_pc(at);
        let expected = at.wrapping_add(e.insn.len as u32);
        let r = exec_insn(cpu, bus, &e.insn);
        done += 1; k += 1;
        if let Err(t) = r { trap = Some(t); break; }
        if cpu.pc != expected || bus.block_break() { broke = true; break; }
    }
    cpu.insn_count += done as u64;
    cpu.advance_ccount(done);
    // cut short by the budget or a timer deadline while still inside the block: resume there
    if trap.is_none() && !broke && k < end { cpu.blocks.resume = (ei, k, cpu.pc); }
    (done + pre as u32, trap)
}

#[cfg(any(test, feature = "wasm-jit-tests"))]
pub(crate) mod ownership_tests {
    use super::*;
    use crate::bus::{FastMem, TlbEntry, TLB_ENTRIES};
    use crate::{Fault, FlatRam};

    // Distinct Bus types with deliberately identical layout: the regression observes wrong
    // helper selection without relying on a layout mismatch or invalid memory access.
    #[repr(transparent)]
    struct TaggedBus<const VALUE: u32>(FlatRam);
    impl<const VALUE: u32> Bus for TaggedBus<VALUE> {
        fn read8(&mut self, a: u32) -> Result<u8, Fault> { self.0.read8(a) }
        fn read16(&mut self, a: u32) -> Result<u16, Fault> { self.0.read16(a) }
        fn read32(&mut self, _: u32) -> Result<u32, Fault> { Ok(VALUE) }
        fn write8(&mut self, a: u32, v: u8) -> Result<(), Fault> { self.0.write8(a, v) }
        fn write16(&mut self, a: u32, v: u16) -> Result<(), Fault> { self.0.write16(a, v) }
        fn write32(&mut self, a: u32, v: u32) -> Result<(), Fault> { self.0.write32(a, v) }
        fn fetch(&mut self, pc: u32) -> Result<[u8; 4], Fault> { self.0.fetch(pc) }
        fn page_versions(&self) -> &[u32] { self.0.page_versions() }
        fn fast_mem(&mut self) -> Option<FastMem> {
            // Admit compiled loads but force their slow helper path.
            static TLB: [TlbEntry; TLB_ENTRIES] = [TlbEntry::EMPTY; TLB_ENTRIES];
            Some(FastMem { tlb: TLB.as_ptr(), page_ver: &mut self.0.ver })
        }
    }
    fn bus<const VALUE: u32>() -> TaggedBus<VALUE> {
        let mut ram = FlatRam::new(0x4037_0000, 64);
        // l32i.n a3,a4,0; j self
        ram.mem[..5].copy_from_slice(&[0x38, 0x04, 0x06, 0xff, 0xff]);
        TaggedBus(ram)
    }

    #[cfg_attr(test, test)]
    pub(crate) fn compiled_helpers_follow_the_current_bus_type() {
        let mut cpu = Cpu::new(0);
        cpu.ps = 0;
        let mut first = bus::<11>();
        let mut second = bus::<22>();
        // A WASM slow load exits immediately; limit both backends to that instruction.
        for _ in 0..40 {
            cpu.pc = first.0.base;
            assert_eq!(run_block(&mut cpu, &mut first, 1), (1, None));
            assert_eq!(cpu.get_ar(3), 11);
        }
        let compiled = cpu.blocks.jit_instructions;
        cpu.pc = second.0.base;
        assert_eq!(run_block(&mut cpu, &mut second, 1), (1, None));
        assert_eq!(cpu.get_ar(3), 22);
        if crate::jit::AVAILABLE {
            assert!(compiled > 0, "must exercise compiled helpers");
            assert_eq!(cpu.blocks.jit_instructions, compiled + 1);
        }
        // Repeated switches reuse the compiled block but must select each bus's helpers.
        for _ in 0..4 {
            cpu.pc = first.0.base;
            assert_eq!(run_block(&mut cpu, &mut first, 1), (1, None));
            assert_eq!(cpu.get_ar(3), 11);
            cpu.pc = second.0.base;
            assert_eq!(run_block(&mut cpu, &mut second, 1), (1, None));
            assert_eq!(cpu.get_ar(3), 22);
        }
        if crate::jit::AVAILABLE {
            assert_eq!(cpu.blocks.jit_instructions, compiled + 9);
            assert!(cpu.blocks.code.is_some(), "execution must restore cache ownership");
        }
        // Execution must restore cache ownership for subsequent invalidation and reuse.
        cpu.blocks.flush();
        cpu.pc = first.0.base;
        assert_eq!(run_block(&mut cpu, &mut first, 1), (1, None));
        assert_eq!(cpu.get_ar(3), 11);
    }
}
