//! RV32IMAC decoder (the C3 is IMC, the C6 adds A). Compressed instructions are expanded to their 32-bit equivalent for
//! execution, but remember which `c.*` form they came from so the disassembler can be checked
//! against `riscv32-esp-elf-objdump` (tests/objdump_diff.rs).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Illegal,
    // RV32I
    Lui, Auipc, Jal, Jalr,
    Beq, Bne, Blt, Bge, Bltu, Bgeu,
    Lb, Lh, Lw, Lbu, Lhu,
    Sb, Sh, Sw,
    Addi, Slti, Sltiu, Xori, Ori, Andi, Slli, Srli, Srai,
    Add, Sub, Sll, Slt, Sltu, Xor, Srl, Sra, Or, And,
    Fence, FenceI,
    Ecall, Ebreak, Mret, Wfi,
    Csrrw, Csrrs, Csrrc, Csrrwi, Csrrsi, Csrrci,
    // RV32M
    Mul, Mulh, Mulhsu, Mulhu, Div, Divu, Rem, Remu,
    // RV32A (the C6); `imm` carries the aq/rl bits for the disassembler
    LrW, ScW, AmoSwapW, AmoAddW, AmoXorW, AmoAndW, AmoOrW, AmoMinW, AmoMaxW, AmoMinuW, AmoMaxuW,
}

/// Which compressed form an instruction was written as (`None` = a real 32-bit instruction).
/// Execution ignores this; only the disassembler cares.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Comp {
    None,
    Addi4spn, Lw, Sw,
    Nop, Addi, Jal, Li, Addi16sp, Lui, Srli, Srai, Andi, Sub, Xor, Or, And, J, Beqz, Bnez,
    Slli, Lwsp, Jr, Mv, Ebreak, Jalr, Add, Swsp,
}

#[derive(Clone, Copy, Debug)]
pub struct Insn {
    pub op: Op,
    pub rd: u8,
    pub rs1: u8,
    pub rs2: u8,
    /// signed immediate; for `jal`/branches the **absolute** target, for CSR ops the CSR number
    pub imm: i32,
    /// 2 for compressed, 4 otherwise
    pub len: u8,
    /// the compressed mnemonic this came from, for the disassembler
    pub comp: Comp,
    pub raw: u32,
}

impl Insn {
    const fn new(op: Op, rd: u8, rs1: u8, rs2: u8, imm: i32, len: u8, raw: u32) -> Insn {
        Insn { op, rd, rs1, rs2, imm, len, comp: Comp::None, raw }
    }
    const fn c(mut self, comp: Comp) -> Insn { self.comp = comp; self.len = 2; self }
    pub fn is_illegal(&self) -> bool { self.op == Op::Illegal }
}

const ILL2: Insn = Insn { op: Op::Illegal, rd: 0, rs1: 0, rs2: 0, imm: 0, len: 2, comp: Comp::None, raw: 0 };

#[inline(always)]
fn sext(v: u32, bits: u32) -> i32 { ((v << (32 - bits)) as i32) >> (32 - bits) }

/// Decode the instruction at `pc`. `bytes` is up to 4 bytes little-endian; only the first two
/// are read for a compressed instruction.
pub fn decode(pc: u32, bytes: [u8; 4]) -> Insn {
    let raw = u32::from_le_bytes(bytes);
    if raw & 3 != 3 { return decode_compressed(pc, raw & 0xffff); }
    decode32(pc, raw)
}

fn decode32(pc: u32, w: u32) -> Insn {
    let (rd, rs1, rs2) = (((w >> 7) & 0x1f) as u8, ((w >> 15) & 0x1f) as u8, ((w >> 20) & 0x1f) as u8);
    let f3 = (w >> 12) & 7;
    let f7 = w >> 25;
    let i_imm = (w as i32) >> 20;
    let s_imm = (((w & 0xfe00_0000) as i32) >> 20) | ((w >> 7) & 0x1f) as i32;
    let b_off = sext(((w >> 31) << 12) | (((w >> 7) & 1) << 11) | (((w >> 25) & 0x3f) << 5) | (((w >> 8) & 0xf) << 1), 13);
    let j_off = sext(((w >> 31) << 20) | (((w >> 12) & 0xff) << 12) | (((w >> 20) & 1) << 11) | (((w >> 21) & 0x3ff) << 1), 21);
    let mk = |op: Op, imm: i32| Insn::new(op, rd, rs1, rs2, imm, 4, w);

    match w & 0x7f {
        0x37 => mk(Op::Lui, (w & 0xffff_f000) as i32),
        0x17 => mk(Op::Auipc, (w & 0xffff_f000) as i32),
        0x6f => mk(Op::Jal, pc.wrapping_add(j_off as u32) as i32),
        0x67 if f3 == 0 => mk(Op::Jalr, i_imm),
        0x63 => {
            let op = match f3 { 0 => Op::Beq, 1 => Op::Bne, 4 => Op::Blt, 5 => Op::Bge, 6 => Op::Bltu, 7 => Op::Bgeu, _ => Op::Illegal };
            mk(op, pc.wrapping_add(b_off as u32) as i32)
        }
        0x03 => {
            let op = match f3 { 0 => Op::Lb, 1 => Op::Lh, 2 => Op::Lw, 4 => Op::Lbu, 5 => Op::Lhu, _ => Op::Illegal };
            mk(op, i_imm)
        }
        0x23 => {
            let op = match f3 { 0 => Op::Sb, 1 => Op::Sh, 2 => Op::Sw, _ => Op::Illegal };
            mk(op, s_imm)
        }
        0x13 => match f3 {
            0 => mk(Op::Addi, i_imm),
            2 => mk(Op::Slti, i_imm),
            3 => mk(Op::Sltiu, i_imm),
            4 => mk(Op::Xori, i_imm),
            6 => mk(Op::Ori, i_imm),
            7 => mk(Op::Andi, i_imm),
            1 if f7 == 0 => mk(Op::Slli, (rs2 & 0x1f) as i32),
            5 if f7 == 0 => mk(Op::Srli, (rs2 & 0x1f) as i32),
            5 if f7 == 0x20 => mk(Op::Srai, (rs2 & 0x1f) as i32),
            _ => mk(Op::Illegal, 0),
        },
        0x33 => {
            let op = match (f7, f3) {
                (0x00, 0) => Op::Add, (0x20, 0) => Op::Sub,
                (0x00, 1) => Op::Sll, (0x00, 2) => Op::Slt, (0x00, 3) => Op::Sltu,
                (0x00, 4) => Op::Xor, (0x00, 5) => Op::Srl, (0x20, 5) => Op::Sra,
                (0x00, 6) => Op::Or, (0x00, 7) => Op::And,
                (0x01, 0) => Op::Mul, (0x01, 1) => Op::Mulh, (0x01, 2) => Op::Mulhsu, (0x01, 3) => Op::Mulhu,
                (0x01, 4) => Op::Div, (0x01, 5) => Op::Divu, (0x01, 6) => Op::Rem, (0x01, 7) => Op::Remu,
                _ => Op::Illegal,
            };
            mk(op, 0)
        }
        0x0f => match f3 { 0 => mk(Op::Fence, i_imm), 1 => mk(Op::FenceI, 0), _ => mk(Op::Illegal, 0) },
        0x2f if f3 == 2 => {
            let op = match w >> 27 {
                0x02 if rs2 == 0 => Op::LrW, 0x03 => Op::ScW, 0x01 => Op::AmoSwapW, 0x00 => Op::AmoAddW,
                0x04 => Op::AmoXorW, 0x0c => Op::AmoAndW, 0x08 => Op::AmoOrW, 0x10 => Op::AmoMinW,
                0x14 => Op::AmoMaxW, 0x18 => Op::AmoMinuW, 0x1c => Op::AmoMaxuW, _ => Op::Illegal,
            };
            mk(op, ((w >> 25) & 3) as i32)                 // bit 1 = aq, bit 0 = rl
        }
        0x73 => match f3 {
            0 => match w >> 20 {
                0x000 if rd == 0 && rs1 == 0 => mk(Op::Ecall, 0),
                0x001 if rd == 0 && rs1 == 0 => mk(Op::Ebreak, 0),
                0x302 if rd == 0 && rs1 == 0 => mk(Op::Mret, 0),
                0x105 if rd == 0 && rs1 == 0 => mk(Op::Wfi, 0),
                _ => mk(Op::Illegal, 0),
            },
            1 => mk(Op::Csrrw, (w >> 20) as i32), 2 => mk(Op::Csrrs, (w >> 20) as i32), 3 => mk(Op::Csrrc, (w >> 20) as i32),
            5 => mk(Op::Csrrwi, (w >> 20) as i32), 6 => mk(Op::Csrrsi, (w >> 20) as i32), 7 => mk(Op::Csrrci, (w >> 20) as i32),
            _ => mk(Op::Illegal, 0),
        },
        _ => mk(Op::Illegal, 0),
    }
}

fn decode_compressed(pc: u32, c: u32) -> Insn {
    let f3 = (c >> 13) & 7;
    let rd = ((c >> 7) & 0x1f) as u8;             // also rs1 in the CI/CR formats
    let rs2 = ((c >> 2) & 0x1f) as u8;
    let rdp = (((c >> 2) & 7) + 8) as u8;         // rd'/rs2'
    let rs1p = (((c >> 7) & 7) + 8) as u8;        // rs1'
    let imm6 = sext((((c >> 12) & 1) << 5) | ((c >> 2) & 0x1f), 6);
    // CL/CS word offset and the two stack-pointer forms
    let lw_off = ((((c >> 5) & 1) << 6) | (((c >> 10) & 7) << 3) | (((c >> 6) & 1) << 2)) as i32;
    let cj_off = sext((((c >> 12) & 1) << 11) | (((c >> 11) & 1) << 4) | (((c >> 9) & 3) << 8) | (((c >> 8) & 1) << 10)
                      | (((c >> 7) & 1) << 6) | (((c >> 6) & 1) << 7) | (((c >> 3) & 7) << 1) | (((c >> 2) & 1) << 5), 12);
    let cb_off = sext((((c >> 12) & 1) << 8) | (((c >> 10) & 3) << 3) | (((c >> 5) & 3) << 6)
                      | (((c >> 3) & 3) << 1) | (((c >> 2) & 1) << 5), 9);
    let mk = |op: Op, rd: u8, rs1: u8, rs2: u8, imm: i32, comp: Comp| Insn::new(op, rd, rs1, rs2, imm, 2, c).c(comp);

    match (c & 3, f3) {
        // ---- quadrant 0
        (0, 0) => {
            let imm = ((((c >> 11) & 3) << 4) | (((c >> 7) & 0xf) << 6) | (((c >> 6) & 1) << 2) | (((c >> 5) & 1) << 3)) as i32;
            if imm == 0 { return ILL2; }                       // canonical illegal / reserved
            mk(Op::Addi, rdp, 2, 0, imm, Comp::Addi4spn)
        }
        (0, 2) => mk(Op::Lw, rdp, rs1p, 0, lw_off, Comp::Lw),
        (0, 6) => mk(Op::Sw, 0, rs1p, rdp, lw_off, Comp::Sw),
        // ---- quadrant 1
        (1, 0) if rd == 0 => mk(Op::Addi, 0, 0, 0, imm6, Comp::Nop),
        (1, 0) => mk(Op::Addi, rd, rd, 0, imm6, Comp::Addi),
        (1, 1) => mk(Op::Jal, 1, 0, 0, pc.wrapping_add(cj_off as u32) as i32, Comp::Jal),
        (1, 2) => mk(Op::Addi, rd, 0, 0, imm6, Comp::Li),
        (1, 3) if rd == 2 => {
            let imm = sext((((c >> 12) & 1) << 9) | (((c >> 6) & 1) << 4) | (((c >> 5) & 1) << 6)
                           | (((c >> 3) & 3) << 7) | (((c >> 2) & 1) << 5), 10);
            if imm == 0 { return ILL2; }
            mk(Op::Addi, 2, 2, 0, imm, Comp::Addi16sp)
        }
        (1, 3) if rd != 0 => {
            let imm = sext((((c >> 12) & 1) << 17) | (((c >> 2) & 0x1f) << 12), 18);
            if imm == 0 { return ILL2; }
            mk(Op::Lui, rd, 0, 0, imm, Comp::Lui)
        }
        (1, 4) => {
            let shamt = ((((c >> 12) & 1) << 5) | ((c >> 2) & 0x1f)) as i32;
            match (c >> 10) & 3 {
                0 if shamt < 32 => mk(Op::Srli, rs1p, rs1p, 0, shamt, Comp::Srli),
                1 if shamt < 32 => mk(Op::Srai, rs1p, rs1p, 0, shamt, Comp::Srai),
                2 => mk(Op::Andi, rs1p, rs1p, 0, imm6, Comp::Andi),
                3 if (c >> 12) & 1 == 0 => {
                    let op = match (c >> 5) & 3 { 0 => Op::Sub, 1 => Op::Xor, 2 => Op::Or, _ => Op::And };
                    let comp = match (c >> 5) & 3 { 0 => Comp::Sub, 1 => Comp::Xor, 2 => Comp::Or, _ => Comp::And };
                    mk(op, rs1p, rs1p, rdp, 0, comp)
                }
                _ => Insn { raw: c, ..ILL2 }, // RV64-only arithmetic or unsupported custom shifts.
            }
        }
        (1, 5) => mk(Op::Jal, 0, 0, 0, pc.wrapping_add(cj_off as u32) as i32, Comp::J),
        (1, 6) => mk(Op::Beq, 0, rs1p, 0, pc.wrapping_add(cb_off as u32) as i32, Comp::Beqz),
        (1, 7) => mk(Op::Bne, 0, rs1p, 0, pc.wrapping_add(cb_off as u32) as i32, Comp::Bnez),
        // ---- quadrant 2
        // RV32C shamt[5] = 1 belongs to custom extensions, not the standard shifts.
        (2, 0) if c & (1 << 12) == 0 => mk(Op::Slli, rd, rd, 0, rs2 as i32, Comp::Slli),
        (2, 2) if rd != 0 => {
            let imm = ((((c >> 12) & 1) << 5) | (((c >> 4) & 7) << 2) | (((c >> 2) & 3) << 6)) as i32;
            mk(Op::Lw, rd, 2, 0, imm, Comp::Lwsp)
        }
        (2, 4) => match ((c >> 12) & 1, rd, rs2) {
            (0, 0, _) => ILL2,
            (0, _, 0) => mk(Op::Jalr, 0, rd, 0, 0, Comp::Jr),
            (0, _, _) => mk(Op::Add, rd, 0, rs2, 0, Comp::Mv),
            (1, 0, 0) => mk(Op::Ebreak, 0, 0, 0, 0, Comp::Ebreak),
            (1, _, 0) => mk(Op::Jalr, 1, rd, 0, 0, Comp::Jalr),
            (_, _, _) => mk(Op::Add, rd, rd, rs2, 0, Comp::Add),
        },
        (2, 6) => {
            let imm = ((((c >> 9) & 0xf) << 2) | (((c >> 7) & 3) << 6)) as i32;
            mk(Op::Sw, 0, 2, rs2, imm, Comp::Swsp)
        }
        _ => Insn { raw: c, ..ILL2 }, // Includes every F/D form: no FPU on the C3.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rv32_compressed_shifts_reject_custom_high_shamt_bit() {
        // RISC-V C v2.0: shamt[5] = 1 is custom space for all three RV32C shifts.
        // https://docs.riscv.org/reference/isa/unpriv/c-st-ext.html
        for (base, op) in [(0x0082, Op::Slli), (0x8001, Op::Srli), (0x8401, Op::Srai)] {
            for shamt in 0..64 {
                let raw = base | (shamt & 31) << 2 | (shamt >> 5) << 12;
                let insn = decode_compressed(0, raw);
                assert_eq!(insn.raw, raw);
                assert_eq!(insn.len, 2);
                assert_eq!(insn.op, if shamt < 32 { op } else { Op::Illegal }, "{raw:04x}");
                if shamt < 32 { assert_eq!(insn.imm, shamt as i32); }
            }
        }
    }
}
