//! Bounded regions: a hot block and the blocks reachable from it over statically known
//! edges (fallthrough, conditional-branch target, J, and the backedge of a hardware loop
//! set up inside the region or active when it formed) compiled as one function. Guest
//! registers stay in locals across internal edges; the only per-edge work is the credit
//! check and a jump.
//! Everything that could make an internal boundary observable exits the region instead:
//! helpers set DIRTY, probes and self-modifying code are checked by the caller, calls,
//! returns and computed jumps end the region, and the one admitted window rotation
//! (ENTRY) re-proves the window before anything after it runs.
use super::*;
use crate::block::{ends_block, must_start_block, pc_bit, MAX_LEN};
use crate::decode::decode;
use crate::exec::max_ar;
use std::collections::HashMap;

// Compile-time emission probes: absent from production and profile builds.
#[cfg(feature = "wasm-jit-tests")]
pub(in crate::jit) static FORWARD_BRANCHES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
#[cfg(feature = "wasm-jit-tests")]
pub(in crate::jit) static SELF_LOOP_BRANCHES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
#[cfg(feature = "wasm-jit-tests")]
pub(in crate::jit) static JX_EDGES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub(super) const MAX_CHUNKS: usize = 64;
pub(super) const MAX_INSNS: usize = 512;
pub(in crate::jit) const MAX_PAGES: usize = 8;

/// One straight-line piece of a region, decoded independently of the block cache.
pub(in crate::jit) struct Chunk {
    pub pc: u32,
    pub instructions: Vec<BlockInsn>,
    /// coverage-s2: the target of a final `l32r aN; jx aN`, read at formation.
    pub jx: Option<u32>,
}

pub(in crate::jit) struct Formed {
    pub chunks: Vec<Chunk>,
    /// Hardware loops set up inside the region or active when it formed, as (LEND, LBEG):
    /// their backedges are internal edges, and an entry with exactly this loop active is admitted.
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
    /// EX181 s2: the current chunk's code is wrapped in a WASM loop at `chunk_depth - 1`
    pub self_loop: bool,
    /// last retired PC for each exit site, indexed by the tag in the result
    pub sites: Vec<ExitSite>,
    /// version-page index range covering every chunk (stores inside it set DIRTY)
    pub page_lo: u32,
    pub page_hi: u32,
    /// LEND -> LBEG for the region's own hardware loops
    pub loops: HashMap<u32, u32>,
    /// the current chunk's predicted JX target
    pub jx: Option<u32>,
    /// tails-s1: dispatch index of each chunk's guarded copy, when it has one; `None` while
    /// the region still counts its credit-short exits (they return CODE_SHORT).
    pub copies: Option<Vec<Option<u32>>>,
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
            | Bany | Bnone | Ball | Bnall
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
        Jx => chunk.jx.into_iter().collect(),
        op if terminal(op) => vec![],
        Loopnez | Loopgtz => vec![next, last.insn.imm as u32],
        // EX181 s3: fallthrough first favors placing the straight-line successor at
        // `current + 1`, where its edge needs no branch.
        op if conditional(op) => vec![next, last.insn.imm as u32],
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
        if i.len == 0 || !(supported_insn(&i, fast) || terminal(i.op)) { break }
        if pc != head && (must_start_block(&i) || cpu.boundary_bloom & pc_bit(pc) != 0) { break }
        v.push(BlockInsn { insn: i, max_ar: max_ar(&i), straddle: cpu.price_control && crate::exec::static_target(&i).is_some_and(|t| crate::exec::straddles(bus, t)), off: v.len() as u32 });
        // Includes the head: an internal backedge to it would skip a probe there.
        *bloom |= pc_bit(pc);
        pc = pc.wrapping_add(i.len as u32);
        if ends_block(&i) || terminal(i.op) { break }
    }
    (!v.is_empty()).then_some(v)
}

/// coverage-s2: `l32r aN, lit; jx aN` (the ROM `__call_*` trampolines) goes where the literal
/// says. The JX compares its register with this value, so a changed literal only exits. Priced
/// runs keep the exit, like every other dynamic transfer.
fn jx_target<B: Bus>(cpu: &Cpu, bus: &mut B, v: &[BlockInsn]) -> Option<u32> {
    let [.., l, j] = v else { return None };
    if cpu.price_control || j.insn.op != crate::Op::Jx || l.insn.op != crate::Op::L32r || l.insn.t != j.insn.s { return None }
    bus.fetch(l.insn.imm as u32).ok().map(u32::from_le_bytes)
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
            let jx = chunks[k].jx.take();
            let mut tail = chunks[k].instructions.split_off(at);
            // The loop exit is usually a chunk head already (the LOOPNEZ skip target).
            if !chunks.iter().any(|c| c.pc == pc) {
                for (n, bi) in tail.iter_mut().enumerate() { bi.off = n as u32; }
                chunks.push(Chunk { pc, instructions: tail, jx });
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
    let jx = jx_target(cpu, bus, &first);
    let mut chunks = vec![Chunk { pc: head, instructions: first, jx }];
    let mut seen: HashMap<u32, ()> = HashMap::from([(head, ())]);
    let mut q = 0;
    while q < chunks.len() && chunks.len() < MAX_CHUNKS && total < MAX_INSNS {
        for t in successors(&chunks[q]) {
            if seen.contains_key(&t) || chunks.len() == MAX_CHUNKS || total >= MAX_INSNS { continue }
            if let Some(c) = chunk(cpu, bus, head, t, fast, MAX_INSNS - total, &mut bloom) {
                total += c.len();
                seen.insert(t, ());
                let jx = jx_target(cpu, bus, &c);
                chunks.push(Chunk { pc: t, instructions: c, jx });
            }
        }
        q += 1;
    }
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
    // coverage-s1: the loop active now was set up before this region (its body got hot first);
    // its backedge is internal too when a chunk instruction ends at its LEND.
    let ends_at = |a: u32| chunks.iter().any(|c| {
        c.instructions.iter().try_fold(c.pc, |pc, bi| {
            let end = pc.wrapping_add(bi.insn.len as u32);
            if end == a { None } else { Some(end) }
        }).is_none()
    });
    if cpu.lcount != 0 && !loops.iter().any(|l| l.0 == cpu.lend) && ends_at(cpu.lend) {
        loops.push((cpu.lend, cpu.lbeg));
    }
    // A loop is only usable when its body and end are region chunks; two loops sharing
    // an end would make the backedge target ambiguous.
    loops.sort_unstable();
    loops.dedup();
    if loops.windows(2).any(|w| w[0].0 == w[1].0) { return None }
    split_at_loop_ends(&mut chunks, &loops);
    if chunks.len() > MAX_CHUNKS + loops.len() { return None }
    // coverage-s1: after the split, so an adopted loop can make a one-chunk body a region.
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
    // EX110: watch every page before reading the versions the region will compare against.
    for (i, _) in &pages { bus.note_code_page(*i); }
    let pv = bus.page_versions();
    for (i, v) in &mut pages { *v = pv.get(*i as usize).copied().unwrap_or(0); }
    Some(Formed { chunks, loops, bloom, lo, hi, pages })
}

/// Retire the current instruction and continue at `target`: inside the region when it is
/// a chunk head, the credit covers that chunk and no code page was written; else exit.
/// `direct` is the final edge of a chunk's code: only that one may fall into the
/// next chunk's code without a branch.
pub(super) fn region_edge(g: &mut Gen, target: u32, direct: bool) {
    // EX178: the next chunk may also be reached from the dispatch table, so ACCX must be
    // in memory on every edge, not only on the ones that leave.
    g.accx_flush();
    g.flush();
    let r = g.region.as_ref().unwrap();
    let (current, loop_depth, chunk_depth) = (r.current, r.loop_depth, r.chunk_depth);
    // Depth just inside every dispatch block, where the br_table sits.
    let (blocks, self_loop) = (loop_depth + r.heads.len() + r.copies.iter().flatten().flatten().count(), r.self_loop);
    match r.heads.get(&target).copied() {
        Some((index, len)) => {
            let (copy, short) = match &r.copies { Some(c) => (c[index], CODE_LEFT), None => (None, CODE_SHORT) };
            g.get(DONE);
            g.c(len);
            g.op(0x6a);
            g.get(3);
            g.op(0x4b);
            g.get(DIRTY);
            g.op(0x72);
            g.begin_if();
            // tails-s1 (EX182 s1 form): credit short of the whole chunk but not spent and no
            // helper ran: retire the quantum's tail in the target's guarded copy.
            if let Some(label) = copy {
                g.get(DIRTY);
                g.op(0x45);
                g.get(3);
                g.get(DONE);
                g.op(0x4b);
                g.op(0x71);
                g.begin_if();
                g.get(3);
                g.get(DONE);
                g.op(0x6b);
                g.set(STOP);
                g.c(0); // tails-s2: enter the copy at its head, not at this call's resume index
                g.set(4);
                g.c(label);
                g.set(NEXT);
                let back = g.depth() - loop_depth;
                g.op(0x0c);
                uleb(&mut g.bytes, back);
                g.end();
            }
            g.spill();
            g.cpu_const(PC, target);
            #[cfg(feature = "wasm-jit-profile")]
            {
                // Only the diagnostic module distinguishes these runtime causes.
                // If both hold, classify DIRTY as the reason execution must leave.
                let saved = g.last_kind;
                g.get(DIRTY);
                g.begin_if();
                g.last_kind = ExitKind::Dirty;
                g.ret_value(CODE_LEFT);
                g.end();
                g.last_kind = ExitKind::Budget;
                g.ret_value(short);
                g.resume_at(index as u32);
                g.last_kind = saved;
            }
            #[cfg(not(feature = "wasm-jit-profile"))]
            { g.ret_value(short); g.resume_at(index as u32); }
            g.end();
            // A guarded copy is neither followed by the next chunk nor inside the open blocks
            // of later chunks: every edge out of it re-dispatches.
            if g.guarded {
                g.c(index as u32);
                g.set(NEXT);
                let label = g.depth() - loop_depth;
                g.op(0x0c);
                uleb(&mut g.bytes, label);
            } else if !direct || index != current + 1 || g.depth() != chunk_depth {
                // EX181: chunk `index` starts after the end of the block at ctl index
                // `blocks - 1 - index`, which is still open for any forward target, and s2
                // wraps a self-looping chunk in its own loop: both are a plain `br`, only a
                // backward edge to another chunk re-dispatches through the br_table.
                let label = if index > current {
                    #[cfg(feature = "wasm-jit-tests")]
                    FORWARD_BRANCHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    g.depth() + index - blocks
                } else if index == current && self_loop {
                    #[cfg(feature = "wasm-jit-tests")]
                    SELF_LOOP_BRANCHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    g.depth() - chunk_depth
                } else {
                    g.c(index as u32);
                    g.set(NEXT);
                    g.depth() - loop_depth
                };
                g.op(0x0c);
                uleb(&mut g.bytes, label);
            }
        }
        None => {
            g.spill();
            g.cpu_const(PC, target);
            #[cfg(feature = "wasm-jit-profile")]
            let saved = std::mem::replace(&mut g.last_kind, ExitKind::Edge);
            g.ret_value(CODE_LEFT);
            #[cfg(feature = "wasm-jit-profile")]
            { g.last_kind = saved; }
        }
    }
}

/// tails-s1: dispatch index of each chunk's guarded copy for the chosen chunks: after the ordinary
/// chunks, in chunk order. A chunk of one instruction is never entered short of credit (r < 1
/// means r = 0) nor resumed at a later index, and gets none.
pub(in crate::jit) fn copy_indices(chunks: &[Chunk], copies: &[usize]) -> Vec<Option<u32>> {
    let mut n = chunks.len() as u32;
    chunks.iter().enumerate().map(|(k, c)| (c.instructions.len() > 1 && copies.contains(&k)).then(|| { n += 1; n - 1 })).collect()
}

/// `copies`: the chunks given a guarded copy (tails-s1), or `None` for a fresh region that
/// reports its credit-short exits as CODE_SHORT so the dispatcher can choose them.
pub(in crate::jit) fn generate(chunks: &[Chunk], pages: &[(u32, u32)], formed_loops: &[(u32, u32)], fast: bool, copies: Option<&[usize]>) -> (Vec<u8>, Vec<ExitSite>) {
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
    let cp = all().fold(0, |mask, bi| mask | policy::required_coprocessors(bi.insn.op));
    let heads: HashMap<_, _> = chunks.iter().enumerate().map(|(i, c)| (c.pc, (i, c.instructions.len() as u32))).collect();
    // Forward labels use the head count; the dispatch nesting uses the chunk count.
    assert_eq!(heads.len(), chunks.len(), "region chunk heads must be unique");
    let loops = formed_loops.iter().copied().collect();
    let copies = copies.map(|c| copy_indices(chunks, c));
    let ncopies = copies.iter().flatten().flatten().count();
    let n = chunks.len() + ncopies;
    let mut g = Gen {
        loaded: registers,
        written,
        max_ar,
        dynamic: true,
        region: Some(RegionGen { heads, current: 0, loop_depth: 0, chunk_depth: 0, self_loop: false, sites: Vec::new(), page_lo, page_hi, loops, jx: None, copies }),
        ..Gen::default()
    };
    // The caller has checked the credit for the entry chunk; window and coprocessor
    // state are proved here. Anything else takes a block module, which handles cuts.
    g.reload();
    if guard_max_ar >= 4 {
        g.window_collision(guard_max_ar);
        g.begin_if();
        g.c(CODE_REJECT << 16);
        g.op(0x0f);
        g.end();
    }
    if entry_head && max_ar >= 4 {
        // Entering past the ENTRY skips its post-rotation proof: prove the union now.
        g.get(4);
        g.c(0);
        g.op(0x47);
        g.window_collision(max_ar);
        g.c(0);
        g.op(0x47);
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
    if ncopies > 0 {
        // tails-s2 (EX182 s2): the entry holds the dispatch index in its low half and, for a
        // resume into a guarded copy, the instruction index in its high half. DONE counts from
        // minus that index, so DONE plus a body's static index is the retired count, and STOP
        // cuts the copy where the credit runs out. Both are zero for an ordinary chunk entry.
        g.c(0xffff);
        g.op(0x71);
        g.get(4);
        g.c(16);
        g.op(0x76);
        g.tee(4);
        g.get(3);
        g.op(0x6a);
        g.set(STOP);
        g.c(0);
        g.get(4);
        g.op(0x6b);
        g.set(DONE);
    }
    g.set(NEXT);
    g.begin_loop();
    let loop_depth = g.depth();
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
        // EX181 s2: a chunk that branches back to its own head (a conditional or J to it, or
        // a hardware loop body that is exactly this chunk) keeps that edge inside the
        // function as a WASM loop; every other arrival still comes through the br_table.
        let self_loop = successors(chunk).contains(&chunk.pc)
            || formed_loops.iter().any(|&(lend, lbeg)| lbeg == chunk.pc && lend == chunk_end(chunk));
        if let Some(run) = formed_loops.contains(&(chunk_end(chunk), chunk.pc)).then(|| memory::store_run(&chunk.instructions, fast)).flatten() {
            // store-s1: once per arrival, before the per-iteration loop; the credit admitted it.
            g.get(3);
            g.get(DONE);
            g.op(0x6b);
            memory::store_bulk(&mut g, &run, chunk.pc, chunk_end(chunk));
        }
        if self_loop {
            g.begin_loop();
        }
        {
            let r = g.region.as_mut().unwrap();
            r.current = k;
            r.loop_depth = loop_depth;
            r.chunk_depth = g.ctl.len();
            r.self_loop = self_loop;
            r.jx = chunk.jx;
        }
        emit_body(&mut g, chunk.pc, &chunk.instructions, fast, false, true, cp);
        if self_loop {
            g.end();
        }
    }
    // tails-s1: the guarded copies (EX156's form). Entered with STOP = the remaining credit, a
    // copy retires the quantum's tail and cuts between instructions; reaching the chunk end takes
    // the ordinary edges. Every cut shares one spill-and-return (EX182 s3).
    let copied: Vec<usize> = g.region.as_ref().unwrap().copies.iter().flatten().enumerate().filter_map(|(k, c)| c.map(|_| k)).collect();
    for k in copied {
        let chunk = &chunks[k];
        g.end();
        g.pending = 0;
        g.begin_block();
        g.cut_target = Some(g.depth());
        let len = chunk.instructions.len();
        for _ in 0..len {
            g.begin_block();
        }
        // tails-s2: a credit-short edge enters at index 0, a resume at its entry index.
        g.get(4);
        g.op(0x0e);
        uleb(&mut g.bytes, len - 1);
        for i in 0..len {
            uleb(&mut g.bytes, i);
        }
        {
            let r = g.region.as_mut().unwrap();
            r.current = k;
            r.loop_depth = loop_depth;
            r.chunk_depth = g.ctl.len();
            r.self_loop = false;
            r.jx = chunk.jx;
        }
        g.guarded = true;
        emit_body(&mut g, chunk.pc, &chunk.instructions, fast, false, true, cp);
        g.guarded = false;
        g.cut_target = None;
        g.op(0x00); // the body always leaves or branches; only a cut reaches the epilogue
        g.end();
        g.spill();
        g.get(TMP);
        g.op(0x0f);
    }
    g.op(0x00); // every chunk leaves or branches; no fallthrough out of the last one
    g.end(); // dispatch loop
    g.op(0x00);
    g.end(); // function body
    let sites = g.region.take().unwrap().sites;
    (finish(g, chunks[0].pc), sites)
}
