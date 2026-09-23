//! Tiny assembler for encoded programs, checked against the decoder by region tests.
pub fn j(pc: u32, target: u32) -> Vec<u8> { w24(0x6 | ((target.wrapping_sub(pc + 4) & 0x3ffff) << 6)) }
pub fn rri8(op0: u32, r: u32, s: u32, t: u32, imm8: u32) -> Vec<u8> { w24(op0 | (t << 4) | (s << 8) | (r << 12) | ((imm8 & 0xff) << 16)) }
/// beq=1 bne=9 blt=2 bge=0xa bltu=3 bgeu=0xb
pub fn bcc(r: u32, pc: u32, s: u32, t: u32, target: u32) -> Vec<u8> { rri8(7, r, s, t, target.wrapping_sub(pc + 4)) }
/// beqz=0 bnez=1 bltz=2 bgez=3
pub fn bz(m: u32, pc: u32, s: u32, target: u32) -> Vec<u8> { w24(0x6 | 0x10 | (m << 6) | (s << 8) | ((target.wrapping_sub(pc + 4) & 0xfff) << 12)) }
pub fn l8ui(t: u32, s: u32, imm: u32) -> Vec<u8> { rri8(2, 0, s, t, imm) }
pub fn l16ui(t: u32, s: u32, imm: u32) -> Vec<u8> { rri8(2, 1, s, t, imm / 2) }
pub fn s8i(t: u32, s: u32, imm: u32) -> Vec<u8> { rri8(2, 4, s, t, imm) }
pub fn s16i(t: u32, s: u32, imm: u32) -> Vec<u8> { rri8(2, 5, s, t, imm / 2) }
pub fn addi(t: u32, s: u32, imm: i32) -> Vec<u8> { rri8(2, 0xc, s, t, imm as u32) }
pub fn xor(r: u32, s: u32, t: u32) -> Vec<u8> { w24((t << 4) | (s << 8) | (r << 12) | (3 << 20)) }
pub fn rsr(t: u32, sr: u32) -> Vec<u8> { w24((3 << 16) | (sr << 8) | (t << 4)) }
/// RUR: op2 = 14, op1 = 3; the user register number is s:t.
pub fn rur(r: u32, ur: u32) -> Vec<u8> { w24((0xe3 << 16) | (r << 12) | (ur << 4)) }
/// WUR: op2 = 15, op1 = 3; the user register number is r:s.
pub fn wur(t: u32, ur: u32) -> Vec<u8> { w24((0xf3 << 16) | (ur << 8) | (t << 4)) }
/// ssr=0 ssl=1
pub fn shift_setup(kind: u32, s: u32) -> Vec<u8> { w24((4 << 20) | (kind << 12) | (s << 8)) }
/// loop=8 loopnez=9 loopgtz=10; the end is pc + 4 + imm8
pub fn lp(r: u32, pc: u32, s: u32, end: u32) -> Vec<u8> { w24(0x76 | (s << 8) | (r << 12) | ((end - pc - 4) << 16)) }
pub fn l32r(t: u32, pc: u32, literal: u32) -> Vec<u8> { w24(0x1 | (t << 4) | (((literal.wrapping_sub((pc + 3) & !3) >> 2) & 0xffff) << 8)) }
pub fn jx(s: u32) -> Vec<u8> { w24(0xa0 | (s << 8)) }
pub fn entry(s: u32, frame: u32) -> Vec<u8> { w24(0x36 | (s << 8) | ((frame >> 3) << 12)) }
pub fn call8(pc: u32, target: u32) -> Vec<u8> { w24(0x25 | (((target.wrapping_sub((pc & !3) + 4) >> 2) & 0x3ffff) << 6)) }
pub fn callx8(s: u32) -> Vec<u8> { w24(0xe0 | (s << 8)) }
pub fn wfr(f: u32, a: u32) -> Vec<u8> { w24(0xfa0050 | (f << 12) | (a << 8)) }
pub fn rfr(a: u32, f: u32) -> Vec<u8> { w24(0xfa0040 | (a << 12) | (f << 8)) }
pub fn retw_n() -> Vec<u8> { w16(0xf01d) }
pub fn l32i_n(t: u32, s: u32, imm: u32) -> Vec<u8> { w16(0x8 | (t << 4) | (s << 8) | ((imm / 4) << 12)) }
pub fn s32i_n(t: u32, s: u32, imm: u32) -> Vec<u8> { w16(0x9 | (t << 4) | (s << 8) | ((imm / 4) << 12)) }
pub fn and(r: u32, s: u32, t: u32) -> Vec<u8> { w24((t << 4) | (s << 8) | (r << 12) | (1 << 20)) }
/// Assemble a PIE instruction from its table entry, the inverse of `pie::extract`.
pub fn pie(name: &str, fields: &[(crate::pie::Role, i32)]) -> Vec<u8> {
    let p = crate::pie::OPS.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no PIE op {name}"));
    let mut w = p.value;
    for &(role, v) in fields {
        let f = p.fields.iter().find(|f| f.role == role).unwrap_or_else(|| panic!("{name} has no {role:?}"));
        let v = (v / f.scale as i32) as u32;
        for &(hi, lo, wp) in f.pieces { let n = hi - lo + 1; w |= ((v >> lo) & ((1 << n) - 1)) << wp; }
    }
    (0..p.len).map(|k| (w >> (8 * k)) as u8).collect()
}
pub fn nop_n() -> Vec<u8> { w16(0xf03d) }
pub fn addi_n(r: u32, s: u32, imm: i32) -> Vec<u8> { w16(0xb | (((if imm == -1 { 0 } else { imm as u32 }) & 0xf) << 4) | (s << 8) | (r << 12)) }
pub fn add_n(r: u32, s: u32, t: u32) -> Vec<u8> { w16(0xa | (t << 4) | (s << 8) | (r << 12)) }
pub fn mov_n(t: u32, s: u32) -> Vec<u8> { w16(0xd | (t << 4) | (s << 8)) }
pub fn movi_n(s: u32, imm: u32) -> Vec<u8> { w16(0xc | (((imm >> 4) & 7) << 4) | (s << 8) | ((imm & 0xf) << 12)) }
fn w24(w: u32) -> Vec<u8> { vec![w as u8, (w >> 8) as u8, (w >> 16) as u8] }
fn w16(w: u32) -> Vec<u8> { vec![w as u8, (w >> 8) as u8] }
