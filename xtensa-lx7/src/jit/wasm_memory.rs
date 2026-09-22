//! Shared scalar and PIE memory probes, version tracking and optional cache pricing.
use super::*;
use emu_core::bus::{TLB_ENTRIES, TLB_INDEX_SHIFT, TLB_XOR_SHIFT, VPAGE_SHIFT};

const VPAGE_MASK: u32 = (1 << VPAGE_SHIFT) - 1;
/// An instruction can begin up to three bytes before a page boundary and still keep bytes
/// in it, because PIE encodings are four bytes long (`pie::decode`). A write into the first
/// three bytes of a page therefore also changes instructions whose code page is the
/// previous one, and the bus bumps that page as well (`esp32s3/src/bus.rs` `bump` and
/// `note_written`, `esp32s3/src/bus/dma.rs` for the DMA run copy).
const PREV_PAGE_BYTES: u32 = 3;

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
    // EX173 s2: one unsigned compare decides the whole access. REL = ADDR - lo is the offset
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
    if width > 1 {
        g.c(width - 1);
        g.op(0x6a);
    }
    g.get(TLB);
    g.load(offset_of!(TlbEntry, span));
    g.op(0x4f);
    g.bytes.extend([0x0d, 0]);
    if store {
        g.get(TLB);
        load16(g, offset_of!(TlbEntry, writable));
        g.op(0x45);
        g.bytes.extend([0x0d, 0]);
    }
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
/// x4 pack: `writable` and `code` are u16 fields.
fn load16(g: &mut Gen, offset: usize) {
    g.op(0x2f); // i32.load16_u
    uleb(&mut g.bytes, 1);
    uleb(&mut g.bytes, offset);
}

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
