//! Shared scalar and PIE memory probes, version tracking and optional cache pricing.
use super::*;
use emu_core::bus::{PREV_PAGE_BYTES, TLB_ENTRIES, TLB_INDEX_SHIFT, TLB_XOR_SHIFT, VPAGE_SHIFT};

const VPAGE_MASK: u32 = (1 << VPAGE_SHIFT) - 1;

/// log2 of one entry's size: scaling the hash into a byte offset folds into its shifts.
const ENTRY_SHIFT: u32 = size_of::<TlbEntry>().ilog2();

const _: () = {
    assert!(TLB_ENTRIES.is_power_of_two());
    assert!(TLB_INDEX_SHIFT < 32 && TLB_XOR_SHIFT < 32);
    // Aligned 16-byte PIE accesses must fit one version page.
    assert!(VPAGE_SHIFT >= 4 && VPAGE_SHIFT < 32);
    // The previous-page rule must not reach past one page.
    assert!(PREV_PAGE_BYTES < 1 << VPAGE_SHIFT);
    // EX173 s2: fold `index * size_of::<TlbEntry>()` into the hash's two shifts and its mask.
    assert!(size_of::<TlbEntry>().is_power_of_two());
    assert!(TLB_INDEX_SHIFT >= ENTRY_SHIFT && TLB_XOR_SHIFT >= ENTRY_SHIFT);
    assert!((TLB_ENTRIES as u64 - 1) << ENTRY_SHIFT < i32::MAX as u64);
};

pub(super) fn emit(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool) {
    use crate::Op::*;
    let i = &bi.insn;
    let store = matches!(i.op, S8i | S16i | S32i | S32iN | Ssi);
    let width = match i.op {
        L8ui | S8i => 1,
        L16ui | L16si | S16i => 2,
        _ => 4,
    };
    if i.op == L32r {
        g.c(i.imm as u32);
    } else {
        g.ar(i.s);
        g.c(i.imm as u32);
        g.op(0x6a);
    }
    g.set(ADDR);
    // This block jumps to the slow instruction before making any memory changes.
    g.begin_block();
    g.begin_block();
    // A byte access is aligned by construction; `width - 1 == 0` made this test a constant.
    if width > 1 {
        g.get(ADDR);
        g.c(width - 1);
        g.op(0x71);
        g.bytes.extend([0x0d, 0]);
    }
    probe(g, width, store);
    #[cfg(feature = "wasm-cache-inline")]
    emit_cache_hit(g, store, 1);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, base));
    g.get(REL);
    g.op(0x6a);
    if store {
        if i.op == Ssi { g.fr(i.t); } else { g.ar(i.t); }
        g.op(match width {
            1 => 0x3a,
            2 => 0x3b,
            _ => 0x36,
        });
        g.bytes.extend([0, 0]);
        record_store(g, 1);
    } else {
        if i.op == Lsi { g.set(TMP); g.get(0); g.get(TMP); }
        g.op(match i.op {
            L8ui => 0x2d,
            L16ui => 0x2f,
            L16si => 0x2e,
            _ => 0x28,
        });
        g.bytes.extend([0, 0]);
        if i.op == Lsi { g.store(offset_of!(Cpu, fr) + 4 * i.t as usize); } else { g.set_ar(i.t); }
    }
    g.bytes.extend([0x0c, 1]);
    g.end();
    g.fallback(bi, pc, next, last, false);
    g.end();
}

/// The ordinary TLB checks already established a successful aligned access.
/// Preserve the reference cache's round-robin policy: hit does not
/// update replacement; miss leaves before touching state and uses the helper.
#[cfg(feature = "wasm-cache-inline")]
pub(super) fn emit_cache_hit(g: &mut Gen, store: bool, accesses: u8) {
    use emu_core::bus::{FastCache, FastCacheLine};
    if !super::CACHE_PROBES.load(std::sync::atomic::Ordering::Relaxed) { return; }
    g.begin_block(); // No cache view or internal memory: keep ordinary fast path.
    g.get(2);
    g.load(offset_of!(Helpers, cache));
    g.tee(CACHE);
    g.op(0x45);
    g.bytes.extend([0x0d, 0]);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, src));
    g.c(!1);
    g.op(0x71);
    g.c(2); // Flash=2, PSRAM=3 in the experimental S3 adapter.
    g.op(0x47);
    g.bytes.extend([0x0d, 0]);

    g.get(TLB);
    g.load(offset_of!(TlbEntry, src));
    g.c(28);
    g.op(0x74);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, off));
    g.get(REL);
    g.op(0x6a);
    g.op(0x72);
    g.c(6);
    g.op(0x76);
    g.set(CACHE_TAG);
    g.get(CACHE);
    g.load(offset_of!(FastCache, lines));
    g.get(CACHE_TAG);
    g.c(super::CACHE_SET_MASK.load(std::sync::atomic::Ordering::Relaxed));   // 64-byte lines, 8 ways: 64 sets at 32 KB, 128 at 64 KB
    g.op(0x71);
    g.c((8 * size_of::<FastCacheLine>()) as u32);
    g.op(0x6c);
    g.op(0x6a);
    g.set(CACHE_SET);
    g.begin_block(); // Find a way. Invalid tags are MAX, impossible for 64B keys.
    for way in 0..8 {
        g.get(CACHE_SET);
        g.c((way * size_of::<FastCacheLine>()) as u32);
        g.op(0x6a);
        g.tee(CACHE_LINE);
        g.load(offset_of!(FastCacheLine, tag));
        g.get(CACHE_TAG);
        g.op(0x46);
        g.begin_if();
        g.bytes.extend([0x0c, 1]);
        g.end();
    }
    g.bytes.extend([0x0c, 2]); // No match: leave to this instruction's slow path.
    g.end();
    if store {
        g.get(CACHE_LINE);
        g.c(1);
        g.store(offset_of!(FastCacheLine, dirty));
    }
    g.get(CACHE);
    g.load(offset_of!(FastCache, hits));
    g.tee(CACHE_SET);
    g.get(CACHE_SET);
    g.bytes.extend([0x29, 3, 0]); // i64.load
    g.bytes.extend([0x42, accesses, 0x7c]); // i64.const accesses; i64.add
    g.bytes.extend([0x37, 3, 0]); // i64.store
    g.end();
}

/// After a fast store bumped the version at the pointer in TMP: a store into one of the
/// region's own code pages means the next chunk head must leave, so the dispatcher
/// re-validates before stale translated code runs.
fn region_store_check(g: &mut Gen) {
    if let Some(r) = &g.region {
        let (lo, hi) = (r.page_lo, r.page_hi);
        g.get(TMP);
        g.get(6);
        g.op(0x6b);
        g.c(lo * 4);
        g.op(0x6b);
        g.c((hi - lo) * 4);
        g.op(0x4d);
        g.get(DIRTY);
        g.op(0x72);
        g.set(DIRTY);
    }
}
/// Probe ADDR for `width` bytes after alignment has been established. On failure,
/// branch to the enclosing slow-path block before any guest state is changed.
/// On success TLB names the entry and REL is its byte offset.
pub(super) fn probe(g: &mut Gen, width: u32, store: bool) {
    // Entry address: (((ADDR >> INDEX) ^ (ADDR >> XOR)) & (ENTRIES - 1)) * size, with the
    // scaling folded into both shifts and the mask (EX173 s2), so the multiply disappears.
    g.get(5);
    g.get(ADDR);
    g.c(TLB_INDEX_SHIFT - ENTRY_SHIFT);
    g.op(0x76);
    g.get(ADDR);
    g.c(TLB_XOR_SHIFT - ENTRY_SHIFT);
    g.op(0x76);
    g.op(0x73);
    g.c((TLB_ENTRIES as u32 - 1) << ENTRY_SHIFT);
    g.op(0x71);
    g.op(0x6a);
    g.set(TLB);
    // EX173 s2: for scalar/vector widths, one unsigned compare decides the whole access. REL = ADDR - lo is the offset
    // within the entry and the access is inside it exactly when REL + width - 1 < span:
    //  - ADDR below lo wraps REL to at least 2^32 - lo, and span <= 2^32 - lo, so it fails;
    //  - an empty slot has span 0, which no offset can beat;
    //  - REL + width - 1 cannot itself wrap, because alignment is already established and
    //    every mapping starts 16-byte aligned, so REL is a multiple of width and at most
    //    2^32 - width.
    g.get(ADDR);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, lo));
    g.op(0x6b);
    g.tee(REL);
    if width > 16 {
        // Coalesced runs are only 16-byte aligned, not aligned to their full width.
        // Reject an offset outside the entry first, then compare the remaining length.
        // Neither subtraction can wrap and no endpoint addition is needed.
        g.get(TLB);
        g.load(offset_of!(TlbEntry, span));
        g.op(0x4f); // i32.ge_u
        g.bytes.extend([0x0d, 0]);
        g.get(TLB);
        g.load(offset_of!(TlbEntry, span));
        g.get(REL);
        g.op(0x6b); // i32.sub
        g.c(width);
        g.op(0x49); // i32.lt_u
        g.bytes.extend([0x0d, 0]);
    } else {
        if width > 1 {
            g.c(width - 1);
            g.op(0x6a);
        }
        g.get(TLB);
        g.load(offset_of!(TlbEntry, span));
        g.op(0x4f);
        g.bytes.extend([0x0d, 0]);
    }
    if store {
        g.get(TLB);
        load16(g, offset_of!(TlbEntry, writable));
        g.op(0x45);
        g.bytes.extend([0x0d, 0]);
    }
}

/// x4 pack: `writable` and `code` are u16 fields.
fn load16(g: &mut Gen, offset: usize) {
    g.op(0x2f); // i32.load16_u
    uleb(&mut g.bytes, 1);
    uleb(&mut g.bytes, offset);
}


/// Match the interpreter's number of word writes, including version increments.
///
/// EX110: a mapping whose `code` is zero has no decoded consumer for any page a version bump
/// could reach (`TlbEntry.code`, `Bus::note_code_page`), so the whole bump and the region's own
/// code-page test go. The interpreter's `write*_access` skips the same bump behind the same flag,
/// so both paths still produce identical version counters. A page that gains code later is
/// watched before its first decode reads bytes and version, so earlier skipped bumps are
/// invisible: that decode already sees the written bytes.
///
/// EX180's previous-page bump lives inside the same gate, and that is sound: a consumer that
/// depends on bytes in the written page `p` records page `p` as well (block entries record the
/// page of their last byte, `block.rs`; regions record both ends of every instruction,
/// `wasm_region.rs`; the decode cache records `pc + 3`, `exec.rs`), and `watch_code_page` marks
/// the 64 KiB blocks containing the watched page and its neighboring pages, so watching `p - 1` alone
/// already forces `code != 0` on any mapping covering `p`.
/// Clobbers TMP; it may finish pointing at the preceding version page.
pub(super) fn record_store(g: &mut Gen, writes: u32) {
    g.get(TLB);
    load16(g, offset_of!(TlbEntry, code));
    g.begin_if();
    record_bump(g, writes);
    g.end();
}

fn record_bump(g: &mut Gen, writes: u32) {
    g.get(6);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, vbase));
    g.get(REL);
    g.c(VPAGE_SHIFT);
    g.op(0x76);
    g.op(0x6a);
    g.c(2);
    g.op(0x74);
    g.op(0x6a);
    g.tee(TMP);
    g.get(TMP);
    g.load(0);
    g.c(writes);
    g.op(0x6a);
    g.store(0);
    region_store_check(g);
    // A write into the first bytes of a page also changes any instruction that began in
    // the previous one, so the bus bumps that page once whatever the access width. Only
    // the first of the interpreter's word writes can qualify, so this adds one, not
    // `writes`. The `p > 0` guard is the pointer still being inside the version array.
    g.get(REL);
    g.c(VPAGE_MASK);
    g.op(0x71);
    g.c(PREV_PAGE_BYTES);
    g.op(0x49);
    g.begin_if();
    g.get(TMP);
    g.get(6);
    g.op(0x4b);
    g.begin_if();
    g.get(TMP);
    g.c(4);
    g.op(0x6b);
    g.tee(TMP);
    g.get(TMP);
    g.load(0);
    g.c(1);
    g.op(0x6a);
    g.store(0);
    region_store_check(g);
    g.end();
    g.end();
}

/// store-s1: bulk runs taken at store-run loop heads and the iterations they retired.
#[cfg(feature = "wasm-jit-profile")]
pub(in crate::jit) static STORE_RUNS: [std::sync::atomic::AtomicU64; 2] = [const { std::sync::atomic::AtomicU64::new(0) }; 2];

/// store-s1: a hardware-loop body that stores one loop-invariant register over exactly the
/// `stride` bytes above a pointer and then steps the pointer by `stride` (the render span
/// fill, the ROM memset). Iterations of it write one repeating pattern.
pub(super) struct StoreRun { p: u8, v: u8, width: u32, stride: u32, len: u32 }

pub(super) fn store_run(body: &[BlockInsn], fast: bool) -> Option<StoreRun> {
    use crate::Op::*;
    use std::sync::atomic::Ordering::Relaxed;
    // Per-access pricing, cache probes and fetch records are per instruction, not per run.
    if !fast || super::PRICED.load(Relaxed) || super::CACHE_PROBES.load(Relaxed) || super::FETCH_RING.load(Relaxed) { return None }
    let ([stores @ .., step], true) = (body, body.len() >= 2) else { return None };
    let p = step.insn.s;
    let (dest, stride) = (if step.insn.op == AddiN { step.insn.r } else { step.insn.t }, step.insn.imm as u32);
    // Bound the stride first: `off < stride` then keeps every shift below in range (review B1).
    if !matches!(step.insn.op, Addi | AddiN) || dest != p || stride == 0 || stride > 16 { return None }
    let first = &stores[0].insn;
    let width = match first.op { S8i => 1, S16i => 2, S32i | S32iN => 4, _ => return None };
    let mut covered = 0u32;
    for bi in stores {
        let i = &bi.insn;
        let w = match i.op { S8i => 1, S16i => 2, S32i | S32iN => 4, _ => 0 };
        let off = i.imm as u32;
        if w != width || i.s != p || i.t != first.t || !off.is_multiple_of(width) || off >= stride || covered & (1 << (off / width)) != 0 { return None }
        covered |= 1 << (off / width);
    }
    // Every unit of the stride written once; the pattern is filled eight bytes at a time.
    (first.t != p && stride == width * stores.len() as u32 && (8u32.is_multiple_of(stride) || stride.is_multiple_of(8)))
        .then_some(StoreRun { p, v: first.t, width, stride, len: body.len() as u32 })
}

/// store-s1: at the head of `run`'s loop, with the remaining credit on the stack, retire as many
/// whole iterations with a taken backedge as LCOUNT and credit allow (leaving one iteration of
/// credit, so the ordinary body that follows still fits) in one proved range, when the loop is
/// this one, the pointer is aligned and the range lies in one writable mapping. Otherwise
/// nothing changes and the ordinary body runs.
pub(super) fn store_bulk(g: &mut Gen, run: &StoreRun, lbeg: u32, lend: u32) {
    g.c(run.len);
    g.op(0x6e); // i32.div_u: iterations the credit covers
    g.set(TMP);
    g.begin_block();
    g.cpu(LEND);
    g.c(lend);
    g.op(0x47);
    g.cpu(LBEG);
    g.c(lbeg);
    g.op(0x47);
    g.op(0x72);
    g.ar(run.p);
    g.c(run.width - 1);
    g.op(0x71);
    g.op(0x72);
    g.bytes.extend([0x0d, 0]);
    // k = min(LCOUNT + 1, credit / len) - 1 >= 1; LCOUNT + 1 wraps to 0 for a 2^32 count.
    g.cpu(LCOUNT);
    g.c(1);
    g.op(0x6a);
    g.tee(REL);
    g.get(TMP);
    g.get(REL);
    g.get(TMP);
    g.op(0x49);
    g.op(0x1b); // select: the smaller
    g.tee(TMP);
    g.c(2);
    g.op(0x49);
    g.bytes.extend([0x0d, 0]);
    g.get(TMP);
    g.c(1);
    g.op(0x6b);
    g.set(TMP);
    g.ar(run.p);
    g.set(ADDR);
    probe_range(g, run);
    // A mapping with decoded consumers moves the versions the stores would have moved.
    g.get(TLB);
    load16(g, offset_of!(TlbEntry, code));
    g.begin_if();
    bump_run(g, run);
    g.end();
    #[cfg(feature = "wasm-jit-tests")]
    g.test_hit(&super::super::tests::STORE_RUN_TAKEN);
    #[cfg(feature = "wasm-jit-profile")]
    {
        for (n, by) in [(0usize, None), (1, Some(()))] {
            g.c(STORE_RUNS[n].as_ptr() as u32);
            g.c(STORE_RUNS[n].as_ptr() as u32);
            g.bytes.extend([0x29, 3, 0]); // i64.load
            if by.is_some() { g.get(TMP); g.op(0xad); } else { g.c64(1); } // i64.extend_i32_u
            g.op(0x7c);
            g.bytes.extend([0x37, 3, 0]); // i64.store
        }
    }
    // The pattern: the stored unit repeated over eight bytes, written one stride at a time.
    g.ar(run.v);
    g.op(0xad);
    g.c64(match run.width { 1 => 0xff, 2 => 0xffff, _ => 0xffff_ffff });
    g.op(0x83);
    g.c64(match run.width { 1 => 0x0101_0101_0101_0101, 2 => 0x0001_0001_0001_0001, _ => 0x0000_0001_0000_0001 });
    g.op(0x7e); // i64.mul
    g.set(WIDE);
    // REL: host end; ADDR: host cursor.
    g.get(TLB);
    g.load(offset_of!(TlbEntry, base));
    g.get(REL);
    g.op(0x6a);
    g.tee(ADDR);
    bytes(g, run);
    g.op(0x6a);
    g.set(REL);
    g.begin_loop();
    for j in 0..run.stride.div_ceil(8) {
        g.get(ADDR);
        g.get(WIDE);
        // i64.store8/16/32 or i64.store, unaligned, at the j-th eight bytes.
        g.op(match run.stride { 1 => 0x3c, 2 => 0x3d, 4 => 0x3e, _ => 0x37 });
        g.op(0);
        uleb(&mut g.bytes, 8 * j as usize);
    }
    g.get(ADDR);
    g.c(run.stride);
    g.op(0x6a);
    g.tee(ADDR);
    g.get(REL);
    g.op(0x49);
    g.bytes.extend([0x0d, 0]);
    g.end();
    g.ar(run.p);
    bytes(g, run);
    g.op(0x6a);
    g.set_ar(run.p);
    g.get(0);
    g.cpu(LCOUNT);
    g.get(TMP);
    g.op(0x6b);
    g.store(LCOUNT);
    g.get(DONE);
    g.get(TMP);
    g.c(run.len);
    g.op(0x6c);
    g.op(0x6a);
    g.set(DONE);
    g.end();
}

/// The run's byte count, TMP iterations of `stride`.
fn bytes(g: &mut Gen, run: &StoreRun) {
    g.get(TMP);
    g.c(run.stride);
    g.op(0x6c);
}

/// store-s1: ADDR up to ADDR + TMP * stride in one writable entry; on success TLB names it
/// and REL is the start's offset in it.
fn probe_range(g: &mut Gen, run: &StoreRun) {
    g.get(5);
    g.get(ADDR);
    g.c(TLB_INDEX_SHIFT - ENTRY_SHIFT);
    g.op(0x76);
    g.get(ADDR);
    g.c(TLB_XOR_SHIFT - ENTRY_SHIFT);
    g.op(0x76);
    g.op(0x73);
    g.c((TLB_ENTRIES as u32 - 1) << ENTRY_SHIFT);
    g.op(0x71);
    g.op(0x6a);
    g.set(TLB);
    // As the coalesced probe: the offset inside the entry, then the bytes remaining above it.
    g.get(ADDR);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, lo));
    g.op(0x6b);
    g.tee(REL);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, span));
    g.op(0x4f);
    g.bytes.extend([0x0d, 0]);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, span));
    g.get(REL);
    g.op(0x6b);
    bytes(g, run);
    g.op(0x49);
    g.bytes.extend([0x0d, 0]);
    g.get(TLB);
    load16(g, offset_of!(TlbEntry, writable));
    g.op(0x45);
    g.bytes.extend([0x0d, 0]);
}

/// Push the version pointer of the page holding entry offset `local` (as `record_bump`).
fn version_ptr(g: &mut Gen, local: u8) {
    g.get(6);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, vbase));
    g.get(local);
    g.c(VPAGE_SHIFT);
    g.op(0x76);
    g.op(0x6a);
    g.c(2);
    g.op(0x74);
    g.op(0x6a);
}

/// store-s1: inside the `if` of a watched mapping, one level in from the skip block. Move each
/// page's version by the stores that land in it and the previous page's by those in its first
/// PREV_PAGE_BYTES, exactly as `record_bump` per store. A range whose pages (or the page before)
/// hold the code running here skips instead: the region's pages, or the looping block's two.
fn bump_run(g: &mut Gen, run: &StoreRun) {
    let shift = run.width.ilog2();
    // ADDR: end offset; HOSTP and NEXT: first and last page version pointers.
    g.get(REL);
    bytes(g, run);
    g.op(0x6a);
    g.tee(ADDR);
    g.c(1);
    g.op(0x6b);
    g.set(NEXT);
    version_ptr(g, NEXT);
    g.set(NEXT);
    version_ptr(g, REL);
    g.set(HOSTP);
    if let Some(r) = &g.region {
        // The region's version pages [lo, hi] meet [first - 1, last].
        let (lo, hi) = (r.page_lo, r.page_hi);
        g.get(6);
        g.c(hi * 4 + 4);
        g.op(0x6a);
        g.get(HOSTP);
        g.op(0x4f);
        g.get(NEXT);
        g.get(6);
        g.c(lo * 4);
        g.op(0x6a);
        g.op(0x4f);
        g.op(0x71);
    } else {
        // Either of the looping block's two version pointers in [first - 1, last].
        for n in 0..2 {
            g.get(2);
            g.load(offset_of!(Helpers, version_ptrs) + n * 4);
            g.get(HOSTP);
            g.c(4);
            g.op(0x6b);
            g.op(0x4f);
            g.get(2);
            g.load(offset_of!(Helpers, version_ptrs) + n * 4);
            g.get(NEXT);
            g.op(0x4d);
            g.op(0x71);
            if n != 0 { g.op(0x72); }
        }
    }
    g.bytes.extend([0x0d, 1]);
    // HOSTP: segment start; NEXT: segment end (the next page start, or the end).
    g.get(REL);
    g.set(HOSTP);
    g.begin_loop();
    g.get(HOSTP);
    g.c(VPAGE_MASK);
    g.op(0x72);
    g.c(1);
    g.op(0x6a);
    g.tee(NEXT);
    g.get(ADDR);
    g.get(NEXT);
    g.get(ADDR);
    g.op(0x49);
    g.op(0x1b);
    g.set(NEXT);
    version_ptr(g, HOSTP);
    version_ptr(g, HOSTP);
    g.load(0);
    g.get(NEXT);
    g.get(HOSTP);
    g.op(0x6b);
    g.c(shift);
    g.op(0x76);
    g.op(0x6a);
    g.store(0);
    // Stores in the page's first bytes also move the previous page, if there is one.
    g.get(HOSTP);
    g.c(VPAGE_MASK);
    g.op(0x71);
    g.c(PREV_PAGE_BYTES);
    g.op(0x49);
    version_ptr(g, HOSTP);
    g.get(6);
    g.op(0x4b);
    g.op(0x71);
    g.begin_if();
    for _ in 0..2 {
        version_ptr(g, HOSTP);
        g.c(4);
        g.op(0x6b);
    }
    g.load(0);
    // (min(end, page start + PREV_PAGE_BYTES) - start + width - 1) >> log2 width
    for _ in 0..2 {
        g.get(HOSTP);
        g.c(!VPAGE_MASK);
        g.op(0x71);
        g.c(PREV_PAGE_BYTES);
        g.op(0x6a);
        g.get(NEXT);
    }
    g.op(0x49);
    g.op(0x1b);
    g.get(HOSTP);
    g.op(0x6b);
    g.c(run.width - 1);
    g.op(0x6a);
    g.c(shift);
    g.op(0x76);
    g.op(0x6a);
    g.store(0);
    g.end();
    g.get(NEXT);
    g.tee(HOSTP);
    g.get(ADDR);
    g.op(0x49);
    g.bytes.extend([0x0d, 0]);
    g.end();
}
