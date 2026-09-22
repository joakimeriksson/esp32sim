//! ESP32-S3 PIE (coprocessor 3) instructions on WebAssembly SIMD. Q registers stay in
//! CPU memory as 128-bit values; the hot vector loop of the TinyDraw tile kernels needs
//! only aligned 128-bit loads and stores with post-increment, lane compares, bitwise
//! logic and 32-bit lane insertion. Inference dot-product kernels add the signed 8- and
//! 16-bit multiply-accumulate into ACCX, with and without its load, and the ACCX reset.
//! Everything else keeps its interpreter path.
use super::*;
use crate::pie::{extract, Cmp, Kind, LdKind, Mode, Ops, PieInsn, Role, OPS};

const QR: usize = offset_of!(Cpu, qr);
/// CPENABLE bit for PIE.
pub(super) const CP3: u32 = 1 << 3;

fn table(i: &crate::Insn) -> (&'static PieInsn, Ops) {
    let p = &OPS[i.imm as usize];
    (p, extract(i.raw, p))
}

/// EX178: the largest magnitude one `ee.vmulas.s*.accx` can add, `lanes * 2^(2w-2)`.
fn max_dot(w: u8) -> i64 {
    debug_assert!(matches!(w, 8 | 16));
    (128 / w as i64) * (1i64 << (2 * w as u32 - 2))
}

/// EX178: instructions that may run while ACCX is held in a local. `ee.zero.accx` starts
/// the run and the accumulates extend it; the two `.ip` vector moves are admitted because
/// they never read the accumulator and their own miss path spills it. Everything else,
/// `rur.accx_0` above all, forces the memory copy first.
pub(super) fn accx_local_safe(i: &crate::Insn, fast: bool) -> bool {
    i.op == crate::Op::Pie
        && policy::pie(i, fast)
        && matches!(
            OPS[i.imm as usize].kind,
            Kind::ZeroAccx
                | Kind::Vld128(Mode::Ip)
                | Kind::Vst128(Mode::Ip)
                | Kind::Vmulas { accx: true, .. }
        )
}

fn v128_load(g: &mut Gen, offset: usize) {
    g.bytes.extend([0xfd, 0x00]);
    uleb(&mut g.bytes, 0);
    uleb(&mut g.bytes, offset);
}
fn v128_store(g: &mut Gen, offset: usize) {
    g.bytes.extend([0xfd, 0x0b]);
    uleb(&mut g.bytes, 0);
    uleb(&mut g.bytes, offset);
}
/// Push Q register `n`.
fn q(g: &mut Gen, n: i32) {
    g.get(0);
    v128_load(g, QR + 16 * (n as usize & 7));
}
/// Store the v128 on the stack into Q register `n`; the CPU pointer must be below it.
fn set_q(g: &mut Gen, n: i32) {
    v128_store(g, QR + 16 * (n as usize & 7));
}

pub(super) fn emit(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool, cp_enabled: bool) {
    let (p, o) = table(&bi.insn);
    if !cp_enabled {
        g.guard_coprocessor(CP3, bi, pc, next, last);
    }
    match p.kind {
        Kind::Andq | Kind::Orq | Kind::Xorq => {
            g.get(0);
            q(g, o.get(Role::Qx));
            q(g, o.get(Role::Qy));
            g.bytes.extend([0xfd, match p.kind { Kind::Andq => 0x4e, Kind::Orq => 0x50, _ => 0x51 }]);
            set_q(g, o.get(Role::Qa));
        }
        Kind::Notq => {
            g.get(0);
            q(g, o.get(Role::Qx));
            g.bytes.extend([0xfd, 0x4d]);
            set_q(g, o.get(Role::Qa));
        }
        Kind::ZeroQ => {
            g.get(0);
            g.bytes.extend([0xfd, 0x0c]);
            g.bytes.extend([0; 16]);
            set_q(g, o.get(Role::Qa));
        }
        Kind::MoviQ => {
            g.get(0);
            q(g, o.get(Role::Qu));
            g.ar(o.get(Role::As) as u8);
            g.bytes.extend([0xfd, 0x1c, (o.get(Role::Sel) & 3) as u8]);
            set_q(g, o.get(Role::Qu));
        }
        Kind::Vcmp { cmp, w } => {
            g.get(0);
            q(g, o.get(Role::Qx));
            q(g, o.get(Role::Qy));
            // i8x16 / i16x8 / i32x4: eq, then lt_s and gt_s (signed lanes, as the TRM defines)
            let base = match w { 8 => 0x23, 16 => 0x2d, _ => 0x37 };
            g.bytes.extend([0xfd, base + match cmp { Cmp::Eq => 0, Cmp::Lt => 2, Cmp::Gt => 4 }]);
            set_q(g, o.get(Role::Qa));
        }
        Kind::SrcQ { qup, ld: Mode::None } => {
            // Bytes k of the result are bytes k+n of Qs0:Qs1, n = SAR_BYTE & 15. A swizzle yields
            // zero for an index above 15, so each half selects only its own bytes.
            g.cpu(offset_of!(Cpu, sar_byte));
            g.c(15);
            g.op(0x71);
            g.bytes.extend([0xfd, 0x0f]); // i8x16.splat
            g.bytes.extend([0xfd, 0x0c]);
            g.bytes.extend(0u8..16);
            g.bytes.extend([0xfd, 0x6e]); // i8x16.add
            g.set(V128);
            q(g, o.get(Role::Qs0));
            g.get(V128);
            g.bytes.extend([0xfd, 0x0e]); // i8x16.swizzle
            q(g, o.get(Role::Qs1));
            g.get(V128);
            g.bytes.extend([0xfd, 0x0c]);
            g.bytes.extend([16u8; 16]);
            g.bytes.extend([0xfd, 0x71]); // i8x16.sub
            g.bytes.extend([0xfd, 0x0e]);
            g.bytes.extend([0xfd, 0x50]); // v128.or
            g.set(V128);
            if qup {
                // Qs0 takes the old Qs1 after Qa is written, in the interpreter's order.
                g.get(0);
                q(g, o.get(Role::Qs1));
            }
            g.get(0);
            g.get(V128);
            set_q(g, o.get(Role::Qa));
            if qup {
                set_q(g, o.get(Role::Qs0));
            }
        }
        Kind::Vsr32 | Kind::Vsl32 => {
            // Lane shift by SAR & 63; 32 and above clears. WASM takes the count modulo 32.
            g.get(0);
            q(g, o.get(Role::Qs));
            g.cpu(SAR);
            g.bytes.extend([0xfd, if p.kind == Kind::Vsr32 { 0xad } else { 0xab }, 0x01]); // i32x4.shr_u / shl
            g.c(0);
            g.cpu(SAR);
            g.c(0x20);
            g.op(0x71);
            g.op(0x45);
            g.op(0x6b);
            g.bytes.extend([0xfd, 0x11]); // i32x4.splat
            g.bytes.extend([0xfd, 0x4e]);
            set_q(g, o.get(Role::Qa));
        }
        Kind::Arith { op, w, .. } => {
            g.get(0);
            q(g, o.get(Role::Qx));
            q(g, o.get(Role::Qy));
            use crate::pie::ArithOp::*;
            let code: u32 = match (op, w) {
                (Adds, 8) => 0x6f, (Subs, 8) => 0x72, (Min, 8) => 0x76, (Max, 8) => 0x78,
                (Adds, 16) => 0x8f, (Subs, 16) => 0x92, (Min, 16) => 0x96, (Max, 16) => 0x98,
                (Min, 32) => 0xb6, (Max, 32) => 0xb8,
                _ => unreachable!("PIE arithmetic was checked before emission"),
            };
            g.bytes.push(0xfd);
            uleb(&mut g.bytes, code as usize);
            set_q(g, if o.has(Role::Qz) { o.get(Role::Qz) } else { o.get(Role::Qa) });
        }
        Kind::Vld128(Mode::Ip) => vmem(g, bi, pc, next, last, &o, false, None),
        Kind::Vst128(Mode::Ip) => vmem(g, bi, pc, next, last, &o, true, None),
        Kind::ZeroAccx => {
            if g.accx_ok {
                // EX178: start a run. The memory copy stays stale until `accx_spill`,
                // which every exit, join and foreign reader emits first.
                g.c64(0);
                g.set(ACC);
                g.accx_live = true;
                g.accx_head = 0;
            } else {
                g.cpu_const(ACCX, 0);
                g.cpu_const(ACCX + 4, 0);
            }
        }
        Kind::Vmulas { w, ld: LdKind::None, .. } => accumulate(g, w, o.get(Role::Qx), o.get(Role::Qy)),
        Kind::Vmulas { w, ld: LdKind::Ip, .. } => {
            let (x, y) = (o.get(Role::Qx), o.get(Role::Qy));
            vmem(g, bi, pc, next, last, &o, false, Some((w, x, y)));
        }
        _ => unreachable!("PIE instruction was checked before emission"),
    }
}

/// Push extract_lane of the i32x4 in V128 for lanes 0..4, each sign-extended and added to the
/// i64 on the stack.
fn add_i32_lanes(g: &mut Gen) {
    for lane in 0..4 {
        g.get(V128);
        g.bytes.extend([0xfd, 0x1b, lane]); // i32x4.extract_lane
        g.op(0xac); // i64.extend_i32_s
        g.op(0x7c); // i64.add
    }
}

/// Flush before opening a conditional memory path: both the fast operation and its
/// interpreter fallback must start from the same authoritative accumulator state.
fn prepare_accumulate(g: &mut Gen, w: u8) {
    if g.accx_live && g.accx_head > ((1i64 << 39) - 1) - max_dot(w) {
        g.accx_flush();
    }
}

/// ACCX += Σ x·y over the signed `w`-bit lanes of Q registers `x` and `y`, saturated to 40
/// bits: exactly `pie::exec_packed`. The products are widening vector multiplies over the low
/// and high halves. Signed-8 products sum in 32 bits (at most 16 * 128 * 128);
/// signed-16 products and ACCX need 64 bits.
fn accumulate(g: &mut Gen, w: u8, x: i32, y: i32) {
    // EX178: after the `ee.zero.accx` that dominates this run the accumulator is exactly
    // the sum of the products added since, whose magnitudes `accx_head` bounds. While that
    // bound stays inside [-2^39, 2^39-1] neither the 40-bit sign extension of the old value
    // nor the two saturating selects can change anything, so the sum can live in a local.
    prepare_accumulate(g, w);
    let step = max_dot(w);
    let held = g.accx_live;
    if held {
        g.accx_head += step;
    }
    if w == 8 {
        // i8·i8 fits i16; two products per i32 lane after the pairwise add, and the whole
        // dot product fits i32 (EX042 s8 reduction), so it is widened once.
        for high in [false, true] {
            q(g, x);
            q(g, y);
            g.bytes.extend([0xfd, if high { 0x9d } else { 0x9c }, 0x01]); // i16x8.extmul_{low,high}_i8x16_s
            g.bytes.extend([0xfd, 0x7e]); // i32x4.extadd_pairwise_i16x8_s
            if high {
                g.get(V128);
                g.bytes.extend([0xfd, 0xae, 0x01]); // i32x4.add
            }
            g.set(V128);
        }
        for lane in 0..4 {
            g.get(V128);
            g.bytes.extend([0xfd, 0x1b, lane]); // i32x4.extract_lane
            if lane != 0 { g.op(0x6a); } // i32.add
        }
        g.op(0xac); // i64.extend_i32_s
        if held {
            g.get(ACC);
            g.op(0x7c); // i64.add
        }
    } else {
        // i16·i16 fits i32; eight products, summed in 64 bits on top of the held value.
        if held { g.get(ACC); } else { g.c64(0); }
        for high in [false, true] {
            q(g, x);
            q(g, y);
            g.bytes.extend([0xfd, if high { 0xbd } else { 0xbc }, 0x01]); // i32x4.extmul_{low,high}_i16x8_s
            g.set(V128);
            add_i32_lanes(g);
        }
    }
    if held {
        g.set(ACC);
        return;
    }
    // The two ACCX words are contiguous. Sign-extend their low 40 bits, ignoring
    // the unused upper bits of accx[1].
    g.get(0);
    g.op(0x29); // i64.load
    uleb(&mut g.bytes, 2);
    uleb(&mut g.bytes, ACCX);
    g.c64(24);
    g.op(0x86);
    g.c64(24);
    g.op(0x87); // i64.shr_s
    g.op(0x7c); // i64.add
    // Saturate: min(v, 2^39-1), then max(v, -2^39). `select` keeps its first operand when
    // the condition holds.
    for (bound, cmp) in [((1i64 << 39) - 1, 0x55u8), (-(1i64 << 39), 0x53u8)] { // i64.gt_s, i64.lt_s
        g.set(WIDE);
        g.c64(bound);
        g.get(WIDE);
        g.get(WIDE);
        g.c64(bound);
        g.op(cmp);
        g.op(0x1b); // select
    }
    g.set(WIDE);
    // Store the canonical 40-bit value, leaving accx[1]'s unused bits zero.
    g.get(0);
    g.get(WIDE);
    g.c64((1i64 << 40) - 1);
    g.op(0x83);
    g.op(0x37); // i64.store
    uleb(&mut g.bytes, 2);
    uleb(&mut g.bytes, ACCX);
}

/// EX178 s1: a straight-line run of PIE post-increment vector loads through one base
/// register. Each address is `(ar[As] & !15) + offsets[k]`, because the hardware ignores
/// the low four address bits and every `.ip` immediate is a multiple of 16 (`pie_table.rs`
/// scales Role::Imm by 16), so one range probe over `span` bytes stands for all of them.
pub(super) struct Run {
    pub len: usize,
    /// Bytes from the first address to the end of the last access.
    span: u32,
    /// Sum of the post-increments, applied to the base register once at the end.
    total: u32,
    base: u8,
    offsets: Vec<u32>,
}

/// Longest coalescable run starting at `bis[0]`, if it is worth a shared probe.
pub(super) fn coalesce(bis: &[BlockInsn], fast: bool) -> Option<Run> {
    use std::sync::atomic::Ordering::Relaxed;
    // The inline data-cache probe and the fetch ring add per-access work that a shared
    // probe cannot carry; neither is on in the benchmarked configuration.
    if super::CACHE_PROBES.load(Relaxed) || super::FETCH_RING.load(Relaxed) {
        return None;
    }
    // A positive immediate keeps the offsets inside the probed range and increasing.
    let load = |bi: &BlockInsn| -> Option<(u8, u32)> {
        if bi.insn.op != crate::Op::Pie || !policy::pie(&bi.insn, fast) {
            return None;
        }
        let (p, o) = table(&bi.insn);
        let ip = matches!(p.kind, Kind::Vld128(Mode::Ip) | Kind::Vmulas { accx: true, ld: LdKind::Ip, .. });
        let imm = o.get(Role::Imm);
        (ip && imm > 0).then(|| ((o.get(Role::As) & 15) as u8, imm as u32))
    };
    let mut offsets: Vec<u32> = Vec::new();
    let mut total = 0u32;
    let mut base = None;
    for bi in bis {
        let Some((b, imm)) = load(bi) else { break };
        // One base register only: its post-increments are the sole writes to it, so the
        // addresses stay statically known for the whole run.
        if *base.get_or_insert(b) != b || total + 16 > 4096 {
            break;
        }
        offsets.push(total);
        total += imm;
    }
    (offsets.len() >= 2).then(|| Run {
        len: offsets.len(),
        span: offsets[offsets.len() - 1] + 16,
        total,
        base: base.unwrap(),
        offsets,
    })
}

/// Emit `run` twice: once behind a single range probe against one TLB entry, once exactly
/// as the per-access path would. Inside the coalesced copy no access can miss, fault or
/// reach a helper, so the intermediate values of the base register are unobservable and
/// the post-increments collapse into one addition. A probe that fails for any reason (the
/// run crosses an entry, the mapping is absent, the PIE price mode is on) falls into the
/// second copy, where faults are still raised by exactly the instruction that causes them.
pub(super) fn emit_run(g: &mut Gen, bis: &[BlockInsn], pc0: u32, extras: &[u8], run: &Run, block_end: bool) {
    let mut pcs = Vec::with_capacity(run.len + 1);
    let mut pc = pc0;
    for bi in bis {
        pcs.push(pc);
        pc = pc.wrapping_add(bi.insn.len as u32);
    }
    pcs.push(pc);
    let entry_pending = g.pending;
    let held = (g.accx_live, g.accx_head);
    g.begin_block(); // join
    g.begin_block(); // per-access copy
    g.ar(run.base);
    g.c(!15u32);
    g.op(0x71);
    g.set(ADDR);
    // Mode 1 prices PIE memory operations in the interpreter, exactly as `vmem` does.
    g.cpu(offset_of!(Cpu, approximate_pie_mode));
    g.c(1);
    g.op(0x46);
    g.bytes.extend([0x0d, 0]);
    // run_inner always supplies a live TLB or the all-empty sentinel.
    memory::probe(g, run.span, false);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, base));
    g.get(REL);
    g.op(0x6a);
    g.set(HOSTP);
    for (k, bi) in bis.iter().enumerate() {
        g.last_pc = pcs[k];
        g.straddle = bi.straddle;
        g.wait_price = extras[k] as u32;
        g.price(g.wait_price);
        let (p, o) = table(&bi.insn);
        // The accumulate reads Qx and Qy before the load overwrites Qu, which may alias.
        if let Kind::Vmulas { w, .. } = p.kind {
            accumulate(g, w, o.get(Role::Qx), o.get(Role::Qy));
        }
        g.get(0);
        g.get(HOSTP);
        v128_load(g, run.offsets[k] as usize);
        set_q(g, o.get(Role::Qu));
        g.advance();
    }
    g.ar(run.base);
    g.c(run.total);
    g.op(0x6a);
    g.set_ar(run.base);
    g.bytes.extend([0x0c, 1]);
    g.end();
    (g.accx_live, g.accx_head) = held;
    for (k, bi) in bis.iter().enumerate() {
        g.last_pc = pcs[k];
        g.straddle = bi.straddle;
        g.wait_price = extras[k] as u32;
        g.price(g.wait_price);
        emit(g, bi, pcs[k], pcs[k + 1], block_end && k + 1 == run.len, true);
        g.advance();
    }
    g.end();
    // Both copies retire the whole run; `end` restored the count at the join.
    g.pending = entry_pending + run.len as u32;
}

/// Aligned 128-bit load or store through the fast mapping, then the post-increment.
/// Mirrors `emit_memory`: nothing is written before every check has passed, and a
/// miss re-executes the whole instruction in the interpreter. `accumulate_first` is the
/// multiply-accumulate of an `ee.vmulas.*.accx.ld.ip`, emitted once the load is known to
/// succeed and before it overwrites Q register Qu, which may be one of its operands.
#[allow(clippy::too_many_arguments)]
fn vmem(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool, o: &Ops, store: bool, accumulate_first: Option<(u8, i32, i32)>) {
    if let Some((w, _, _)) = accumulate_first { prepare_accumulate(g, w); }
    let a = o.get(Role::As) as u8;
    let imm = o.get(Role::Imm) as u32;
    // The hardware ignores the low address bits.
    g.ar(a);
    g.c(!15u32);
    g.op(0x71);
    g.set(ADDR);
    g.begin_block();
    g.begin_block();
    // Mode 1 prices these memory operations in the existing interpreter helper.
    // Keep the experiment simple; the default and SRC.Q.LD-only mode stay fast.
    g.cpu(offset_of!(Cpu, approximate_pie_mode));
    g.c(1);
    g.op(0x46);
    g.bytes.extend([0x0d, 0]);
    memory::probe(g, 16, store);
    #[cfg(feature = "wasm-cache-inline")]
    memory::emit_cache_hit(g, store, 4); // The reference PIE helper performs four words.
    if store {
        g.get(TLB);
        g.load(offset_of!(TlbEntry, base));
        g.get(REL);
        g.op(0x6a);
        q(g, o.get(Role::Qv));
        v128_store(g, 0);
        // One version page: a 16-byte aligned access never crosses a 256-byte page. The
        // interpreter stores four words, bumping the version four times; match it exactly
        // so version arrays stay identical, not merely both changed.
        memory::record_store(g, 4);
    } else {
        if let Some((w, x, y)) = accumulate_first {
            accumulate(g, w, x, y);
        }
        g.get(0);
        g.get(TLB);
        g.load(offset_of!(TlbEntry, base));
        g.get(REL);
        g.op(0x6a);
        v128_load(g, 0);
        set_q(g, o.get(Role::Qu));
    }
    g.ar(a);
    g.c(imm);
    g.op(0x6a);
    g.set_ar(a);
    g.bytes.extend([0x0c, 1]);
    g.end();
    g.fallback(bi, pc, next, last, false);
    g.end();
}
