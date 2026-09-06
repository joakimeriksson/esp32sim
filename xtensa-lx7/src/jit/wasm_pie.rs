//! ESP32-S3 PIE (coprocessor 3) instructions on WebAssembly SIMD. Q registers stay in
//! CPU memory as 128-bit values; the hot vector loop of the TinyDraw tile kernels needs
//! only aligned 128-bit loads and stores with post-increment, lane compares, bitwise
//! logic and 32-bit lane insertion. Everything else keeps its interpreter path.
use super::*;
use crate::pie::{extract, Cmp, Kind, Mode, Ops, PieInsn, Role, OPS};

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
        Kind::Vld128(Mode::Ip) => vmem(g, bi, pc, next, last, &o, false),
        Kind::Vst128(Mode::Ip) => vmem(g, bi, pc, next, last, &o, true),
        _ => unreachable!("PIE instruction was checked before emission"),
    }
}

/// Aligned 128-bit load or store through the fast mapping, then the post-increment.
/// Mirrors `emit_memory`: nothing is written before every check has passed, and a
/// miss re-executes the whole instruction in the interpreter.
fn vmem(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool, o: &Ops, store: bool) {
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
