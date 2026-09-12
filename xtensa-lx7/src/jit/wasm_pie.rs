//! ESP32-S3 PIE (coprocessor 3) instructions on WebAssembly SIMD. Q registers stay in
//! CPU memory as 128-bit values; the hot vector loop of the TinyDraw tile kernels needs
//! only aligned 128-bit loads and stores with post-increment, lane compares, bitwise
//! logic and 32-bit lane insertion. Inference dot-product kernels add the signed 8- and
//! 16-bit multiply-accumulate into ACCX, with and without its load, and the ACCX reset.
//! Everything else keeps its interpreter path.
use super::*;
use crate::pie::{extract, Cmp, Kind, LdKind, Mode, Ops, PieInsn, Role, OPS};

const ACCX: usize = offset_of!(Cpu, accx);

const QR: usize = offset_of!(Cpu, qr);
/// CPENABLE bit for PIE.
pub(super) const CP3: u32 = 1 << 3;

fn table(i: &crate::Insn) -> (&'static PieInsn, Ops) {
    let p = &OPS[i.imm as usize];
    (p, extract(i.raw, p))
}

pub(super) fn supported(i: &crate::Insn, fast: bool) -> bool {
    if i.op != crate::Op::Pie {
        return false;
    }
    match OPS[i.imm as usize].kind {
        Kind::Andq | Kind::Orq | Kind::Xorq | Kind::Notq | Kind::MoviQ | Kind::ZeroQ => true,
        Kind::Vcmp { w, .. } => matches!(w, 8 | 16 | 32),
        Kind::Vld128(Mode::Ip) | Kind::Vst128(Mode::Ip) => fast,
        Kind::ZeroAccx => true,
        Kind::Vmulas { signed: true, w: 8 | 16, accx: true, ld: LdKind::None, qup: false } => true,
        Kind::Vmulas { signed: true, w: 8 | 16, accx: true, ld: LdKind::Ip, qup: false } => fast,
        _ => false,
    }
}

/// The CP3-disabled check can be proved once for a body whose PIE instructions are all
/// emitted; a helper in between could disable the coprocessor.
pub(super) fn can_hoist(instructions: &[BlockInsn], fast: bool) -> bool {
    instructions.iter().any(|bi| bi.insn.op == crate::Op::Pie)
        && instructions.iter().enumerate().all(|(n, bi)| {
            supported_insn(&bi.insn, fast) || (n + 1 == instructions.len() && terminal_helper(bi.insn.op))
        })
}

pub(super) fn guard(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool) {
    g.cpu(offset_of!(Cpu, cpenable));
    g.c(CP3);
    g.op(0x71);
    g.op(0x45);
    g.begin_if();
    g.fallback(bi, pc, next, last, false);
    g.end();
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
        guard(g, bi, pc, next, last);
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
        Kind::Vld128(Mode::Ip) => vmem(g, bi, pc, next, last, &o, false, None),
        Kind::Vst128(Mode::Ip) => vmem(g, bi, pc, next, last, &o, true, None),
        Kind::ZeroAccx => {
            g.cpu_const(ACCX, 0);
            g.cpu_const(ACCX + 4, 0);
        }
        Kind::Vmulas { w, ld: LdKind::None, .. } => accumulate(g, w, o.get(Role::Qx), o.get(Role::Qy)),
        Kind::Vmulas { w, ld: LdKind::Ip, .. } => {
            let (x, y) = (o.get(Role::Qx), o.get(Role::Qy));
            vmem(g, bi, pc, next, last, &o, false, Some((w, x, y)));
        }
        _ => unreachable!("PIE instruction was checked before emission"),
    }
}

fn i64_const(g: &mut Gen, mut n: i64) {
    g.op(0x42);
    loop {
        let b = (n as u8) & 127;
        n >>= 7;
        let done = (n == 0 && b & 64 == 0) || (n == -1 && b & 64 != 0);
        g.bytes.push(b | if done { 0 } else { 128 });
        if done {
            break;
        }
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

/// ACCX += Σ x·y over the signed `w`-bit lanes of Q registers `x` and `y`, saturated to 40
/// bits: exactly `pie::exec_packed`. The products are widening vector multiplies over the low
/// and high halves; the lane sums and ACCX itself are 64-bit, so no intermediate can wrap.
fn accumulate(g: &mut Gen, w: u8, x: i32, y: i32) {
    i64_const(g, 0);
    if w == 8 {
        // i8·i8 fits i16; two products per i32 lane after the pairwise add.
        for high in [false, true] {
            q(g, x);
            q(g, y);
            g.bytes.extend([0xfd, if high { 0x9d } else { 0x9c }, 0x01]); // i16x8.extmul_{low,high}_i8x16_s
            g.bytes.extend([0xfd, 0x7e]); // i32x4.extadd_pairwise_i16x8_s
            g.set(V128);
            add_i32_lanes(g);
        }
    } else {
        // i16·i16 fits i32; eight products, summed in 64 bits.
        for high in [false, true] {
            q(g, x);
            q(g, y);
            g.bytes.extend([0xfd, if high { 0xbd } else { 0xbc }, 0x01]); // i32x4.extmul_{low,high}_i16x8_s
            g.set(V128);
            add_i32_lanes(g);
        }
    }
    // ACCX, sign-extended from 40 bits: accx[0] | (accx[1] & 0xff) << 32.
    g.get(0);
    g.op(0x35); // i64.load32_u
    uleb(&mut g.bytes, 2);
    uleb(&mut g.bytes, ACCX);
    g.get(0);
    g.op(0x35);
    uleb(&mut g.bytes, 2);
    uleb(&mut g.bytes, ACCX + 4);
    i64_const(g, 0xff);
    g.op(0x83); // i64.and
    i64_const(g, 32);
    g.op(0x86); // i64.shl
    g.op(0x84); // i64.or
    i64_const(g, 24);
    g.op(0x86);
    i64_const(g, 24);
    g.op(0x87); // i64.shr_s
    g.op(0x7c); // i64.add
    // Saturate: min(v, 2^39-1), then max(v, -2^39). `select` keeps its first operand when
    // the condition holds.
    for (bound, cmp) in [((1i64 << 39) - 1, 0x55u8), (-(1i64 << 39), 0x53u8)] { // i64.gt_s, i64.lt_s
        g.set(WIDE);
        i64_const(g, bound);
        g.get(WIDE);
        g.get(WIDE);
        i64_const(g, bound);
        g.op(cmp);
        g.op(0x1b); // select
    }
    g.set(WIDE);
    // Store it back as accx_set does: the low 32 bits, then bits 32..40.
    g.get(0);
    g.get(WIDE);
    g.op(0x3e); // i64.store32
    uleb(&mut g.bytes, 2);
    uleb(&mut g.bytes, ACCX);
    g.get(0);
    g.get(WIDE);
    i64_const(g, 32);
    g.op(0x87);
    i64_const(g, 0xff);
    g.op(0x83);
    g.op(0x3e);
    uleb(&mut g.bytes, 2);
    uleb(&mut g.bytes, ACCX + 4);
}

/// Aligned 128-bit load or store through the fast mapping, then the post-increment.
/// Mirrors `emit_memory`: nothing is written before every check has passed, and a
/// miss re-executes the whole instruction in the interpreter. `accumulate_first` is the
/// multiply-accumulate of an `ee.vmulas.*.accx.ld.ip`, emitted once the load is known to
/// succeed and before it overwrites Q register Qu, which may be one of its operands.
#[allow(clippy::too_many_arguments)]
fn vmem(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool, o: &Ops, store: bool, accumulate_first: Option<(u8, i32, i32)>) {
    let a = o.get(Role::As) as u8;
    let imm = o.get(Role::Imm) as u32;
    // The hardware ignores the low address bits.
    g.ar(a);
    g.c(!15u32);
    g.op(0x71);
    g.set(ADDR);
    g.begin_block();
    g.begin_block();
    g.get(5);
    g.op(0x45);
    g.bytes.extend([0x0d, 0]);
    g.get(5);
    g.get(ADDR);
    g.c(16);
    g.op(0x76);
    g.get(ADDR);
    g.c(24);
    g.op(0x76);
    g.op(0x73);
    g.c(511);
    g.op(0x71);
    g.c(size_of::<TlbEntry>() as u32);
    g.op(0x6c);
    g.op(0x6a);
    g.set(TLB);
    g.get(ADDR);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, lo));
    g.op(0x49);
    g.bytes.extend([0x0d, 0]);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, hi));
    g.get(ADDR);
    g.op(0x6b);
    g.c(16);
    g.op(0x49);
    g.bytes.extend([0x0d, 0]);
    g.get(ADDR);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, hi));
    g.op(0x4f);
    g.bytes.extend([0x0d, 0]);
    if store {
        g.get(TLB);
        g.load(offset_of!(TlbEntry, writable));
        g.op(0x45);
        g.bytes.extend([0x0d, 0]);
    }
    g.get(ADDR);
    g.get(TLB);
    g.load(offset_of!(TlbEntry, lo));
    g.op(0x6b);
    g.set(REL);
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
        g.get(6);
        g.get(TLB);
        g.load(offset_of!(TlbEntry, vbase));
        g.get(REL);
        g.c(8);
        g.op(0x76);
        g.op(0x6a);
        g.c(2);
        g.op(0x74);
        g.op(0x6a);
        g.tee(TMP);
        g.get(TMP);
        g.load(0);
        g.c(4);
        g.op(0x6a);
        g.store(0);
        region_store_check(g);
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
