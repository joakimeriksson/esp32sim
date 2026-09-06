//! Bounded regions: a hot block and the blocks reachable from it over statically known
//! edges (fallthrough, conditional-branch target, J, and the backedge of a hardware loop
//! set up inside the region) compiled as one function. Guest registers stay in locals
//! across internal edges; the only per-edge work is the credit check and a jump.
//! Everything that could make an internal boundary observable exits the region instead:
//! helpers set DIRTY, probes and self-modifying code are checked by the caller, calls,
//! returns and computed jumps end the region, and the one admitted window rotation
//! (ENTRY) re-proves the window before anything after it runs.
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
    /// Hardware loops set up inside the region, as (LEND, LBEG): their backedges are
    /// internal edges, and an entry with exactly this loop active is admitted.
    pub loops: Vec<(u32, u32)>,
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
    /// LEND -> LBEG for the region's own hardware loops
    pub loops: HashMap<u32, u32>,
}

/// May appear anywhere in a chunk.
fn eligible(i: &crate::Insn, fast: bool) -> bool {
    supported_insn(i, fast) && !terminal(i.op)
}

/// Ends a chunk and leaves the region by itself: calls, returns and computed jumps.
/// Direct calls and JX are emitted; returns run through the terminal helper.
fn terminal(op: crate::Op) -> bool {
    terminal_helper(op) || op == crate::Op::Jx
}

fn conditional(op: crate::Op) -> bool {
    use crate::Op::*;
    matches!(
        op,
        Beqz | BeqzN | Bnez | BnezN | Bltz | Bgez | Beqi | Bnei | Blti | Bgei | Bltui | Bgeui
            | Beq | Bne | Blt | Bge | Bltu | Bgeu | Bbci | Bbsi | Bbc | Bbs | Bf | Bt
    )
}

fn chunk_end(chunk: &Chunk) -> u32 {
    chunk.pc.wrapping_add(chunk.instructions.iter().map(|i| i.insn.len as u32).sum())
}

/// Where control goes after `chunk`, statically.
fn successors(chunk: &Chunk) -> Vec<u32> {
    use crate::Op::*;
    let last = chunk.instructions.last().unwrap();
    let next = chunk_end(chunk);
    match last.insn.op {
        J => vec![last.insn.imm as u32],
        op if terminal(op) => vec![],
        Loopnez | Loopgtz => vec![next, last.insn.imm as u32],
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
        if i.len == 0 || !(eligible(&i, fast) || terminal(i.op)) { break }
        if pc != head && (must_start_block(&i) || cpu.boundary_bloom & pc_bit(pc) != 0) { break }
        v.push(BlockInsn { insn: i, max_ar: max_ar(&i), off: v.len() as u32 });
        // Includes the head: an internal backedge to it would skip a probe there.
        *bloom |= pc_bit(pc);
        pc = pc.wrapping_add(i.len as u32);
        if ends_block(&i) || terminal(i.op) { break }
    }
    (!v.is_empty()).then_some(v)
}

/// A hardware loop's last instruction ends exactly at LEND; make that a chunk boundary
/// so the backedge can be an ordinary edge to the LBEG chunk. Each split adds at most
/// one chunk per loop, so the total stays within MAX_CHUNKS plus the loop count.
fn split_at_loop_ends(chunks: &mut Vec<Chunk>, loops: &[(u32, u32)]) {
    let mut k = 0;
    while k < chunks.len() {
        let mut pc = chunks[k].pc;
        let mut split = None;
        for (j, bi) in chunks[k].instructions.iter().enumerate() {
            pc = pc.wrapping_add(bi.insn.len as u32);
            if j + 1 < chunks[k].instructions.len() && loops.iter().any(|&(lend, _)| lend == pc) {
                split = Some((j + 1, pc));
                break;
            }
        }
        if let Some((at, pc)) = split {
            let mut tail = chunks[k].instructions.split_off(at);
            // The loop exit is usually a chunk head already (the LOOPNEZ skip target).
            if !chunks.iter().any(|c| c.pc == pc) {
                for (n, bi) in tail.iter_mut().enumerate() { bi.off = n as u32; }
                chunks.push(Chunk { pc, instructions: tail });
            }
        }
        k += 1;
    }
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
    let mut loops: Vec<(u32, u32)> = Vec::new();
    for c in &chunks {
        let mut pc = c.pc;
        for bi in &c.instructions {
            let next = pc.wrapping_add(bi.insn.len as u32);
            if matches!(bi.insn.op, crate::Op::Loop | crate::Op::Loopnez | crate::Op::Loopgtz) {
                loops.push((bi.insn.imm as u32, next));
            }
            pc = next;
        }
    }
    // A loop is only usable when its body and end are region chunks; two loops sharing
    // an end would make the backedge target ambiguous.
    loops.sort_unstable();
    loops.dedup();
    if loops.windows(2).any(|w| w[0].0 == w[1].0) { return None }
    split_at_loop_ends(&mut chunks, &loops);
    if chunks.len() > MAX_CHUNKS + loops.len() { return None }
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
    Some(Formed { chunks, loops, bloom, lo, hi, pages })
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

pub(in crate::jit) fn generate(chunks: &[Chunk], pages: &[(u32, u32)], formed_loops: &[(u32, u32)], fast: bool) -> (Vec<u8>, Vec<u32>) {
    let page_lo = pages.iter().map(|p| p.0).min().unwrap_or(0);
    let page_hi = pages.iter().map(|p| p.0).max().unwrap_or(0);
    let all = || chunks.iter().flat_map(|c| c.instructions.iter());
    // A return helper reads the CPU after the spill and exits: it needs no operands
    // loaded, exactly as at the end of a single block.
    let emitted = |bi: &BlockInsn| supported_insn(&bi.insn, fast) || !terminal_helper(bi.insn.op);
    let registers = all().filter(|bi| emitted(bi)).fold(0u16, |m, bi| m | bi.insn.gpr_effects().touched());
    let written = all().filter(|bi| emitted(bi)).fold(0u16, |m, bi| {
        let e = bi.insn.gpr_effects();
        m | e.writes | e.conditional_writes | e.unclassified
    });
    let max_ar = all().map(|bi| bi.max_ar).max().unwrap_or(0);
    // An ENTRY head rotates the window before the rest runs, so the proof for the rest
    // follows it; the entry-time proof then covers only ENTRY's own operand, which a
    // malformed `entry aN` with N >= 4 needs before the interpreter helper runs it.
    let entry_head = chunks[0].instructions[0].insn.op == crate::Op::Entry;
    let guard_max_ar = if entry_head { chunks[0].instructions[0].max_ar } else { max_ar };
    // Every instruction is emitted, so both coprocessor bits can be proved at entry.
    let cp = (all().any(|bi| float::requires_coprocessor(bi.insn.op)) as u32)
        | if all().any(|bi| bi.insn.op == crate::Op::Pie) { pie::CP3 } else { 0 };
    let heads = chunks.iter().enumerate().map(|(i, c)| (c.pc, (i, c.instructions.len() as u32))).collect();
    let loops = formed_loops.iter().copied().collect();
    let mut g = Gen {
        loaded: registers,
        written,
        max_ar,
        dynamic: true,
        region: Some(RegionGen { heads, current: 0, loop_depth: 0, chunk_depth: 0, sites: Vec::new(), page_lo, page_hi, loops }),
        ..Gen::default()
    };
    // The caller has checked the credit for the entry chunk; window and coprocessor
    // state are proved here. Anything else takes a block module, which handles cuts.
    g.reload();
    if guard_max_ar >= 4 {
        g.get(WINDOWS);
        g.c((1 << (guard_max_ar / 4)) - 1);
        g.op(0x71);
        g.begin_if();
        g.c(CODE_REJECT << 16);
        g.op(0x0f);
        g.end();
    }
    if cp != 0 {
        g.cpu(offset_of!(Cpu, cpenable));
        g.c(cp);
        g.op(0x71);
        g.c(cp);
        g.op(0x47);
        g.begin_if();
        g.c(CODE_REJECT << 16);
        g.op(0x0f);
        g.end();
    }
    g.c(0);
    g.set(DIRTY);
    g.get(4); // the entry chunk
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
        emit_body(&mut g, chunk.pc, &chunk.instructions, fast, false, true, cp);
    }
    g.op(0x00); // every chunk leaves or branches; no fallthrough out of the last one
    g.end(); // dispatch loop
    g.op(0x00);
    g.end(); // function body
    let sites = g.region.take().unwrap().sites;
    (finish(g, chunks[0].pc), sites)
}
