//! Bounded regions: a hot block and the blocks reachable from it over statically known
//! edges (fallthrough, conditional-branch target, J) compiled as one function. Guest
//! registers stay in locals across internal edges; the only per-edge work is the credit
//! check and a jump. Everything that could make an internal boundary observable exits the
//! region instead: helpers set DIRTY, probes and self-modifying code are checked by the
//! caller, and window rotation, hardware loops and dynamic transfers are never admitted.
use super::*;
use crate::block::{ends_block, must_start_block, pc_bit, MAX_LEN};
use crate::decode::decode;
use crate::exec::max_ar;
use std::collections::HashMap;

pub(super) const MAX_CHUNKS: usize = 8;
pub(super) const MAX_INSNS: usize = 64;
const MAX_PAGES: usize = 4;

/// One straight-line piece of a region, decoded independently of the block cache.
pub(in crate::jit) struct Chunk {
    pub pc: u32,
    pub instructions: Vec<BlockInsn>,
}

pub(in crate::jit) struct Formed {
    pub chunks: Vec<Chunk>,
    /// Every instruction PC, head included: a probe there must stop the region being used.
    pub bloom: u64,
    /// Lowest PC and highest end address; an active hardware loop ending inside rejects.
    pub lo: u32,
    pub hi: u32,
    /// Code pages and the versions the chunks were decoded from.
    pub pages: Vec<(u32, u32)>,
}

pub(super) struct RegionGen {
    /// chunk head pc -> (chunk index, instruction count)
    pub heads: HashMap<u32, (usize, u32)>,
    pub current: usize,
    /// control depth just inside the dispatch loop
    pub loop_depth: usize,
    /// control depth at the top level of the current chunk's code
    pub chunk_depth: usize,
    /// last retired PC for each exit site, indexed by the tag in the result
    pub sites: Vec<u32>,
    /// version-page index range covering every chunk (stores inside it set DIRTY)
    pub page_lo: u32,
    pub page_hi: u32,
}

fn eligible(op: crate::Op, fast: bool) -> bool {
    use crate::Op::*;
    supported(op, fast)
        && !terminal_helper(op)
        && !matches!(op, Entry | Loop | Loopnez | Loopgtz | Jx)
}

fn conditional(op: crate::Op) -> bool {
    use crate::Op::*;
    matches!(
        op,
        Beqz | BeqzN | Bnez | BnezN | Bltz | Bgez | Beqi | Bnei | Blti | Bgei | Bltui | Bgeui
            | Beq | Bne | Blt | Bge | Bltu | Bgeu | Bbci | Bbsi | Bbc | Bbs | Bf | Bt
    )
}

/// Where control goes after `chunk`, statically.
fn successors(chunk: &Chunk) -> Vec<u32> {
    let last = chunk.instructions.last().unwrap();
    let next = chunk.pc.wrapping_add(chunk.instructions.iter().map(|i| i.insn.len as u32).sum());
    match last.insn.op {
        crate::Op::J => vec![last.insn.imm as u32],
        op if conditional(op) => vec![last.insn.imm as u32, next],
        _ => vec![next],
    }
}

/// Decode a chunk at `pc0`, the way the block decoder would, stopping before anything
/// the region cannot contain. A non-head chunk may not start at a probe boundary.
fn chunk<B: Bus>(cpu: &Cpu, bus: &mut B, head: u32, pc0: u32, fast: bool, room: usize, bloom: &mut u64)
    -> Option<Vec<BlockInsn>> {
    let mut v: Vec<BlockInsn> = Vec::new();
    let mut pc = pc0;
    while v.len() < room.min(MAX_LEN) {
        let Ok(bytes) = bus.fetch(pc) else { break };
        let i = decode(pc, bytes);
        if i.len == 0 || !eligible(i.op, fast) { break }
        if pc != head && (must_start_block(&i) || cpu.boundary_bloom & pc_bit(pc) != 0) { break }
        v.push(BlockInsn { insn: i, max_ar: max_ar(&i), off: v.len() as u32 });
        // Includes the head: an internal backedge to it would skip a probe there.
        *bloom |= pc_bit(pc);
        pc = pc.wrapping_add(i.len as u32);
        if ends_block(&i) { break }
    }
    (!v.is_empty()).then_some(v)
}

/// `block` is the head block's own decode: the chunk found in memory must agree with it,
/// which is always so for a validated entry and never for a synthetic test block.
pub(in crate::jit) fn form<B: Bus>(cpu: &Cpu, bus: &mut B, head: u32, block: &[BlockInsn], fast: bool) -> Option<Formed> {
    let mut bloom = 0u64;
    let first = chunk(cpu, bus, head, head, fast, MAX_INSNS, &mut bloom)?;
    let n = first.len().min(block.len());
    if first[..n].iter().zip(&block[..n]).any(|(a, b)| a.insn != b.insn) { return None }
    let mut total = first.len();
    let mut chunks = vec![Chunk { pc: head, instructions: first }];
    let mut seen: HashMap<u32, ()> = HashMap::from([(head, ())]);
    let mut q = 0;
    while q < chunks.len() && chunks.len() < MAX_CHUNKS && total < MAX_INSNS {
        for t in successors(&chunks[q]) {
            if seen.contains_key(&t) || chunks.len() == MAX_CHUNKS || total >= MAX_INSNS { continue }
            if let Some(c) = chunk(cpu, bus, head, t, fast, MAX_INSNS - total, &mut bloom) {
                total += c.len();
                seen.insert(t, ());
                chunks.push(Chunk { pc: t, instructions: c });
            }
        }
        q += 1;
    }
    if chunks.len() < 2 { return None }
    let (mut lo, mut hi) = (u32::MAX, 0u32);
    let mut pages: Vec<(u32, u32)> = Vec::new();
    for c in &chunks {
        let mut pc = c.pc;
        for bi in &c.instructions {
            let end = pc.wrapping_add(bi.insn.len as u32);
            lo = lo.min(pc);
            hi = hi.max(end);
            for a in [pc, end - 1] {
                let p = bus.code_page(a);
                if !pages.iter().any(|&(i, _)| i == p) { pages.push((p, 0)); }
            }
            pc = end;
        }
    }
    if pages.len() > MAX_PAGES { return None }
    let pv = bus.page_versions();
    for (i, v) in &mut pages { *v = pv.get(*i as usize).copied().unwrap_or(0); }
    Some(Formed { chunks, bloom, lo, hi, pages })
}

/// Retire the current instruction and continue at `target`: inside the region when it is
/// a chunk head, the credit covers that chunk and no code page was written; else exit.
/// `direct` is the final edge of a chunk's code: only that one may fall into the
/// next chunk's code without a branch.
pub(super) fn region_edge(g: &mut Gen, target: u32, direct: bool) {
    g.flush();
    let r = g.region.as_ref().unwrap();
    let (current, loop_depth, chunk_depth) = (r.current, r.loop_depth, r.chunk_depth);
    match r.heads.get(&target).copied() {
        Some((index, len)) => {
            g.get(DONE);
            g.c(len);
            g.op(0x6a);
            g.get(3);
            g.op(0x4b);
            g.get(DIRTY);
            g.op(0x72);
            g.begin_if();
            g.spill();
            g.cpu_const(PC, target);
            g.ret_value(CODE_LEFT);
            g.end();
            if !direct || index != current + 1 || g.depth() != chunk_depth {
                g.c(index as u32);
                g.set(NEXT);
                g.op(0x0c);
                let label = g.depth() - loop_depth;
                uleb(&mut g.bytes, label);
            }
        }
        None => {
            g.spill();
            g.cpu_const(PC, target);
            g.ret_value(CODE_LEFT);
        }
    }
}

pub(in crate::jit) fn generate(chunks: &[Chunk], pages: &[(u32, u32)], fast: bool) -> (Vec<u8>, Vec<u32>) {
    let page_lo = pages.iter().map(|p| p.0).min().unwrap_or(0);
    let page_hi = pages.iter().map(|p| p.0).max().unwrap_or(0);
    let all = || chunks.iter().flat_map(|c| c.instructions.iter());
    let registers = all().fold(0u16, |m, bi| m | bi.insn.gpr_effects().touched());
    let written = all().fold(0u16, |m, bi| {
        let e = bi.insn.gpr_effects();
        m | e.writes | e.conditional_writes | e.unclassified
    });
    let max_ar = all().map(|bi| bi.max_ar).max().unwrap_or(0);
    let float = all().any(|bi| float::requires_coprocessor(bi.insn.op));
    let heads = chunks.iter().enumerate().map(|(i, c)| (c.pc, (i, c.instructions.len() as u32))).collect();
    let mut g = Gen {
        loaded: registers,
        written,
        max_ar,
        dynamic: true,
        region: Some(RegionGen { heads, current: 0, loop_depth: 0, chunk_depth: 0, sites: Vec::new(), page_lo, page_hi }),
        ..Gen::default()
    };
    // Only a fresh entry with credit for the whole head chunk may run here; anything
    // else takes the head block's own module, which handles cuts and resumes.
    g.get(4);
    g.get(3);
    g.c(chunks[0].instructions.len() as u32);
    g.op(0x49);
    g.op(0x72);
    g.begin_if();
    g.c(CODE_REJECT << 16);
    g.op(0x0f);
    g.end();
    g.reload();
    if max_ar >= 4 {
        g.get(WINDOWS);
        g.c((1 << (max_ar / 4)) - 1);
        g.op(0x71);
        g.begin_if();
        g.c(CODE_REJECT << 16);
        g.op(0x0f);
        g.end();
    }
    if float {
        g.cpu(offset_of!(Cpu, cpenable));
        g.c(1);
        g.op(0x71);
        g.op(0x45);
        g.begin_if();
        g.c(CODE_REJECT << 16);
        g.op(0x0f);
        g.end();
    }
    g.c(0);
    g.set(DIRTY);
    g.c(0);
    g.set(NEXT);
    g.begin_loop();
    let loop_depth = g.depth();
    let n = chunks.len();
    for _ in 0..n {
        g.begin_block();
    }
    g.get(NEXT);
    g.op(0x0e);
    uleb(&mut g.bytes, n);
    for k in 0..=n {
        uleb(&mut g.bytes, k.min(n - 1));
    }
    for (k, chunk) in chunks.iter().enumerate() {
        g.end();
        g.pending = 0;
        {
            let r = g.region.as_mut().unwrap();
            r.current = k;
            r.loop_depth = loop_depth;
            r.chunk_depth = g.ctl.len();
        }
        emit_body(&mut g, chunk.pc, &chunk.instructions, fast, false, true, float);
    }
    g.op(0x00); // every chunk leaves or branches; no fallthrough out of the last one
    g.end(); // dispatch loop
    g.op(0x00);
    g.end(); // function body
    let sites = g.region.take().unwrap().sites;
    (finish(g, chunks[0].pc), sites)
}
