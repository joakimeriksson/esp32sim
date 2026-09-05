//! Scalar FP and boolean instruction emission. FR values stay in CPU memory so
//! the exact FMA helper and existing exception/memory helpers see committed bits.
use super::*;

pub(super) fn supported(op: crate::Op) -> bool {
    use crate::Op::*;
    matches!(
        op,
        AddS | SubS
            | MulS
            | MaddS
            | MsubS
            | MovS
            | AbsS
            | NegS
            | Rfr
            | Wfr
            | ConstS
            | FloatS
            | UfloatS
            | RoundS
            | TruncS
            | FloorS
            | CeilS
            | UtruncS
            | UnS
            | OeqS
            | UeqS
            | OltS
            | UltS
            | OleS
            | UleS
            | MoveqzS
            | MovnezS
            | MovltzS
            | MovgezS
            | MovfS
            | MovtS
            | MaddnS
            | DivnS
            | Div0S
            | Nexp01S
            | Recip0S
            | Rsqrt0S
            | Sqrt0S
            | AddexpS
            | MkdadjS
            | MksadjS
            | AddexpmS
            | Movf
            | Movt
            | Bf
            | Bt
    )
}

// CPENABLE cannot change within an entirely supported block. A final call/return
// helper may change machine state, but exits immediately. Do not extend this proof
// across arbitrary interpreter helpers: they may write CPENABLE before later FP.
pub(super) fn can_hoist_guard(block: &Block) -> bool {
    block
        .instructions
        .iter()
        .any(|bi| requires_coprocessor(bi.insn.op))
        && block.instructions.iter().enumerate().all(|(n, bi)| {
            super::supported(bi.insn.op, block.fast)
                || (n + 1 == block.instructions.len() && terminal_helper(bi.insn.op))
        })
}

fn requires_coprocessor(op: crate::Op) -> bool {
    use crate::Op::*;
    (supported(op) && !matches!(op, Movf | Movt | Bf | Bt)) || matches!(op, Lsi | Ssi)
}

pub(super) fn guard(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool) {
    // Keep the check at the instruction boundary: prefixes and budget cuts must
    // complete before a disabled-coprocessor exception is delivered.
    g.cpu(offset_of!(Cpu, cpenable));
    g.c(1);
    g.op(0x71);
    g.op(0x45);
    g.begin_if();
    g.fallback(bi, pc, next, last, false);
    g.end();
}

pub(super) fn emit(g: &mut Gen, bi: &BlockInsn, pc: u32, next: u32, last: bool, cp_enabled: bool) {
    use crate::Op::*;
    let i = &bi.insn;
    let (r, s, t) = (i.r, i.s, i.t);
    let imm = i.imm as u32;
    if !cp_enabled && requires_coprocessor(i.op) {
        guard(g, bi, pc, next, last);
    }
    match i.op {
        MaddnS | DivnS | Div0S | Nexp01S | Recip0S | Rsqrt0S | Sqrt0S | AddexpS => {}
        AddS | SubS | MulS | MkdadjS | MksadjS => {
            g.get(0);
            g.float(s);
            if i.op != MksadjS {
                g.float(if i.op == MkdadjS { r } else { t });
            }
            g.op(match i.op {
                AddS => 0x92,
                SubS => 0x93,
                MulS => 0x94,
                MkdadjS => 0x95,
                _ => 0x91,
            });
            g.op(0xbc);
            g.store(offset_of!(Cpu, fr) + 4 * r as usize);
        }
        MaddS | MsubS => {
            g.get(0);
            g.fr(s);
            g.fr(t);
            g.fr(r);
            g.c((i.op == MsubS) as u32);
            g.helper(offset_of!(Helpers, fused), 1);
            g.store(offset_of!(Cpu, fr) + 4 * r as usize);
        }
        FloatS | UfloatS => {
            g.get(0);
            g.ar(s);
            g.op(if i.op == FloatS { 0xb2 } else { 0xb3 });
            g.c(((1u64 << imm) as f32).to_bits());
            g.op(0xbe);
            g.op(0x95);
            g.op(0xbc);
            g.store(offset_of!(Cpu, fr) + 4 * r as usize);
        }
        RoundS | TruncS | FloorS | CeilS | UtruncS => {
            g.float(s);
            g.c(((1u64 << imm) as f32).to_bits());
            g.op(0xbe);
            g.op(0x94);
            g.op(match i.op {
                RoundS => 0x90,
                FloorS => 0x8e,
                CeilS => 0x8d,
                _ => 0x8f,
            });
            g.op(0xbc);
            g.set(TMP);
            g.get(TMP);
            g.op(0xbe);
            g.0.extend([0xfc, if i.op == UtruncS { 1 } else { 0 }]);
            g.c(if i.op == UtruncS {
                u32::MAX
            } else {
                0x8000_0000
            });
            g.get(TMP);
            g.op(0xbe);
            g.get(TMP);
            g.op(0xbe);
            g.op(0x5b); // Ordered equality rejects NaN; Xtensa uses a nonzero NaN sentinel.
            g.op(0x1b);
            g.set_ar(r);
        }
        MovS | AddexpmS | AbsS | NegS | Rfr | Wfr | ConstS => {
            if i.op != Rfr {
                g.get(0);
            }
            match i.op {
                Wfr => g.ar(s),
                ConstS => g.c(match imm {
                    1 => 1f32,
                    2 => 2f32,
                    3 => 0.5f32,
                    _ => 0f32,
                }
                .to_bits()),
                _ => g.fr(s),
            }
            if matches!(i.op, AbsS | NegS) {
                g.c(if i.op == AbsS {
                    0x7fff_ffff
                } else {
                    0x8000_0000
                });
                g.op(if i.op == AbsS { 0x71 } else { 0x73 });
            }
            if i.op == Rfr {
                g.set_ar(r);
            } else {
                g.store(offset_of!(Cpu, fr) + 4 * r as usize);
            }
        }
        UnS | OeqS | UeqS | OltS | UltS | OleS | UleS => {
            g.get(0);
            g.cpu(offset_of!(Cpu, br));
            g.c(!(1 << r));
            g.op(0x71);
            if i.op != UnS {
                g.float(s);
                g.float(t);
                g.op(match i.op {
                    OeqS | UeqS => 0x5b,
                    OltS | UltS => 0x5d,
                    _ => 0x5f,
                });
            }
            if matches!(i.op, UnS | UeqS | UltS | UleS) {
                g.float(s);
                g.float(s);
                g.op(0x5c);
                g.float(t);
                g.float(t);
                g.op(0x5c);
                g.op(0x72);
                if i.op != UnS {
                    g.op(0x72);
                }
            }
            g.c(r as u32);
            g.op(0x74);
            g.op(0x72);
            g.store(offset_of!(Cpu, br));
        }
        Movf | Movt | MovfS | MovtS | MoveqzS | MovnezS | MovltzS | MovgezS => {
            if matches!(i.op, Movf | Movt | MovfS | MovtS) {
                g.boolean(t);
                if matches!(i.op, Movf | MovfS) {
                    g.op(0x45);
                }
            } else {
                g.ar(t);
                g.c(0);
                g.op(match i.op {
                    MoveqzS => 0x46,
                    MovnezS => 0x47,
                    MovltzS => 0x48,
                    _ => 0x4e,
                });
            }
            g.begin_if();
            if matches!(i.op, Movf | Movt) {
                g.ar(s);
                g.set_ar(r);
            } else {
                g.get(0);
                g.fr(s);
                g.store(offset_of!(Cpu, fr) + 4 * r as usize);
            }
            g.end();
        }
        Bf | Bt => {
            g.boolean(s);
            if i.op == Bf {
                g.op(0x45);
            }
            g.begin_if();
            g.advance();
            g.cpu_const(PC, imm);
            g.ret(CODE_LEFT);
            g.end();
        }
        _ => unreachable!("scalar instruction was checked before emission"),
    }
}
