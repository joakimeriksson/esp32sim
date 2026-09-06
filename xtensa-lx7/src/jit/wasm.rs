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
const HOT: u32 = 32;
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
    generation: u64,
    hits: Cell<u32>,
    slot: Cell<u32>,
    bytes: Cell<usize>,
    /// A region headed by this block, once it is hot and one could be formed.
    region: RefCell<Option<Region>>,
    region_tries: Cell<u8>,
}
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
    sites: Vec<u32>,
}
// Compiled instructions own their backing storage, independently of the decoder arena.
// A decoder flush invalidates every handle before reset may compact this cache.
pub struct CodeCache {
    blocks: Vec<Block>,
    by_pc: HashMap<(u32, usize, bool), u32>,
    generation: u64,
}
impl Block {
    fn release(&self) {
        if self.slot.get() != NONE && self.slot.get() != 0 {
            // SAFETY: reset/drop happen only when no compiled block is executing.
            unsafe {
                host_jit_release(self.slot.get());
            }
        }
        self.drop_region();
    }
    fn drop_region(&self) {
        if let Some(r) = self.region.borrow_mut().take() {
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
        })
    }
    pub fn used(&self) -> usize {
        self.blocks.iter().map(|b| b.size()).sum()
    }
    pub fn reset(&mut self) {
        self.generation += 1;
        // Keep recently decoded blocks across arena turnover. Prefer recent code under
        // pressure; enforce these retention limits only after all decoder handles die.
        self.blocks.sort_by_key(|b| std::cmp::Reverse(b.generation));
        let (mut bytes, mut count) = (0, 0);
        self.blocks.retain(|b| {
            let keep = self.generation - b.generation <= 2
                && count < RETAIN_BLOCKS
                && bytes + b.size() <= RETAIN_BYTES;
            if keep {
                bytes += b.size();
                count += 1;
            } else {
                b.release();
            }
            keep
        });
        self.by_pc.clear();
        for (id, b) in self.blocks.iter().enumerate() {
            self.by_pc
                .insert((b.pc, b.instructions.len(), b.fast), id as u32);
        }
    }
}
impl Drop for CodeCache {
    fn drop(&mut self) {
        for b in &self.blocks {
            b.release();
        }
    }
}

pub fn compile(
    cc: &mut CodeCache,
    instructions: &mut [BlockInsn],
    pc: u32,
    fast: bool,
) -> Option<u32> {
    if instructions.len() < 2
        || !instructions.iter().enumerate().all(|(n, i)| {
            let last = n + 1 == instructions.len();
            (!emitter::terminal_helper(i.insn.op) || last)
                && (emitter::supported(i.insn.op, fast)
                    || (last && emitter::terminal_helper(i.insn.op)))
        })
    {
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
        // A block that writes LCOUNT through LOOP* must never be admitted as a retained
        // hardware loop: run() locates the last executed instruction from LCOUNT deltas.
        loop_prefix: if instructions.iter().any(|i| matches!(i.insn.op, crate::Op::Loop | crate::Op::Loopnez | crate::Op::Loopgtz)) { 0 }
            else { instructions.iter().take_while(|i| emitter::loop_safe(i.insn.op, fast)).count() },
        instructions: instructions.to_vec(),
        pc,
        fast,
        generation: cc.generation,
        hits: Cell::new(0),
        slot: Cell::new(NONE),
        bytes: Cell::new(0),
        region: RefCell::new(None),
        region_tries: Cell::new(0),
    });
    cc.by_pc.insert(key, id);
    id
}
pub fn ready(cc: &CodeCache, code: u32) -> bool {
    let b = &cc.blocks[code as usize];
    if b.slot.get() == NONE {
        let hits = b.hits.get() + 1;
        b.hits.set(hits);
        if hits < HOT {
            return false;
        }
        let bytes = generate(b);
        // SAFETY: The host synchronously copies these bytes, installs a module using the
        // shared memory/table, and returns a correctly typed function slot or zero.
        let slot = unsafe { host_jit_compile(bytes.as_ptr(), bytes.len()) };
        b.slot.set(slot);
        b.bytes.set(if slot == 0 { 0 } else { bytes.len() });
    }
    b.slot.get() != 0
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Helpers {
    exec: usize,
    overflow: usize,
    fused: usize,
    loop_end: u32,
    version_ptrs: [*const u32; 2],
    versions: [u32; 2],
}
impl Helpers {
    pub fn new<B: Bus>() -> Self {
        Self {
            exec: h_exec::<B> as *const () as usize,
            overflow: h_overflow as *const () as usize,
            fused: h_fused as *const () as usize,
            loop_end: 0,
            version_ptrs: [std::ptr::null(); 2],
            versions: [0; 2],
        }
    }
}
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
    cpu.pc = pc;
    bus.note_pc(pc);
    match exec_insn(cpu, bus, &instruction.insn) {
        Ok(()) => (bus.block_break() as u32) << 1,
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
    type Run<B> =
        extern "C" fn(*mut Cpu, *mut B, *const Helpers, u32, u32, *const TlbEntry, *mut u32) -> u32;
    // SAFETY: host_jit_compile installs exactly this signature in the shared WASM table.
    let f: Run<B> = unsafe { std::mem::transmute(cc.blocks[code as usize].slot.get() as usize) };
    let (tlb, versions) = fm
        .map(|m| (m.tlb, m.page_ver))
        .unwrap_or((std::ptr::null(), std::ptr::null_mut()));
    let b = &cc.blocks[code as usize];
    if entry == 0 && !cpu.blocks.observed {
        if b.region_tries.get() < REGION_TRIES && b.region.borrow().is_none() {
            b.region_tries.set(b.region_tries.get() + 1);
            let formed = emitter::region::form(cpu, bus, b.pc, &b.instructions, b.fast).and_then(|f| {
                let (bytes, sites) = emitter::region::generate(&f.chunks, &f.pages, &f.loops, b.fast);
                // SAFETY: as for ready(): the host copies and installs the module.
                let slot = unsafe { host_jit_compile(bytes.as_ptr(), bytes.len()) };
                (slot != 0).then(|| Region {
                    chunks: f.chunks, slot, bytes: bytes.len(), bloom: f.bloom, lo: f.lo, hi: f.hi, loops: f.loops, pages: f.pages, sites,
                })
            });
            #[cfg(feature = "wasm-jit-tests")]
            REGION_STATS[if formed.is_some() { 0 } else { 1 }].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *b.region.borrow_mut() = formed;
        }
        let region = b.region.borrow();
        if let Some(r) = region.as_ref() {
            let pv = bus.page_versions();
            let current = r.pages.iter().all(|&(i, v)| pv.get(i as usize).copied().unwrap_or(0) == v);
            if !current {
                // Some chunk's code changed: rebuild the region from the new code later.
                drop(region);
                b.drop_region();
            } else if cpu.boundary_bloom & r.bloom == 0
                && (cpu.lcount == 0
                    || cpu.lend.wrapping_sub(r.lo) > r.hi.wrapping_sub(r.lo)
                    || r.loops.contains(&(cpu.lend, cpu.lbeg)))
            {
                // SAFETY: the region was installed with the block signature.
                let f: Run<B> = unsafe { std::mem::transmute(r.slot as usize) };
                let result = f(cpu, bus, h, budget.min(0xffff), 0, tlb, versions);
                #[cfg(feature = "wasm-jit-tests")]
                {
                    use std::sync::atomic::Ordering::Relaxed;
                    REGION_STATS[2].fetch_max(result & 0xffff, Relaxed);
                    REGION_STATS[3 + ((result >> 16) & 7) as usize].fetch_add(1, Relaxed);
                    REGION_STATS[11].fetch_max(budget, Relaxed);
                }
                if (result >> 16) & 7 != CODE_REJECT {
                    assert!(((result >> 19) as usize) < r.sites.len(), "region {:x}: result {result:#x} sites {}", b.pc, r.sites.len());
                    bus.note_pc(r.sites[(result >> 19) as usize]);
                    return result & 0x7ffff;
                }
            }
        }
    }
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
    if done > 0 {
        // LCOUNT changes only at the admitted hardware backedge. Subtract repeated
        // prefixes when locating the last executed instruction (including slow exits).
        let repeated = looping.map_or(0, |n| (initial_lcount - cpu.lcount) as usize * n);
        let offset = (entry + done) as usize - repeated;
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
    // Direct memory accesses need no peripheral callbacks. Preserve the last instruction PC
    // for subsequent bus diagnostics just as the interpreter does.
    result
}

#[path = "wasm_emit.rs"]
mod emitter;
use emitter::generate;

#[cfg(feature = "wasm-jit-profile")]
#[path = "wasm_profile.rs"]
pub mod profile;

#[cfg(feature = "wasm-jit-tests")]
#[path = "wasm_tests.rs"]
pub mod tests;
