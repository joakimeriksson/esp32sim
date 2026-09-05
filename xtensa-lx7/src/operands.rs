//! Windowed integer operand effects. Encoding nibbles also contain immediates,
//! selectors and floating/boolean registers: they are not themselves AR operands.
use crate::{Insn, Op};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GprEffects {
    pub reads: u16,
    /// Writes on normal completion, in the instruction's current register window.
    pub writes: u16,
    pub conditional_writes: u16,
    /// Known operands whose direction/window is not classified. Consumers doing
    /// liveness analysis must treat these as a barrier, not as dead registers.
    pub unclassified: u16,
    /// The instruction can change the logical-to-physical AR mapping. Liveness
    /// consumers must end their window proof here, even with no AR operands.
    pub changes_window: bool,
}

impl GprEffects {
    pub fn touched(self) -> u16 {
        self.reads | self.writes | self.conditional_writes | self.unclassified
    }
    pub fn max_ar(self) -> u8 {
        (15u32.saturating_sub(self.touched().leading_zeros())) as u8
    }
}

impl Insn {
    /// Architectural AR operands, including destinations even when a condition
    /// suppresses their write. FP and boolean register numbers never occur here.
    pub fn gpr_effects(&self) -> GprEffects {
        use Op::*;
        let (r, s, t) = (1u16 << self.r, 1u16 << self.s, 1u16 << self.t);
        let rw = |reads, writes| GprEffects { reads, writes, ..GprEffects::default() };
        let conditional = |reads, conditional_writes| GprEffects { reads, conditional_writes, ..GprEffects::default() };
        let window = |mut effects: GprEffects| { effects.changes_window = true; effects };
        match self.op {
            Add | AddN | Sub | Addx2 | Addx4 | Addx8 | Subx2 | Subx4 | Subx8
            | And | Or | Xor | Min | Max | Minu | Maxu | Src | Mull | Muluh | Mulsh
            | Mul16u | Mul16s | Quou | Quos | Remu | Rems | Salt | Saltu => rw(s | t, r),
            Moveqz | Movnez | Movltz | Movgez => conditional(s | t, r),
            Movf | Movt => conditional(s, r),
            AddiN | Sext | Clamps | Slli | Sll => rw(s, r),
            Neg | Abs | Extui | Srai | Srli | Srl | Sra => rw(t, r),
            Nsa | Nsau | Movsp | Mov | MovN | Addi | Addmi => rw(s, t),
            Rur | RoundS | TruncS | FloorS | CeilS | UtruncS | Rfr => rw(0, r),
            Rsr | Rsil | Movi | L32r => rw(0, t),
            Wur => rw(t, 0),
            Wsr => if self.imm as u32 == crate::state::sr::WINDOWBASE { window(rw(t, 0)) } else { rw(t, 0) },
            Xsr => if self.imm as u32 == crate::state::sr::WINDOWBASE {
                window(GprEffects { reads: t, unclassified: t, ..GprEffects::default() })
            } else { rw(t, t) },
            MoviN => rw(0, s),
            Call0 => rw(0, 1), Call4 => rw(0, 1 << 4),
            Call8 => rw(0, 1 << 8), Call12 => rw(0, 1 << 12),
            Callx0 => rw(s, 1), Callx4 => rw(s, 1 << 4),
            Callx8 => rw(s, 1 << 8), Callx12 => rw(s, 1 << 12),
            Ret | RetN => rw(1, 0),
            Retw | RetwN => window(rw(1, 0)),
            Rotw | Rfwo | Rfwu => window(rw(0, 0)),
            // ENTRY reads the old window and writes the rotated one.
            Entry => window(GprEffects { reads: s, unclassified: s, ..GprEffects::default() }),
            Ssr | Ssl | Ssa8l | Ssa8b | Jx | Iitlb | Idtlb
            | Beqz | Bnez | Bltz | Bgez | BeqzN | BnezN | Beqi | Bnei | Blti
            | Bgei | Bltui | Bgeui | Bbci | Bbsi | Loop | Loopnez | Loopgtz
            | Lsi | Ssi | FloatS | UfloatS | Wfr
            | Dpfr | Dpfw | Dpfro | Dpfwo | Dhwb | Dhwbi | Dhi | Dii | Ipf
            | Ihi | Iii | Ipfl | Ihu | Iiu | Dpfl | Dhu | Diu => rw(s, 0),
            L8ui | L16ui | L16si | L32i | L32iN | L32ai | L32e
            | Rer | Ritlb0 | Ritlb1 | Rdtlb0 | Rdtlb1 | Pitlb | Pdtlb => rw(s, t),
            S8i | S16i | S32i | S32iN | S32ri | S32e | S32nb | Wer | Witlb | Wdtlb
            | Bnone | Beq | Blt | Bltu | Ball | Bbc | Bany | Bne | Bge | Bgeu | Bnall | Bbs
            | Lsx | Ssx => rw(s | t, 0),
            S32c1i => rw(s | t, t),
            Lsip | Ssip => rw(s, s),
            Lsxp | Ssxp => rw(s | t, s),
            MoveqzS | MovnezS | MovltzS | MovgezS => rw(t, 0),
            Mac16 => match (self.raw >> 20) & 15 {
                7 => rw(s | t, 0), 3 => rw(s, 0), 6 => rw(t, 0),
                4 | 5 => rw(s | t, s), 0 | 1 | 8 | 9 => rw(s, s),
                _ => rw(0, 0),
            },
            Pie => {
                // Reuse the extension's typed fields; don't create a parallel
                // opcode table or infer AR operands from the raw nibbles.
                GprEffects { unclassified: crate::pie::gpr_mask(self.raw, self.imm as usize), ..GprEffects::default() }
            }
            Ill | IllN | Nop | NopN | Break | BreakN | Syscall | Simcall | Waiti
            | Isync | Rsync | Esync | Dsync | Excw | Memw | Extw | J
            | Rfe | Rfue | Rfde | Rfi | Rfme | Bf | Bt | Ssai
            | Andb | Andbc | Orb | Orbc | Xorb | Any4 | All4 | Any8 | All8
            | AddS | SubS | MulS | MaddS | MsubS | MaddnS | DivnS | MovS | AbsS | NegS
            | Div0S | Nexp01S | ConstS | Recip0S | Rsqrt0S | Sqrt0S | MksadjS
            | MkdadjS | AddexpS | AddexpmS | UnS | OeqS | UeqS | OltS | UltS | OleS
            | UleS | MovfS | MovtS => rw(0, 0),
        }
    }
}
