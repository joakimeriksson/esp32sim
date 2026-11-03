//! IEEE 754 binary32 arithmetic on integers, with RISC-V F's choices where the standard leaves one
//! (RISC-V unprivileged ISA, F, "NaN Generation and Propagation" and "Subnormal Arithmetic"): every
//! NaN result is the canonical NaN, and tininess is detected after rounding. Host floats are not
//! used: their rounding mode cannot be set portably and their exception flags cannot be read.
//!
//! Each operation computes its exact result as `sig × 2^exp` (or one with a sticky bit below every
//! rounding position) and rounds it once in `round`.

use std::cmp::Ordering;

/// fflags bits (F, "Floating-Point Control and Status Register").
pub mod flag {
    pub const NX: u32 = 1; pub const UF: u32 = 2; pub const OF: u32 = 4; pub const DZ: u32 = 8; pub const NV: u32 = 16;
}
use flag::*;

/// The rounding modes; rm encodings 5 and 6 are reserved, 7 (DYN) is resolved by the caller.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rm { Rne, Rtz, Rdn, Rup, Rmm }

impl Rm {
    pub fn from_bits(bits: u32) -> Option<Rm> {
        Some(match bits { 0 => Rm::Rne, 1 => Rm::Rtz, 2 => Rm::Rdn, 3 => Rm::Rup, 4 => Rm::Rmm, _ => return None })
    }
}

pub const CANONICAL_NAN: u32 = 0x7fc0_0000;
pub const SIGN: u32 = 0x8000_0000;
const INF: u32 = 0x7f80_0000;
const MAX_FINITE: u32 = 0x7f7f_ffff;

#[inline] fn is_nan(a: u32) -> bool { a & !SIGN > INF }
#[inline] fn is_snan(a: u32) -> bool { is_nan(a) && a & 0x0040_0000 == 0 }
#[inline] fn is_inf(a: u32) -> bool { a & !SIGN == INF }
#[inline] fn is_zero(a: u32) -> bool { a & !SIGN == 0 }
#[inline] fn neg(a: u32) -> bool { a & SIGN != 0 }

fn nan_result(inputs: &[u32]) -> (u32, u32) {
    (CANONICAL_NAN, if inputs.iter().any(|&x| is_snan(x)) { NV } else { 0 })
}

/// A finite nonzero value as `sig × 2^exp`.
fn unpack(a: u32) -> (u128, i32) {
    let (e, m) = ((a >> 23) & 0xff, a & 0x7f_ffff);
    if e == 0 { (m as u128, -149) } else { ((m | 0x80_0000) as u128, e as i32 - 150) }
}

fn bit_len(m: u128) -> i32 { 128 - m.leading_zeros() as i32 }

/// `sig >> s` for `sig > 0`, where the discarded bits lie against half a unit of the last kept bit,
/// and whether any were nonzero.
fn shift(sig: u128, s: i32) -> (u128, Ordering, bool) {
    match s {
        ..=0 => (sig << -s, Ordering::Less, false),
        1..=127 => {
            let rest = sig & ((1u128 << s) - 1);
            (sig >> s, rest.cmp(&(1u128 << (s - 1))), rest != 0)
        }
        128 => (0, sig.cmp(&(1u128 << 127)), true),
        _ => (0, Ordering::Less, true),
    }
}

fn round_up(rm: Rm, negative: bool, odd: bool, half: Ordering, inexact: bool) -> bool {
    match rm {
        Rm::Rne => half == Ordering::Greater || (half == Ordering::Equal && odd),
        Rm::Rmm => half != Ordering::Less,
        Rm::Rtz => false,
        Rm::Rdn => inexact && negative,
        Rm::Rup => inexact && !negative,
    }
}

/// Round the exact value `±sig × 2^exp` (`sig > 0`) to binary32: 24 significant bits, no finer
/// than the subnormal quantum 2^-149.
fn round(negative: bool, sig: u128, exp: i32, rm: Rm) -> (u32, u32) {
    let top = exp + bit_len(sig) - 1;
    let mut q = (top - 23).max(-149);
    let (kept, half, inexact) = shift(sig, q - exp);
    let mut m = kept + round_up(rm, negative, kept & 1 == 1, half, inexact) as u128;
    if m == 1 << 24 { m = 1 << 23; q += 1; }
    let sign = if negative { SIGN } else { 0 };
    if m >= 1 << 23 && q + 150 >= 0xff {
        let to_inf = match rm { Rm::Rne | Rm::Rmm => true, Rm::Rtz => false, Rm::Rdn => negative, Rm::Rup => !negative };
        return (sign | if to_inf { INF } else { MAX_FINITE }, OF | NX);
    }
    let mut flags = if inexact { NX } else { 0 };
    // Tiny when rounding to 24 bits with an unbounded exponent stays below 2^-126.
    if inexact && top < -126 {
        let tiny = top < -127 || {
            let (k, h, x) = shift(sig, top - 23 - exp);
            k + round_up(rm, negative, k & 1 == 1, h, x) as u128 != 1 << 24
        };
        if tiny { flags |= UF; }
    }
    let bits = if m >= 1 << 23 { ((q + 150) as u32) << 23 | (m as u32 & 0x7f_ffff) } else { m as u32 };
    (sign | bits, flags)
}

/// An exact zero sum of opposite signs is +0, or -0 when rounding down (IEEE 754-2008 6.3).
fn exact_zero(rm: Rm) -> u32 { if rm == Rm::Rdn { SIGN } else { 0 } }

/// `±x + ±y` for finite nonzero operands given as `sig × 2^exp`, `sig < 2^49`.
fn sum(nx: bool, mx: u128, ex: i32, ny: bool, my: u128, ey: i32, rm: Rm) -> (u32, u32) {
    let (tx, ty) = (ex + bit_len(mx) - 1, ey + bit_len(my) - 1);
    let ((nx, mx, ex, tx), (ny, my, ey, ty)) = if ty > tx { ((ny, my, ey, ty), (nx, mx, ex, tx)) } else { ((nx, mx, ex, tx), (ny, my, ey, ty)) };
    if tx - ty > 60 {
        // y lies wholly below bit 0 of x widened to 61 bits, far under any rounding position,
        // so a sticky unit stands in for it.
        let sh = 61 - bit_len(mx);
        let w = mx << sh;
        return round(nx, if nx == ny { w + 1 } else { w - 1 }, ex - sh, rm);
    }
    let e = ex.min(ey);
    let (sx, sy) = (mx << (ex - e), my << (ey - e));
    let (n, m) = if nx == ny { (nx, sx + sy) } else if sx >= sy { (nx, sx - sy) } else { (ny, sy - sx) };
    if m == 0 { return (exact_zero(rm), 0); }
    round(n, m, e, rm)
}

pub fn add(a: u32, b: u32, rm: Rm) -> (u32, u32) {
    if is_nan(a) || is_nan(b) { return nan_result(&[a, b]); }
    if is_inf(a) || is_inf(b) {
        if is_inf(a) && is_inf(b) && neg(a) != neg(b) { return (CANONICAL_NAN, NV); }
        return (if is_inf(a) { a } else { b }, 0);
    }
    match (is_zero(a), is_zero(b)) {
        (true, true) => (if neg(a) == neg(b) { a } else { exact_zero(rm) }, 0),
        (true, false) => (b, 0),
        (false, true) => (a, 0),
        _ => {
            let ((ma, ea), (mb, eb)) = (unpack(a), unpack(b));
            sum(neg(a), ma, ea, neg(b), mb, eb, rm)
        }
    }
}

pub fn sub(a: u32, b: u32, rm: Rm) -> (u32, u32) { add(a, b ^ SIGN, rm) }

pub fn mul(a: u32, b: u32, rm: Rm) -> (u32, u32) {
    if is_nan(a) || is_nan(b) { return nan_result(&[a, b]); }
    let sign = (a ^ b) & SIGN;
    if is_inf(a) || is_inf(b) {
        return if is_zero(a) || is_zero(b) { (CANONICAL_NAN, NV) } else { (sign | INF, 0) };
    }
    if is_zero(a) || is_zero(b) { return (sign, 0); }
    let ((ma, ea), (mb, eb)) = (unpack(a), unpack(b));
    round(sign != 0, ma * mb, ea + eb, rm)
}

pub fn div(a: u32, b: u32, rm: Rm) -> (u32, u32) {
    if is_nan(a) || is_nan(b) { return nan_result(&[a, b]); }
    let sign = (a ^ b) & SIGN;
    if is_inf(a) { return if is_inf(b) { (CANONICAL_NAN, NV) } else { (sign | INF, 0) }; }
    if is_inf(b) { return (sign, 0); }
    if is_zero(b) { return if is_zero(a) { (CANONICAL_NAN, NV) } else { (sign | INF, DZ) }; }
    if is_zero(a) { return (sign, 0); }
    let ((ma, ea), (mb, eb)) = (unpack(a), unpack(b));
    // At least 39 quotient bits; the remainder becomes a sticky half-unit below them.
    let n = ma << 62;
    let (q, r) = (n / mb, n % mb);
    round(sign != 0, q << 1 | (r != 0) as u128, ea - eb - 63, rm)
}

/// Floor square root and remainder, digit by digit.
fn isqrt(n: u128) -> (u128, u128) {
    let (mut rem, mut root, mut bit) = (n, 0u128, 1u128 << 126);
    while bit > rem { bit >>= 2; }
    while bit != 0 {
        if rem >= root + bit { rem -= root + bit; root = (root >> 1) + bit; } else { root >>= 1; }
        bit >>= 2;
    }
    (root, rem)
}

pub fn sqrt(a: u32, rm: Rm) -> (u32, u32) {
    if is_nan(a) { return nan_result(&[a]); }
    if is_zero(a) { return (a, 0); }
    if neg(a) { return (CANONICAL_NAN, NV); }
    if is_inf(a) { return (a, 0); }
    let (mut m, mut e) = unpack(a);
    if e & 1 != 0 { m <<= 1; e -= 1; }
    // At least 51 root bits; the remainder becomes a sticky half-unit below them.
    let (root, rem) = isqrt(m << 100);
    round(false, root << 1 | (rem != 0) as u128, (e - 100) / 2 - 1, rm)
}

/// `a × b + c` with one rounding. Invalid for ∞ × 0 even when `c` is a quiet NaN
/// (F, "Single-Precision Floating-Point Computational Instructions").
pub fn fma(a: u32, b: u32, c: u32, rm: Rm) -> (u32, u32) {
    let inf_times_zero = (is_inf(a) && is_zero(b)) || (is_zero(a) && is_inf(b));
    if is_nan(a) || is_nan(b) || is_nan(c) {
        let (r, f) = nan_result(&[a, b, c]);
        return (r, f | if inf_times_zero { NV } else { 0 });
    }
    if inf_times_zero { return (CANONICAL_NAN, NV); }
    let np = neg(a) != neg(b);
    if is_inf(a) || is_inf(b) {
        if is_inf(c) && neg(c) != np { return (CANONICAL_NAN, NV); }
        return (if np { SIGN | INF } else { INF }, 0);
    }
    if is_inf(c) { return (c, 0); }
    if is_zero(a) || is_zero(b) {
        return (if !is_zero(c) || neg(c) == np { c } else { exact_zero(rm) }, 0);
    }
    let ((ma, ea), (mb, eb)) = (unpack(a), unpack(b));
    if is_zero(c) { return round(np, ma * mb, ea + eb, rm); }
    let (mc, ec) = unpack(c);
    sum(np, ma * mb, ea + eb, neg(c), mc, ec, rm)
}

/// Neither operand NaN; -0 equals +0.
fn less(a: u32, b: u32) -> bool {
    if is_zero(a) && is_zero(b) { return false; }
    match (neg(a), neg(b)) { (true, false) => true, (false, true) => false, (false, false) => a < b, (true, true) => a > b }
}

/// FEQ is quiet: only a signaling NaN is invalid. FLT and FLE signal on any NaN.
pub fn eq(a: u32, b: u32) -> (bool, u32) {
    if is_nan(a) || is_nan(b) { return (false, if is_snan(a) || is_snan(b) { NV } else { 0 }); }
    (a == b || (is_zero(a) && is_zero(b)), 0)
}

pub fn lt(a: u32, b: u32) -> (bool, u32) {
    if is_nan(a) || is_nan(b) { return (false, NV); }
    (less(a, b), 0)
}

pub fn le(a: u32, b: u32) -> (bool, u32) {
    if is_nan(a) || is_nan(b) { return (false, NV); }
    (!less(b, a), 0)
}

/// IEEE 754-2019 minimumNumber/maximumNumber, with -0 below +0 (F v2.2).
fn min_max(a: u32, b: u32, max: bool) -> (u32, u32) {
    let flags = if is_snan(a) || is_snan(b) { NV } else { 0 };
    let r = match (is_nan(a), is_nan(b)) {
        (true, true) => CANONICAL_NAN,
        (true, false) => b,
        (false, true) => a,
        _ => {
            let a_below = if neg(a) != neg(b) { neg(a) } else if neg(a) { a > b } else { a < b };
            if a_below != max { a } else { b }
        }
    };
    (r, flags)
}

pub fn min(a: u32, b: u32) -> (u32, u32) { min_max(a, b, false) }
pub fn max(a: u32, b: u32) -> (u32, u32) { min_max(a, b, true) }

/// FCLASS's one-hot mask (F, "Single-Precision Floating-Point Classify Instruction").
pub fn classify(a: u32) -> u32 {
    let n = neg(a);
    let bit = if is_nan(a) { if is_snan(a) { 8 } else { 9 } }
        else if is_inf(a) { if n { 0 } else { 7 } }
        else if is_zero(a) { if n { 3 } else { 4 } }
        else if a & INF == 0 { if n { 2 } else { 5 } }
        else if n { 1 } else { 6 };
    1 << bit
}

/// The magnitude `a` rounds to as an integer (`None` past 2^64), and whether that was inexact.
fn round_to_int(a: u32, rm: Rm) -> (Option<u128>, bool) {
    if is_inf(a) { return (None, false); }
    if is_zero(a) { return (Some(0), false); }
    let (m, e) = unpack(a);
    if e >= 0 { return (if e > 40 { None } else { Some(m << e) }, false); }
    let (kept, half, inexact) = shift(m, -e);
    (Some(kept + round_up(rm, neg(a), kept & 1 == 1, half, inexact) as u128), inexact)
}

/// FCVT.W.S: a rounded value out of range, or NaN, clips and is invalid, never also inexact
/// (F, "Single-Precision Floating-Point Conversion and Move Instructions").
pub fn to_i32(a: u32, rm: Rm) -> (u32, u32) {
    if is_nan(a) { return (i32::MAX as u32, NV); }
    let n = neg(a);
    let (mag, inexact) = round_to_int(a, rm);
    match mag {
        Some(m) if m <= if n { 1 << 31 } else { (1 << 31) - 1 } => (if n { (m as u32).wrapping_neg() } else { m as u32 }, if inexact { NX } else { 0 }),
        _ => (if n { i32::MIN as u32 } else { i32::MAX as u32 }, NV),
    }
}

/// FCVT.WU.S: a negative value that rounds to 0 is valid; any other negative one clips to 0.
pub fn to_u32(a: u32, rm: Rm) -> (u32, u32) {
    if is_nan(a) { return (u32::MAX, NV); }
    let n = neg(a);
    match round_to_int(a, rm) {
        (Some(0), inexact) => (0, if inexact { NX } else { 0 }),
        (Some(m), inexact) if !n && m <= u32::MAX as u128 => (m as u32, if inexact { NX } else { 0 }),
        _ => (if n { 0 } else { u32::MAX }, NV),
    }
}

pub fn from_i32(v: u32, rm: Rm) -> (u32, u32) {
    let v = v as i32;
    if v == 0 { return (0, 0); }
    round(v < 0, v.unsigned_abs() as u128, 0, rm)
}

pub fn from_u32(v: u32, rm: Rm) -> (u32, u32) {
    if v == 0 { return (0, 0); }
    round(false, v as u128, 0, rm)
}
