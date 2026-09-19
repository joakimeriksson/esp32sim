//! WASM block backend for the ordinary scheduler. Hot blocks are installed once in the
//! exported function table; execution then uses WASM call_indirect, with no JS dispatch.
//! This preserves the interpreter's instruction-count timing, not the receipt cost model.
use crate::block::BlockInsn;
use crate::bus::{Bus, FastMem, TlbEntry};
use crate::exec::exec_insn;
use crate::state::{ps, Cpu};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::mem::{offset_of, size_of};

pub const AVAILABLE: bool = true;
pub const NONE: u32 = u32::MAX;
pub const CODE_END: u32 = 0;
pub const CODE_LEFT: u32 = 1;
pub const CODE_TRAP: u32 = 2;
pub const CODE_CUT: u32 = 3;
pub const CODE_TRAP_PRE: u32 = 4;
/// A region declined to run (resume, short credit, window or coprocessor state).
pub const CODE_REJECT: u32 = 5;
/// Formation attempts per block, including re-formation after a code page changed.
const REGION_TRIES: u8 = 8;
#[cfg(feature = "wasm-jit-tests")]
pub(crate) static REGION_STATS: [std::sync::atomic::AtomicU32; 12] = [const { std::sync::atomic::AtomicU32::new(0) }; 12];

#[cfg(not(feature = "wasm-jit-profile"))]
type ExitSite = u32;
#[cfg(feature = "wasm-jit-profile")]
type ExitSite = (u32, ExitKind);

#[cfg(feature = "wasm-jit-profile")]
#[derive(Clone, Copy, Default)]
enum ExitKind {
    Call, Callx, Retw, Ret, Jx, Sr, Memory, Edge, Budget, Dirty,
    #[default]
    Other,
}
#[cfg(feature = "wasm-jit-profile")]
impl ExitKind {
    fn for_op(op: crate::Op) -> Self {
        use crate::Op::*;
        match op {
            Call0 | Call4 | Call8 | Call12 => Self::Call,
            Callx0 | Callx4 | Callx8 | Callx12 => Self::Callx,
            Retw | RetwN => Self::Retw,
            Ret | RetN => Self::Ret,
            Jx => Self::Jx,
            Wsr | Xsr | Rsil => Self::Sr,
            L8ui | L16ui | L16si | L32i | L32iN | L32r | S8i | S16i | S32i | S32iN | Lsi | Ssi | Pie => Self::Memory,
            _ => Self::Other,
        }
    }
}
#[inline(always)]
fn site_pc(site: ExitSite) -> u32 {
    #[cfg(feature = "wasm-jit-profile")]
    { site.0 }
    #[cfg(not(feature = "wasm-jit-profile"))]
    { site }
}

/// Region counters for the opt-in profile build; absent from production.
#[cfg(feature = "wasm-jit-profile")]
#[derive(Default)]
pub struct RegionStats {
    pub formed: Cell<u64>,
    pub failed: Cell<u64>,
    pub covered: Cell<u64>,
    pub dropped: Cell<u64>,
    pub calls: Cell<u64>,
    pub rejected: Cell<u64>,
    pub retired: Cell<u64>,
    pub exits: [Cell<u64>; 8],
    pub left_kinds: [Cell<u64>; 11],
    pub chunks: Cell<u64>,
    pub instructions: Cell<u64>,
    pub bytes: Cell<u64>,
    /// EX153 census: [run calls, calls with budget>=64, whole calls, whole retired, tail-cut calls, tail-cut retired,
    /// resumed calls, resumed retired, resumed-and-cut-again calls, zero-retired calls, sum of budgets]
    pub ex153: [Cell<u64>; 12],
}
#[cfg(feature = "wasm-jit-profile")]
impl RegionStats {
    pub fn report(&self) -> String {
        format!("[ex153] run_calls={} budget64_calls={} whole_calls={} whole_retired={} tailcut_calls={} tailcut_retired={} resumed_calls={} resumed_retired={} resumed_cut_again={} zero_retired_calls={} budget_sum={} chained={}\n[wasm-region] formed={} failed={} covered={} dropped={} chunks={} instructions={} bytes={} calls={} rejected={} retired={} exits[end,left,trap,cut,pre]={:?} left_kinds[call,callx,retw,ret,jx,sr,memory,edge,budget,dirty,other]={:?}",
            self.ex153[0].get(), self.ex153[1].get(), self.ex153[2].get(), self.ex153[3].get(), self.ex153[4].get(), self.ex153[5].get(),
            self.ex153[6].get(), self.ex153[7].get(), self.ex153[8].get(), self.ex153[9].get(), self.ex153[10].get(), self.ex153[11].get(),
            self.formed.get(), self.failed.get(), self.covered.get(), self.dropped.get(), self.chunks.get(),
            self.instructions.get(), self.bytes.get(), self.calls.get(), self.rejected.get(), self.retired.get(),
            self.exits[..5].iter().map(|c| c.get()).collect::<Vec<_>>(),
            self.left_kinds.iter().map(|c| c.get()).collect::<Vec<_>>())
    }
}
pub static CENSUS: [std::sync::atomic::AtomicU64; 8] = [const { std::sync::atomic::AtomicU64::new(0) }; 8];
#[inline(always)]
fn census(i: usize, n: u64) { if cfg!(feature = "wasm-cpu-profile") { CENSUS[i].fetch_add(n, std::sync::atomic::Ordering::Relaxed); } }
const HOT: u32 = 32;
/// EX153: chain compiled calls inside the wrapper.
const CHAIN: bool = true;
/// EX153: an interpreter helper ran during this wrapper call; the dispatcher must look again.
static HELPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// EX138: emit control-flow prices into code generated from now on.
pub static PRICED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Emit the inline data-cache probe into code generated from now on (a `cache-inline` build that
/// runs without the timing model must not pay for it).
/// EX147: generated code records reached instruction byte ranges in `Cpu::fetch_ring`.
pub static FETCH_RING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set index mask of the inline data-cache probe: 63 for the 32 KB cache, 127 for 64 KB (64-byte lines, 8 ways).
pub static CACHE_SET_MASK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(63);
pub static CACHE_PROBES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const RETAIN_BYTES: usize = 64 << 20;
const RETAIN_BLOCKS: usize = 16_384;

#[link(wasm_import_module = "env")]
extern "C" {
    fn host_jit_compile(bytes: *const u8, len: usize) -> u32;
    fn host_jit_release(slot: u32);
}

struct Block {
    instructions: Vec<BlockInsn>,
    pc: u32,
    pcs: Vec<u32>,
    fast: bool,
    loop_prefix: usize,
    /// EX156: the LEND seen inside this block when its module was generated, or zero.
    lend_hint: Cell<u32>,
    generation: u64,
    hits: Cell<u32>,
    slot: Cell<u32>,
    bytes: Cell<usize>,
    /// A region headed by this block, once it is hot and one could be formed.
    region: RefCell<Option<Region>>,
    region_tries: Cell<u8>,
    /// Region (owning block, chunk) this head was last found in; rechecked when stale.
    covered_by: Cell<(u32, u32)>,
    /// Last coverage epoch where this PC was absent from the map.
    uncovered_epoch: Cell<u64>,
    /// EX136: everything a dispatch at this head needs to enter its region, copied out of the
    /// owning block so the common path follows no pointers; valid while `epoch` is current.
    hot: Cell<Hot>,
}
/// Entry facts of one region chunk. `sites` points into the owning region's vector, which
/// lives until that region is dropped, and every drop moves `CodeCache::region_epoch` on.
#[derive(Clone, Copy)]
struct Hot { epoch: u64, bloom: u64, slot: u32, k: u32, len: u32, lo: u32, span: u32, pages: [(u32, u32); 8], npages: u32, nsites: u32, sites: *const ExitSite }
impl Hot { const NONE: Hot = Hot { epoch: 0, bloom: 0, slot: 0, k: 0, len: 0, lo: 0, span: 0, pages: [(0, 0); 8], npages: 0, nsites: 0, sites: std::ptr::null() }; }
/// Several chunks compiled as one function; see wasm_region.rs.
struct Region {
    /// The generated code holds pointers to these instructions for its helper calls,
    /// so they live exactly as long as the module does.
    #[allow(dead_code)]
    chunks: Vec<emitter::region::Chunk>,
    slot: u32,
    bytes: usize,
    bloom: u64,
    lo: u32,
    hi: u32,
    loops: Vec<(u32, u32)>,
    pages: Vec<(u32, u32)>,
    sites: Vec<ExitSite>,
    /// instructions per chunk, for the credit check at an entry
    lens: Vec<u32>,
}
// Compiled instructions own their backing storage, independently of the decoder arena.
// A decoder flush invalidates every handle before reset may compact this cache.
pub struct CodeCache {
    blocks: Vec<Block>,
    by_pc: HashMap<(u32, usize, bool), u32>,
    generation: u64,
    /// Chunk heads of live regions: PC -> (owning block, chunk index). A head inside
    /// some region does not get a region of its own; a dispatch there enters the
    /// covering region at that chunk. Overlapping copies of one loop would only cost
    /// code and compile time.
    covered: RefCell<HashMap<u32, (u32, u32)>>,
    /// Advances whenever a previously absent PC might acquire a region.
    coverage_epoch: Cell<u64>,
    /// EX136: moves on whenever a region is dropped or block indices change; never zero.
    region_epoch: Cell<u64>,
    #[cfg(feature = "wasm-jit-profile")]
    pub region_stats: RegionStats,
}
impl Block {
    fn release(&self, id: u32, covered: &RefCell<HashMap<u32, (u32, u32)>>) {
        if self.slot.get() != NONE && self.slot.get() != 0 {
            // SAFETY: reset/drop happen only when no compiled block is executing.
            unsafe {
                host_jit_release(self.slot.get());
            }
        }
        self.drop_region(id, covered);
    }
    fn drop_region(&self, id: u32, covered: &RefCell<HashMap<u32, (u32, u32)>>) {
        if let Some(r) = self.region.borrow_mut().take() {
            let mut covered = covered.borrow_mut();
            for c in &r.chunks {
                if covered.get(&c.pc).is_some_and(|&(owner, _)| owner == id) { covered.remove(&c.pc); }
            }
            // SAFETY: as above; a region is dropped from Rust between compiled calls.
            unsafe {
                host_jit_release(r.slot);
            }
        }
    }
    fn size(&self) -> usize {
        self.bytes.get() + self.region.borrow().as_ref().map_or(0, |r| r.bytes)
    }
}
impl CodeCache {
    pub fn new(_: usize) -> Option<Self> {
        Some(Self {
            blocks: Vec::new(),
            by_pc: HashMap::new(),
            generation: 0,
            covered: RefCell::new(HashMap::new()),
            coverage_epoch: Cell::new(0),
            region_epoch: Cell::new(1),
            #[cfg(feature = "wasm-jit-profile")]
            region_stats: RegionStats::default(),
        })
    }
    pub fn used(&self) -> usize {
        self.blocks.iter().map(|b| b.size()).sum()
    }
    pub fn reset(&mut self) {
        self.generation += 1;
        self.coverage_epoch.set(self.coverage_epoch.get().wrapping_add(1));
        self.region_epoch.set(self.region_epoch.get() + 1);
        // Keep recently decoded blocks across arena turnover. Prefer recent code under
        // pressure; enforce these retention limits only after all decoder handles die.
        self.blocks.sort_by_key(|b| std::cmp::Reverse(b.generation));
        let (mut bytes, mut count) = (0, 0);
        let (generation, covered) = (self.generation, &self.covered);
        let mut id = 0u32;
        self.blocks.retain(|b| {
            let keep = generation - b.generation <= 2
                && count < RETAIN_BLOCKS
                && bytes + b.size() <= RETAIN_BYTES;
            if keep {
                bytes += b.size();
                count += 1;
            } else {
                b.release(id, covered);
            }
            id += 1;
            keep
        });
        // Retained blocks have new indices: rebuild every map that holds them.
        self.by_pc.clear();
        let mut covered = self.covered.borrow_mut();
        covered.clear();
        for (id, b) in self.blocks.iter().enumerate() {
            self.by_pc
                .insert((b.pc, b.instructions.len(), b.fast), id as u32);
            if let Some(r) = b.region.borrow().as_ref() {
                for (k, c) in r.chunks.iter().enumerate() { covered.entry(c.pc).or_insert((id as u32, k as u32)); }
            }
        }
    }
}
impl Drop for CodeCache {
    fn drop(&mut self) {
        for (id, b) in self.blocks.iter().enumerate() {
            b.release(id as u32, &self.covered);
        }
    }
}

pub fn compile(
    cc: &mut CodeCache,
    instructions: &mut [BlockInsn],
    pc: u32,
    fast: bool,
) -> Option<u32> {
    if instructions.len() < 2 || !emitter::admitted(instructions, fast) {
        return None;
    }
    Some(queue(cc, instructions, pc, fast))
}

fn queue(cc: &mut CodeCache, instructions: &mut [BlockInsn], pc: u32, fast: bool) -> u32 {
    for (i, instruction) in instructions.iter_mut().enumerate() {
        instruction.off = i as u32;
    }
    let key = (pc, instructions.len(), fast);
    if let Some(&id) = cc.by_pc.get(&key) {
        let b = &mut cc.blocks[id as usize];
        // PC alone is not identity: self-modifying code and new observer boundaries
        // must never resurrect stale code. Compare every decoded field, including raw.
        if b.instructions
            .iter()
            .zip(instructions.iter())
            .all(|(a, b)| a.insn == b.insn && a.max_ar == b.max_ar)
        {
            b.generation = cc.generation;
            return id;
        }
    }
    let id = cc.blocks.len() as u32;
    let mut at = pc;
    let pcs = instructions
        .iter()
        .map(|i| {
            let old = at;
            at = at.wrapping_add(i.insn.len as u32);
            old
        })
        .collect();
    cc.blocks.push(Block {
        pcs,
        // Explicit loop-state writes break the LCOUNT-delta accounting used for retained
        // prefixes. A terminal WSR/XSR LEND can also create a new helper-side backedge.
        loop_prefix: if instructions.iter().any(|i| matches!(i.insn.op, crate::Op::Loop | crate::Op::Loopnez | crate::Op::Loopgtz)
            || (matches!(i.insn.op, crate::Op::Wsr | crate::Op::Xsr)
                && matches!(i.insn.imm as u32, crate::state::sr::LBEG | crate::state::sr::LEND | crate::state::sr::LCOUNT))) { 0 }
            else { instructions.iter().take_while(|i| emitter::loop_safe(i.insn.op, fast)).count() },
        instructions: instructions.to_vec(),
        pc,
        fast,
        lend_hint: Cell::new(0),
        generation: cc.generation,
        hits: Cell::new(0),
        slot: Cell::new(NONE),
        bytes: Cell::new(0),
        region: RefCell::new(None),
        region_tries: Cell::new(0),
        covered_by: Cell::new((NONE, 0)),
        uncovered_epoch: Cell::new(u64::MAX),
        hot: Cell::new(Hot::NONE),
    });
    cc.by_pc.insert(key, id);
    id
}
#[inline]
pub fn ready(cc: &CodeCache, code: u32, lend: u32) -> bool {
    let b = &cc.blocks[code as usize];
    let slot = b.slot.get();
    if slot == NONE { prepare(b, lend) } else { slot != 0 }
}

#[cold]
#[inline(never)]
fn prepare(b: &Block, lend: u32) -> bool {
    let hits = b.hits.get() + 1;
    b.hits.set(hits);
    if hits < HOT { return false; }
    // LEND persists after its loop: a loop-body block keeps seeing the same value.
    if b.instructions.iter().zip(&b.pcs).any(|(i, pc)| pc.wrapping_add(i.insn.len as u32) == lend) { b.lend_hint.set(lend); }
    let bytes = generate(b);
    // SAFETY: The host synchronously copies these bytes, installs a module using the
    // shared memory/table, and returns a correctly typed function slot or zero.
    let slot = unsafe { host_jit_compile(bytes.as_ptr(), bytes.len()) };
    b.slot.set(slot);
    b.bytes.set(if slot == 0 { 0 } else { bytes.len() });
    slot != 0
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Helpers {
    exec: *const (),
    overflow: *const (),
    fused: *const (),
    loop_end: u32,
    version_ptrs: [*const u32; 2],
    versions: [u32; 2],
    #[cfg(feature = "wasm-cache-inline")]
    cache: *const emu_core::bus::FastCache,
}
impl Helpers {
    pub const fn new<B: Bus>() -> Self {
        Self {
            exec: h_exec::<B> as *const (),
            overflow: h_overflow as *const (),
            fused: h_fused as *const (),
            loop_end: 0,
            version_ptrs: [std::ptr::null(); 2],
            versions: [0; 2],
            #[cfg(feature = "wasm-cache-inline")]
            cache: std::ptr::null(),
        }
    }
    pub fn shared<B: Bus>() -> &'static Self {
        &const { Helpers::new::<B>() }
    }
}
const _: () = {
    assert!(size_of::<Helpers>() == if cfg!(feature = "wasm-cache-inline") { 36 } else { 32 });
    assert!(offset_of!(Helpers, overflow) == 4);
    assert!(offset_of!(Helpers, fused) == 8);
};
// Baseline WASM has no fused multiply-add opcode. Preserve Rust's single rounding
// without spilling integer register locals or invoking the instruction dispatcher.
extern "C" fn h_fused(s: u32, t: u32, r: u32, subtract: u32) -> u32 {
    let s = f32::from_bits(s ^ if subtract != 0 { 0x8000_0000 } else { 0 });
    s.mul_add(f32::from_bits(t), f32::from_bits(r)).to_bits()
}
extern "C" fn h_exec<B: Bus>(
    cpu: *mut Cpu,
    bus: *mut B,
    instruction: *const BlockInsn,
    pc: u32,
) -> u32 {
    // SAFETY: The compiled caller passes the exclusive live CPU/bus and an instruction
    // owned by its live CodeCache. No Rust execution overlaps generated access.
    let (cpu, bus, instruction) = unsafe { (&mut *cpu, &mut *bus, &*instruction) };
    HELPED.store(true, std::sync::atomic::Ordering::Relaxed);
    cpu.pc = pc;
    #[cfg(feature = "wasm-jit-profile")]
    {
        let c = crate::census::get();
        let core = crate::census::core(cpu);
        let i = &instruction.insn;
        use crate::Op::*;
        let addr = match i.op {
            L8ui | L16ui | L16si | L32i | L32iN | S8i | S16i | S32i | S32iN | Lsi | Ssi => Some(cpu.get_ar(i.s).wrapping_add(i.imm as u32)),
            L32r => Some(i.imm as u32),
            Pie => { let p = &crate::pie::OPS[i.imm as usize]; let o = crate::pie::extract(i.raw, p); o.has(crate::pie::Role::As).then(|| cpu.get_ar(o.get(crate::pie::Role::As) as u8)) }
            _ => None,
        };
        let nm = crate::census::name(i);
        *c.helpers.entry((core, nm.clone(), addr.map_or(0, |a| a >> 16))).or_default() += 1;
        if let Some(a) = addr { *c.slowmem.entry((core, nm, crate::census::region(a))).or_default() += 1; }
    }
    if crate::exec::defer_instruction(cpu, bus, &instruction.insn) {
        cpu.jit_trap = None;
        return 1;
    }
    bus.note_pc(pc);
    match exec_insn(cpu, bus, &instruction.insn) {
        Ok(()) => {
            if cpu.price_control {
                let taken = crate::exec::control_taken(cpu, &instruction.insn);
                cpu.timing_extra += crate::exec::control_price(instruction.insn.op, taken)
                    + u32::from(taken && crate::exec::transfers(instruction.insn.op) && crate::exec::straddles(bus, cpu.pc));
            }
            (bus.block_break() as u32) << 1
        }
        Err(t) => {
            cpu.jit_trap = Some(t);
            1
        }
    }
}
extern "C" fn h_overflow(cpu: *mut Cpu, max_ar: u32, pc: u32) -> u32 {
    // SAFETY: The generated caller has exclusive access to this CPU.
    let cpu = unsafe { &mut *cpu };
    cpu.pc = pc;
    match cpu.check_overflow(max_ar as u8) {
        Some(t) => {
            cpu.jit_trap = Some(t);
            1
        }
        None => 0,
    }
}

/// A loop may repeat only across an ordinary instruction boundary with no observer.
/// The decoder already cuts at interior observers; the loop head needs its own check.
pub fn loop_len(cc: &CodeCache, code: u32, cpu: &Cpu) -> Option<usize> {
    let b = &cc.blocks[code as usize];
    if cpu.blocks.observed || cpu.lcount == 0 || cpu.lbeg != b.pc
        || cpu.boundary_bloom & emu_core::core::pc_bit(b.pc) != 0 {
        return None;
    }
    b.instructions.iter().zip(&b.pcs).take(b.loop_prefix)
        .position(|(i, pc)| pc.wrapping_add(i.insn.len as u32) == cpu.lend)
        .map(|n| n + 1)
}

/// Execute a published block against the exclusively borrowed machine state.
/// Returns retired count in bits 0..16 and exit code in bits 16..19. For CODE_CUT,
/// bits 19..32 carry the next instruction index in the decoded block.
///
/// # Safety
/// `code` must be ready in this cache; `entry` must be its recorded instruction index.
/// `h` must have been created for B. FastMem must describe this bus and remain valid.
#[cfg_attr(feature = "wasm-cpu-profile", inline(never))]
pub unsafe fn run<B: Bus>(
    cc: &CodeCache,
    code: u32,
    cpu: &mut Cpu,
    bus: &mut B,
    h: &Helpers,
    budget: u32,
    entry: u32,
    fm: Option<FastMem>,
) -> u32 {
    let priced_fetch = cpu.price_control && cpu.icache_fill != 0;
    cpu.fetch_n = 0;
    let budget = if cpu.icache_fill != 0 { budget.min(64) } else { budget };
    // SAFETY: preserve the caller's live code, helper and memory guarantees.
    HELPED.store(false, std::sync::atomic::Ordering::Relaxed);
    cpu.blocks.chain_ei = NONE;
    // A dispatch at a probed PC stays one block long, as the differential suite requires.
    let chain = CHAIN && cpu.boundary_bloom & emu_core::core::pc_bit(cpu.pc) == 0;
    let mut result = unsafe { run_inner(cc, code, cpu, bus, h, budget, entry, fm) };
    // EX153: keep going inside this wrapper while nothing the dispatcher would look at can have
    // changed: a plain END/LEFT exit, no interpreter helper ran, credit remains, and the next PC
    // has a valid decoded entry with ready code that may start without a boundary check.
    if chain {
        let mut total = 0u32;
        loop {
            let exit = (result >> 16) & 7;
            let sofar = total + (result & 0xffff);
            if (exit != CODE_END && exit != CODE_LEFT) || sofar >= budget || cpu.blocks.observed
                || HELPED.load(std::sync::atomic::Ordering::Relaxed) { break; }
            let pc = cpu.pc;
            if cpu.boundary_bloom & emu_core::core::pc_bit(pc) != 0 { break; }
            let Some((ei, next)) = cpu.blocks.chain_target(pc, bus.page_versions()) else { break };
            let slot = cc.blocks[next as usize].slot.get();
            if slot == NONE || slot == 0 { break; }
            total = sofar;
            cpu.blocks.chain_ei = ei;
            #[cfg(feature = "wasm-jit-profile")]
            { let st = &cc.region_stats.ex153; st[11].set(st[11].get() + 1); }
            // SAFETY: the entry is valid for the current code pages and its code is ready in this cache.
            result = unsafe { run_inner(cc, next, cpu, bus, h, budget - total, 0, fm) };
        }
        result += total;
    }
    #[cfg(feature = "wasm-jit-profile")]
    {
        let st = &cc.region_stats.ex153;
        let add = |i: usize, n: u64| st[i].set(st[i].get() + n);
        add(0, 1);
        if budget >= 64 { add(1, 1); }
        add(10, budget as u64);
        if result & 0xffff == 0 { add(9, 1); }
    }
    if priced_fetch {
        let n = cpu.fetch_n.saturating_sub(u32::from(bus.deferred()));
        assert!(n <= 64, "compiled fetch trace exceeded its instruction budget");
        for index in 0..n as usize {
            let [lo, hi] = cpu.fetch_ring[index];
            cpu.touch_fetch_lines(lo, hi);
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
unsafe fn run_inner<B: Bus>(
    cc: &CodeCache,
    code: u32,
    cpu: &mut Cpu,
    bus: &mut B,
    h: &Helpers,
    budget: u32,
    entry: u32,
    fm: Option<FastMem>,
) -> u32 {
    type Run<B> =
        extern "C" fn(*mut Cpu, *mut B, *const Helpers, u32, u32, *const TlbEntry, *mut u32) -> u32;
    #[cfg(feature = "wasm-cache-inline")]
    let cache_view = if CACHE_PROBES.load(std::sync::atomic::Ordering::Relaxed) { bus.fast_cache() } else { None };
    #[cfg(feature = "wasm-cache-inline")]
    let hinted = Helpers { cache: cache_view.as_ref().map_or(std::ptr::null(), |v| v), ..*h };
    #[cfg(feature = "wasm-cache-inline")]
    let h = &hinted;
    let (tlb, versions) = fm
        .map(|m| (m.tlb, m.page_ver))
        .unwrap_or((std::ptr::null(), std::ptr::null_mut()));
    let b = &cc.blocks[code as usize];
    if entry == 0 && !cpu.blocks.observed {
        // EX136: the facts the checks below would fetch through the owning block, its region and
        // three of its vectors are cached in this block while no region has been dropped.
        let hot = b.hot.get();
        if hot.epoch == cc.region_epoch.get() && budget >= hot.len && cpu.boundary_bloom & hot.bloom == 0
            && (cpu.lcount == 0 || cpu.lend.wrapping_sub(hot.lo) > hot.span)
        {
            let pv = bus.page_versions();
            if hot.pages[..hot.npages as usize].iter().all(|&(i, v)| pv.get(i as usize).copied().unwrap_or(0) == v) {
                // SAFETY: as for the region call below; the epoch proves slot and sites are live.
                let f: Run<B> = unsafe { std::mem::transmute(hot.slot as usize) };
                let result = f(cpu, bus, h, budget.min(0xffff), hot.k, tlb, versions);
                let site = if (result >> 16) & 7 != CODE_REJECT {
                    assert!((result >> 19) < hot.nsites);
                    // SAFETY: index checked against the live vector's length.
                    Some(unsafe { *hot.sites.add((result >> 19) as usize) })
                } else { None };
                region_stats(cc, result, budget, site);
                if site.is_some() { census(0, 1); census(1, (result & 0xffff) as u64); }
                if let Some(site) = site {
                    bus.note_pc(site_pc(site));
                    return result & 0x7ffff;
                }
                return run_block_body(cc, code, cpu, bus, h, budget, entry, tlb, versions);
            }
        }
        // The region to run: this block's own, or the one covering this PC.
        let (owner, k) = if b.region.borrow().is_some() {
            (code, 0)
        } else {
            let cached = b.covered_by.get();
            let live = cached.0 != NONE
                && cc.blocks.get(cached.0 as usize).and_then(|o| o.region.borrow().as_ref()
                    .map(|r| r.chunks.get(cached.1 as usize).is_some_and(|c| c.pc == b.pc))).unwrap_or(false);
            // A temporary borrow in an `if let` would outlive the whole chain.
            let epoch = cc.coverage_epoch.get();
            let found = if live || b.uncovered_epoch.get() == epoch {
                None
            } else {
                let found = cc.covered.borrow().get(&b.pc).copied();
                if found.is_none() { b.uncovered_epoch.set(epoch); }
                found
            };
            if live {
                cached
            } else if let Some(found) = found {
                b.covered_by.set(found);
                #[cfg(feature = "wasm-jit-profile")]
                cc.region_stats.covered.set(cc.region_stats.covered.get() + 1);
                #[cfg(feature = "wasm-jit-tests")]
                REGION_STATS[10].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                found
            } else if b.region_tries.get() < REGION_TRIES {
                b.region_tries.set(b.region_tries.get() + 1);
                let formed = emitter::region::form(cpu, bus, b.pc, &b.instructions, b.fast).and_then(|f| {
                    let (bytes, sites) = emitter::region::generate(&f.chunks, &f.pages, &f.loops, b.fast);
                    // SAFETY: as for ready(): the host copies and installs the module.
                    let slot = unsafe { host_jit_compile(bytes.as_ptr(), bytes.len()) };
                    (slot != 0).then(|| Region {
                        lens: f.chunks.iter().map(|c| c.instructions.len() as u32).collect(),
                        chunks: f.chunks, slot, bytes: bytes.len(), bloom: f.bloom, lo: f.lo, hi: f.hi, loops: f.loops, pages: f.pages, sites,
                    })
                });
                if let Some(r) = &formed {
                    // Removal cannot invalidate a negative lookup; insertion can.
                    cc.coverage_epoch.set(cc.coverage_epoch.get().wrapping_add(1));
                    let mut covered = cc.covered.borrow_mut();
                    for (k, c) in r.chunks.iter().enumerate() { covered.entry(c.pc).or_insert((code, k as u32)); }
                    #[cfg(feature = "wasm-jit-profile")]
                    {
                        let st = &cc.region_stats;
                        st.formed.set(st.formed.get() + 1);
                        st.chunks.set(st.chunks.get() + r.chunks.len() as u64);
                        st.instructions.set(st.instructions.get() + r.chunks.iter().map(|c| c.instructions.len() as u64).sum::<u64>());
                        st.bytes.set(st.bytes.get() + r.bytes as u64);
                    }
                }
                #[cfg(feature = "wasm-jit-profile")]
                if formed.is_none() { cc.region_stats.failed.set(cc.region_stats.failed.get() + 1); }
                #[cfg(feature = "wasm-jit-tests")]
                REGION_STATS[if formed.is_some() { 0 } else { 1 }].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                *b.region.borrow_mut() = formed;
                (code, 0)
            } else {
                (NONE, 0)
            }
        };
        if owner != NONE {
            let rb = &cc.blocks[owner as usize];
            let region = rb.region.borrow();
            if let Some(r) = region.as_ref() {
                let pv = bus.page_versions();
                let current = r.pages.iter().all(|&(i, v)| pv.get(i as usize).copied().unwrap_or(0) == v);
                if !current {
                    // Some chunk's code changed: rebuild the region from the new code later.
                    drop(region);
                    rb.drop_region(owner, &cc.covered);
                    cc.region_epoch.set(cc.region_epoch.get() + 1);
                    #[cfg(feature = "wasm-jit-profile")]
                    cc.region_stats.dropped.set(cc.region_stats.dropped.get() + 1);
                    #[cfg(feature = "wasm-jit-tests")]
                    REGION_STATS[9].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                } else if budget >= r.lens[k as usize]
                    && cpu.boundary_bloom & r.bloom == 0
                    && (cpu.lcount == 0
                        || cpu.lend.wrapping_sub(r.lo) > r.hi.wrapping_sub(r.lo)
                        || r.loops.contains(&(cpu.lend, cpu.lbeg)))
                {
                    // SAFETY: the region was installed with the block signature; its
                    // entry parameter is the chunk index.
                    let f: Run<B> = unsafe { std::mem::transmute(r.slot as usize) };
                    if r.pages.len() <= 8 {
                        let mut pages = [(0, 0); 8];
                        pages[..r.pages.len()].copy_from_slice(&r.pages);
                        b.hot.set(Hot { epoch: cc.region_epoch.get(), bloom: r.bloom, slot: r.slot, k, len: r.lens[k as usize], lo: r.lo,
                            span: r.hi.wrapping_sub(r.lo), pages, npages: r.pages.len() as u32, nsites: r.sites.len() as u32, sites: r.sites.as_ptr() });
                    }
                    let result = f(cpu, bus, h, budget.min(0xffff), k, tlb, versions);
                    let site = if (result >> 16) & 7 != CODE_REJECT {
                        assert!(((result >> 19) as usize) < r.sites.len(), "region {:x}: result {result:#x} sites {}", rb.pc, r.sites.len());
                        Some(r.sites[(result >> 19) as usize])
                    } else { None };
                    region_stats(cc, result, budget, site);
                    if site.is_some() { census(0, 1); census(1, (result & 0xffff) as u64); }
                    if let Some(site) = site {
                        bus.note_pc(site_pc(site));
                        return result & 0x7ffff;
                    }
                }
            }
        }
    }
    run_block_body(cc, code, cpu, bus, h, budget, entry, tlb, versions)
}

/// Test and profile counters of one region call.
#[inline(always)]
#[allow(unused_variables)]
fn region_stats(cc: &CodeCache, result: u32, budget: u32, site: Option<ExitSite>) {
    #[cfg(feature = "wasm-jit-tests")]
    {
        use std::sync::atomic::Ordering::Relaxed;
        REGION_STATS[2].fetch_max(result & 0xffff, Relaxed);
        REGION_STATS[3 + ((result >> 16) & 7) as usize].fetch_add(1, Relaxed);
        REGION_STATS[11].fetch_max(budget, Relaxed);
    }
    #[cfg(feature = "wasm-jit-profile")]
    {
        let st = &cc.region_stats;
        st.calls.set(st.calls.get() + 1);
        let exit = ((result >> 16) & 7) as usize;
        if exit == CODE_REJECT as usize {
            st.rejected.set(st.rejected.get() + 1);
        } else {
            st.retired.set(st.retired.get() + (result & 0xffff) as u64);
            st.exits[exit].set(st.exits[exit].get() + 1);
            if exit == CODE_LEFT as usize {
                let kind = site.expect("a region LEFT exit has a site").1 as usize;
                st.left_kinds[kind].set(st.left_kinds[kind].get() + 1);
            }
        }
    }
}

/// The block's own module: whole, resumed or as a retained hardware loop.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
unsafe fn run_block_body<B: Bus>(cc: &CodeCache, code: u32, cpu: &mut Cpu, bus: &mut B, h: &Helpers, budget: u32, entry: u32, tlb: *const TlbEntry, versions: *mut u32) -> u32 {
    type Run<B> =
        extern "C" fn(*mut Cpu, *mut B, *const Helpers, u32, u32, *const TlbEntry, *mut u32) -> u32;
    // SAFETY: host_jit_compile installs exactly this signature in the shared WASM table.
    let f: Run<B> = unsafe { std::mem::transmute(cc.blocks[code as usize].slot.get() as usize) };
    let b = &cc.blocks[code as usize];
    let looping = loop_len(cc, code, cpu);
    let initial_lcount = cpu.lcount;
    let result = if looping.is_some() {
        let mut guarded = *h;
        let last = b.pcs.last().unwrap().wrapping_add(b.instructions.last().unwrap().insn.len as u32 - 1);
        let indices = [bus.code_page(b.pc), bus.code_page(last)];
        let pv = bus.page_versions();
        if let (Some(a), Some(z)) = (pv.get(indices[0] as usize), pv.get(indices[1] as usize)) {
            guarded.loop_end = cpu.lend;
            guarded.version_ptrs = [a as *const u32, z as *const u32];
            guarded.versions = [*a, *z];
        }
        f(cpu, bus, &guarded, budget.min(0xffff), entry, tlb, versions)
    } else {
        f(cpu, bus, h, budget.min(0xffff), entry, tlb, versions)
    };
    let done = result & 0xffff;
    {
        let bytes = b.pcs.last().unwrap().wrapping_add(b.instructions.last().unwrap().insn.len as u32).wrapping_sub(b.pc);
        let noloop = initial_lcount == 0 || cpu.lend.wrapping_sub(b.pc) > bytes;
        if entry == 0 && budget as usize >= b.instructions.len() { census(3, 1); census(4, done as u64); }
        else if noloop { census(5, 1); census(6, done as u64); if entry != 0 { census(2, done as u64); } }
        else { census(7, done as u64); }
    }
    // LCOUNT changes only at the admitted hardware backedge. Subtract repeated
    // prefixes to locate both the last retired instruction and a cut continuation.
    let repeated = looping.map_or(0, |n| (initial_lcount - cpu.lcount) as usize * n);
    let offset = (entry + done) as usize - repeated;
    if done > 0 {
        #[cfg(feature = "wasm-jit-profile")]
        if looping.is_some() {
            let retained = initial_lcount - cpu.lcount - u32::from(offset == 0);
            cpu.blocks.profile.record_loop(b.pc, retained);
        }
        // Offset zero means the last retired instruction took a hardware backedge.
        // The destination PC alone cannot prove that: a suffix branch may target LBEG.
        let last = if offset == 0 { looping.unwrap() - 1 } else { offset - 1 };
        let pc = *b.pcs.get(last).unwrap_or_else(|| panic!("block {:x} {:?} entry {entry} done {done} budget {budget} looping {looping:?} lcount {initial_lcount}->{} result {result:#x}",
            b.pc, b.instructions.iter().map(|i| i.insn.op).collect::<Vec<_>>(), cpu.lcount));
        bus.note_pc(pc);
    }
    #[cfg(feature = "wasm-jit-profile")]
    {
        let st = &cc.region_stats.ex153;
        let add = |i: usize, n: u64| st[i].set(st[i].get() + n);
        let cut = result >> 16 == CODE_CUT;
        if entry != 0 { add(6, 1); add(7, done as u64); if cut { add(8, 1); } }
        else if cut { add(4, 1); add(5, done as u64); }
        else { add(2, 1); add(3, done as u64); }
    }
    // Reuse the offset already reconstructed above instead of scanning decoded PCs
    // again in run_block_inner. Regions never return CODE_CUT.
    if result >> 16 == CODE_CUT { result | ((offset as u32) << 19) } else { result }
}

#[cfg(feature = "wasm-jit-profile")]
pub fn census_supported(i: &crate::Insn, fast: bool) -> bool { emitter::supported_insn(i, fast) }
#[cfg(feature = "wasm-jit-profile")]
pub fn census_terminal(op: crate::Op) -> bool { emitter::terminal_helper(op) }
#[path = "wasm_emit.rs"]
mod emitter;
use emitter::generate;

#[cfg(feature = "wasm-jit-profile")]
#[path = "wasm_profile.rs"]
pub mod profile;

#[cfg(feature = "wasm-jit-tests")]
#[path = "wasm_tests.rs"]
pub mod tests;
