//! Admission and proof policy for the WASM backend. Opcode families stay explicit;
//! decoded PIE kinds and special-register operands retain their own legality checks.
use super::*;
use crate::pie::{ArithOp, Kind, LdKind, Mode, OPS};

/// `supported` for a decoded instruction: PIE eligibility depends on the table entry, and RUR
/// is emitted for ACCX_0/ACCX_1, which inference kernels read after every dot product.
pub(in crate::jit) fn supported_insn(i: &crate::Insn, fast: bool) -> bool {
    supported_opcode(i.op, fast) || pie(i, fast) || (i.op == crate::Op::Rur && matches!(i.imm, 0 | 1 | 13))
        // EX155: WUR SAR_BYTE precedes every ee.src.q of the 4-bit unpack kernels.
        || (i.op == crate::Op::Wur && i.imm == 13)
        || (i.op == crate::Op::Rsr && rsr_field(i.imm as u32).is_some())
}

/// EX135: special registers whose `Cpu` field is exact at any instruction of a dispatch and that
/// `read_sr` returns unchanged. Not CCOUNT/INTERRUPT/ICOUNT (advance at dispatch ends), not the
/// loop, shift and window registers generated code may hold in locals.
pub(in crate::jit) fn rsr_field(n: u32) -> Option<usize> {
    use crate::state::sr;
    Some(match n {
        sr::PS => offset_of!(Cpu, ps), sr::PRID => offset_of!(Cpu, prid), sr::SCOMPARE1 => offset_of!(Cpu, scompare1),
        sr::INTENABLE => offset_of!(Cpu, intenable), sr::VECBASE => offset_of!(Cpu, vecbase), sr::CPENABLE => offset_of!(Cpu, cpenable),
        sr::EXCCAUSE => offset_of!(Cpu, exccause), sr::EXCVADDR => offset_of!(Cpu, excvaddr), sr::DEPC => offset_of!(Cpu, depc),
        177..=183 => offset_of!(Cpu, epc) + 4 * (n - 176) as usize,
        194..=199 => offset_of!(Cpu, eps) + 4 * (n - 192) as usize,
        209..=215 => offset_of!(Cpu, excsave) + 4 * (n - 208) as usize,
        244..=247 => offset_of!(Cpu, misc) + 4 * (n - 244) as usize,
        _ => return None,
    })
}

/// A compiled body consists of inline instructions and an optional terminal helper.
/// Calls are emitted directly but still end a body: their window state may change.
pub(in crate::jit) fn admitted(instructions: &[BlockInsn], fast: bool) -> bool {
    instructions.iter().enumerate().all(|(n, bi)| {
        let last = n + 1 == instructions.len();
        (!terminal_helper(bi.insn.op) || last)
            && (supported_insn(&bi.insn, fast) || (last && terminal_helper(bi.insn.op)))
    })
}

/// A helper in the middle could change CPENABLE. Use the same body proof as admission
/// before hoisting either coprocessor guard; terminal helpers leave immediately.
pub(in crate::jit) fn coprocessors(instructions: &[BlockInsn], fast: bool) -> u32 {
    if !admitted(instructions, fast) { return 0; }
    instructions.iter().fold(0, |mask, bi| mask | required_coprocessors(bi.insn.op))
}

pub(in crate::jit) fn required_coprocessors(op: crate::Op) -> u32 {
    (requires_coprocessor(op) as u32) | if op == crate::Op::Pie { pie::CP3 } else { 0 }
}

// This opcode-only predicate deliberately excludes operand-sensitive RSR/RUR/PIE.
// Call supported_insn for admission of a decoded instruction.
// Most unsupported operations keep their block interpreted. Calls/returns at the
// end may use a helper after the compiled prefix; memory misses also use helpers.
pub(in crate::jit) fn supported_opcode(op: crate::Op, fast: bool) -> bool {
    use crate::Op::*;
    matches!(
        op,
        Nop | NopN
            | Rsync | Esync | Dsync
            | Memw
            | Extw
            | Movi
            | MoviN
            | Mov
            | MovN
            | Add
            | AddN
            | Sub
            | And
            | Or
            | Xor
            | Mull
            | Muluh
            | Mulsh
            | Quou
            | Quos
            | Remu
            | Rems
            | Salt
            | Saltu
            | Addi
            | AddiN
            | Addmi
            | Addx2
            | Addx4
            | Addx8
            | Subx2
            | Subx4
            | Subx8
            | Neg
            | Abs
            | Slli
            | Srli
            | Srai
            | Sll
            | Srl
            | Sra
            | Src
            | Entry
            | Extui
            | Sext
            | Ssr
            | Ssl
            | Ssa8l
            | Ssa8b
            | Ssai
            | Nsau
            | Moveqz
            | Movnez
            | Movltz
            | Movgez
            | Min
            | Max
            | Minu
            | Maxu
            | J
            | Jx
            | Call0 | Call4 | Call8 | Call12 | Callx0 | Callx4 | Callx8 | Callx12
            | Beqz
            | BeqzN
            | Bnez
            | BnezN
            | Bltz
            | Bgez
            | Beqi
            | Bnei
            | Blti
            | Bgei
            | Bltui
            | Bgeui
            | Beq
            | Bne
            | Blt
            | Bge
            | Bltu
            | Bgeu
            | Bbci
            | Bbsi
            | Bbc
            | Bbs
            | Loop | Loopnez | Loopgtz
    ) || floating_point(op) || (fast
        && matches!(
            op,
            L8ui | L16ui | L16si | L32i | L32iN | L32r | S8i | S16i | S32i | S32iN | Lsi | Ssi
        ))
}

// Initially admit only straight-line integer/memory loops. Slow memory paths leave
// generated execution; no helper can change mappings or interrupt state and continue.
pub(in crate::jit) fn loop_safe(op: crate::Op, fast: bool) -> bool {
    use crate::Op::*;
    matches!(op, Nop | NopN | Movi | MoviN | Mov | MovN | Add | AddN | Sub
        | And | Or | Xor | Addi | AddiN | Addmi | Addx2 | Addx4 | Addx8
        | Subx2 | Subx4 | Subx8 | Neg | Slli | Srli | Srai | Extui | Sext)
        || (fast && matches!(op, L8ui | L16ui | L16si | L32i | L32iN | L32r
            | S8i | S16i | S32i | S32iN))
}

// Calls and returns must end decoder blocks. Normal calls are emitted directly;
// returns and exceptional calls retain exec_insn's window and exception handling.
pub(in crate::jit) fn terminal_helper(op: crate::Op) -> bool {
    use crate::Op::*;
    matches!(op, Call0 | Call4 | Call8 | Call12 | Callx0 | Callx4 | Callx8 | Callx12
        | Ret | RetN | Retw | RetwN
        // EX135: these end a block too, and run as well through the helper after a compiled prefix.
        | Wsr | Xsr | Rsil)
}

pub(in crate::jit) fn floating_point(op: crate::Op) -> bool {
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

pub(in crate::jit) fn requires_coprocessor(op: crate::Op) -> bool {
    use crate::Op::*;
    (floating_point(op) && !matches!(op, Movf | Movt | Bf | Bt)) || matches!(op, Lsi | Ssi)
}

pub(in crate::jit) fn pie(i: &crate::Insn, fast: bool) -> bool {
    if i.op != crate::Op::Pie {
        return false;
    }
    match OPS[i.imm as usize].kind {
        Kind::Andq | Kind::Orq | Kind::Xorq | Kind::Notq | Kind::MoviQ | Kind::ZeroQ => true,
        Kind::Vcmp { w, .. } => matches!(w, 8 | 16 | 32),
        Kind::Vld128(Mode::Ip) | Kind::Vst128(Mode::Ip) => fast,
        Kind::ZeroAccx => true,
        // EX155: the 4-bit weight unpack (byte shift across two Q registers, lane shift, saturating
        // subtract of the zero point); none of these touch memory or an optional PIE price.
        Kind::SrcQ { ld: Mode::None, .. } | Kind::Vsr32 | Kind::Vsl32 => true,
        Kind::Arith { op: ArithOp::Adds | ArithOp::Subs, w: 8 | 16, ld: false, st: false } => true,
        Kind::Arith { op: ArithOp::Max | ArithOp::Min, w: 8 | 16 | 32, ld: false, st: false } => true,
        Kind::Vmulas { signed: true, w: 8 | 16, accx: true, ld: LdKind::None, qup: false } => true,
        Kind::Vmulas { signed: true, w: 8 | 16, accx: true, ld: LdKind::Ip, qup: false } => fast,
        _ => false,
    }
}

