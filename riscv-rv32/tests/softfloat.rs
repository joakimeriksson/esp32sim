//! The binary32 soft-float against two independent oracles. In round-to-nearest-even it must match
//! the host's IEEE binary32 arithmetic bit for bit (a NaN result being the canonical NaN). In every
//! mode, with its NX/UF/OF flags, it must match a reference built from binary64: the exact result,
//! rounded to odd at binary64's 53 bits, rounds to binary32 correctly in any mode because 53 ≥ 24 + 2
//! (Boldo and Melquiond, "Emulation of FMA and correctly rounded sums: proved algorithms using
//! rounding to odd", IEEE Trans. Computers 57(4), 2008). The special cases are checked by hand
//! against the RISC-V F chapter.
use std::cmp::Ordering::{self, Equal, Greater};
use riscv_rv32::softfloat::{self as sf, flag::*, Rm, CANONICAL_NAN};

const MODES: [Rm; 5] = [Rm::Rne, Rm::Rtz, Rm::Rdn, Rm::Rup, Rm::Rmm];
const CASES: usize = 40_000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
    fn below(&mut self, n: u64) -> u64 { self.next() % n }
    fn coin(&mut self) -> bool { self.next() & 1 == 1 }
}

/// Significand patterns that reach ties, carries out of the significand and exact results.
fn mantissa(r: &mut Rng) -> u32 {
    match r.below(6) {
        0 => 0,
        1 => 0x7f_ffff,
        2 => 1 << r.below(23),
        3 => 0x7f_ffff >> r.below(23),
        4 => (0x7f_ffff << r.below(23)) & 0x7f_ffff,
        _ => r.next() as u32 & 0x7f_ffff,
    }
}

/// A finite nonzero binary32 with a biased exponent within `spread` of `center`.
fn near(r: &mut Rng, center: i32, spread: i32) -> u32 {
    let e = (center + r.below(2 * spread as u64 + 1) as i32 - spread).clamp(0, 254) as u32;
    let m = mantissa(r);
    (r.coin() as u32) << 31 | e << 23 | if e == 0 && m == 0 { 1 } else { m }
}

/// A biased exponent, weighted towards the subnormal and overflow ends.
fn exponent(r: &mut Rng) -> i32 {
    match r.below(4) { 0 => r.below(12) as i32, 1 => 243 + r.below(12) as i32, _ => r.below(255) as i32 }
}

/// A finite nonzero binary32 anywhere in the range.
fn anywhere(r: &mut Rng) -> u32 { let e = exponent(r); near(r, e, 0) }

fn bexp(a: u32) -> i32 { ((a >> 23) & 0xff) as i32 }

/// 2^k exactly, for binary64's normal range.
fn p2(k: i32) -> f64 { f64::from_bits(((k + 1023) as u64) << 52) }

fn sign(v: f64) -> Ordering { v.partial_cmp(&0.0).unwrap() }

fn two_sum(a: f64, b: f64) -> (f64, f64) { let s = a + b; let bb = s - a; (s, (a - (s - bb)) + (b - bb)) }

/// `x` rounded to odd at 53 bits, from its binary64 rounding `s` and the sign of `x - s`.
fn odd(s: f64, x_minus_s: Ordering) -> f64 {
    if x_minus_s == Equal || s.to_bits() & 1 == 1 { s } else if x_minus_s == Greater { s.next_up() } else { s.next_down() }
}

/// Round the finite nonzero `v` to binary32 in `rm`: the bits and NX/UF/OF, with tininess
/// detected after rounding.
fn round_to_f32(v: f64, rm: Rm) -> (u32, u32) {
    let neg = v < 0.0;
    let a = v.abs();
    let sign_bit = (neg as u32) << 31;
    // the integer `n ≥ 0` rounds to, and whether it was inexact
    let pick = |n: f64| -> (f64, bool) {
        let lo = n.floor();
        let frac = n - lo;
        if frac == 0.0 { return (lo, false); }
        let up = match rm {
            Rm::Rne => frac > 0.5 || (frac == 0.5 && lo % 2.0 == 1.0),
            Rm::Rmm => frac >= 0.5,
            Rm::Rtz => false,
            Rm::Rdn => neg,
            Rm::Rup => !neg,
        };
        (if up { lo + 1.0 } else { lo }, true)
    };
    let e = ((a.to_bits() >> 52) as i32) - 1023;
    let (n, unbounded_inexact) = pick(a * p2(23 - e));
    let unbounded = n * p2(e - 23);
    if unbounded >= p2(128) {
        let to_inf = match rm { Rm::Rne | Rm::Rmm => true, Rm::Rtz => false, Rm::Rdn => neg, Rm::Rup => !neg };
        return (sign_bit | if to_inf { 0x7f80_0000 } else { 0x7f7f_ffff }, OF | NX);
    }
    let (value, inexact) = if e >= -126 { (unbounded, unbounded_inexact) } else { let (k, x) = pick(a * p2(149)); (k * p2(-149), x) };
    let mut flags = if inexact { NX } else { 0 };
    if inexact && unbounded < p2(-126) { flags |= UF; }
    let bits = sign_bit | (value as f32).to_bits();
    if rm == Rm::Rne { assert_eq!(bits, (v as f32).to_bits(), "the reference itself disagrees with the host for {v:e}"); }
    (bits, flags)
}

fn f64_of(a: u32) -> f64 { f32::from_bits(a) as f64 }

/// The reference result of `op` on finite nonzero operands, or `None` for an exact zero sum
/// (whose sign the hand cases check).
fn reference(op: &str, a: u32, b: u32, c: u32, rm: Rm) -> Option<(u32, u32)> {
    let (x, y, z) = (f64_of(a), f64_of(b), f64_of(c));
    let v = match op {
        "add" | "sub" => {
            let (s, e) = two_sum(x, if op == "sub" { -y } else { y });
            if s == 0.0 { return None; }
            odd(s, sign(e))
        }
        "mul" => x * y,
        "fma" => {
            let (s, e) = two_sum(x * y, z);
            if s == 0.0 { return None; }
            odd(s, sign(e))
        }
        "div" => {
            let s = x / y;
            let r = (-s).mul_add(y, x);
            odd(s, if y > 0.0 { sign(r) } else { sign(r).reverse() })
        }
        "sqrt" => { let s = x.sqrt(); odd(s, sign((-s).mul_add(s, x))) }
        _ => unreachable!(),
    };
    Some(round_to_f32(v, rm))
}

fn ours(op: &str, a: u32, b: u32, c: u32, rm: Rm) -> (u32, u32) {
    match op {
        "add" => sf::add(a, b, rm), "sub" => sf::sub(a, b, rm), "mul" => sf::mul(a, b, rm),
        "div" => sf::div(a, b, rm), "sqrt" => sf::sqrt(a & 0x7fff_ffff, rm), "fma" => sf::fma(a, b, c, rm),
        _ => unreachable!(),
    }
}

/// Operands for `op`, finite and nonzero, correlated so results cancel, tie, underflow and overflow.
fn operands(r: &mut Rng, op: &str) -> (u32, u32, u32) {
    let a = anywhere(r);
    let b = match (op, r.below(3)) {
        ("add" | "sub", 0 | 1) => near(r, bexp(a), 30),
        ("mul" | "fma", 0) => near(r, 127 + 127 - bexp(a), 30),      // product near 1
        ("mul" | "fma", 1) => near(r, 127 - 126 - bexp(a) + 127, 25), // product near the subnormals
        ("div", 0) => near(r, bexp(a), 30),
        ("div", 1) => near(r, bexp(a) + 120, 15),                     // quotient near the subnormals
        _ => anywhere(r),
    };
    let pe = bexp(a) + bexp(b) - 127;
    let c = match r.below(4) {
        0 => near(r, pe, 5),
        1 => near(r, pe, 40),
        2 => { let far = pe + if r.coin() { 70 } else { -70 }; near(r, far, 10) }   // far apart: the sticky path
        _ => anywhere(r),
    };
    (a, b, c)
}

#[test]
fn every_rounding_mode_matches_the_round_to_odd_reference() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    for op in ["add", "sub", "mul", "div", "sqrt", "fma"] {
        let mut checked = 0;
        for _ in 0..CASES {
            let (a, b, c) = operands(&mut r, op);
            let a = if op == "sqrt" { a & 0x7fff_ffff } else { a };
            for rm in MODES {
                let Some(want) = reference(op, a, b, c, rm) else { continue };
                assert_eq!(ours(op, a, b, c, rm), want, "{op} {a:08x} {b:08x} {c:08x} {rm:?}: (bits, flags) differ from the reference");
                checked += 1;
            }
        }
        assert!(checked > CASES * 4, "{op}: only {checked} cases reached the reference");
    }
}

#[test]
fn round_to_nearest_even_matches_the_host_on_any_bits() {
    let mut r = Rng(0x2545_f491_4f6c_dd1d);
    // NaN is tested on the bits, so the oracle does not depend on how the optimiser treats is_nan() after sqrt.
    let host = |v: f32| if v.to_bits() & 0x7fff_ffff > 0x7f80_0000 { CANONICAL_NAN } else { v.to_bits() };
    for _ in 0..CASES * 4 {
        let any = |r: &mut Rng| match r.below(3) { 0 => r.next() as u32, 1 => anywhere(r), _ => [0, 0x8000_0000, 0x7f80_0000, 0xff80_0000, 0x7fc0_0001, 0x7f80_0001, 1, 0x7f7f_ffff, 0x0080_0000][r.below(9) as usize] };
        let (a, b, c) = (any(&mut r), any(&mut r), any(&mut r));
        let (x, y, z) = (f32::from_bits(a), f32::from_bits(b), f32::from_bits(c));
        let rne = Rm::Rne;
        assert_eq!(sf::add(a, b, rne).0, host(x + y), "add {a:08x} {b:08x}");
        assert_eq!(sf::sub(a, b, rne).0, host(x - y), "sub {a:08x} {b:08x}");
        assert_eq!(sf::mul(a, b, rne).0, host(x * y), "mul {a:08x} {b:08x}");
        assert_eq!(sf::div(a, b, rne).0, host(x / y), "div {a:08x} {b:08x}");
        assert_eq!(sf::sqrt(a, rne).0, host(x.sqrt()), "sqrt {a:08x}");
        assert_eq!(sf::fma(a, b, c, rne).0, host(x.mul_add(y, z)), "fma {a:08x} {b:08x} {c:08x}");
        assert_eq!(sf::from_i32(a, rne).0, (a as i32 as f32).to_bits(), "fcvt.s.w {a:08x}");
        assert_eq!(sf::from_u32(a, rne).0, (a as f32).to_bits(), "fcvt.s.wu {a:08x}");
        if !x.is_nan() {
            assert_eq!(sf::to_i32(a, Rm::Rtz).0, x as i32 as u32, "fcvt.w.s rtz {a:08x}");
            assert_eq!(sf::to_u32(a, Rm::Rtz).0, x as u32, "fcvt.wu.s rtz {a:08x}");
        }
    }
}

/// Integer conversions in every mode against binary64's exact roundings; out of range (after
/// rounding) clips and raises NV only, otherwise NX when inexact.
#[test]
fn integer_conversions_round_and_clip_in_every_mode() {
    let mut r = Rng(0x0123_4567_89ab_cdef);
    for _ in 0..CASES {
        let a = match r.below(3) { 0 => { let e = 127 + r.below(34) as i32; near(&mut r, e, 0) }, 1 => near(&mut r, 126, 1), _ => anywhere(&mut r) };
        let v = r.next() as u32;
        let x = f64_of(a);
        for rm in MODES {
            let rounded = match rm { Rm::Rne => x.round_ties_even(), Rm::Rtz => x.trunc(), Rm::Rdn => x.floor(), Rm::Rup => x.ceil(), Rm::Rmm => x.round() };
            let nx = if rounded != x { NX } else { 0 };
            let want_w = if rounded < -p2(31) { (0x8000_0000, NV) } else if rounded > p2(31) - 1.0 { (0x7fff_ffff, NV) } else { (rounded as i32 as u32, nx) };
            let want_wu = if rounded < 0.0 { (0, NV) } else if rounded > p2(32) - 1.0 { (u32::MAX, NV) } else { (rounded as u32, nx) };
            assert_eq!(sf::to_i32(a, rm), want_w, "fcvt.w.s {a:08x} {rm:?}");
            assert_eq!(sf::to_u32(a, rm), want_wu, "fcvt.wu.s {a:08x} {rm:?}");
            let iv = v as i32;
            assert_eq!(sf::from_i32(v, rm), if iv == 0 { (0, 0) } else { round_to_f32(iv as f64, rm) }, "fcvt.s.w {v:08x} {rm:?}");
            assert_eq!(sf::from_u32(v, rm), if v == 0 { (0, 0) } else { round_to_f32(v as f64, rm) }, "fcvt.s.wu {v:08x} {rm:?}");
        }
    }
}

const QNAN: u32 = 0x7fc0_1234;
const SNAN: u32 = 0x7f80_0001;
const INF: u32 = 0x7f80_0000;
const NINF: u32 = 0xff80_0000;
const ONE: u32 = 0x3f80_0000;
const NZERO: u32 = 0x8000_0000;
const MAX: u32 = 0x7f7f_ffff;
const MIN_NORMAL: u32 = 0x0080_0000;

/// F, "NaN Generation and Propagation" and the invalid/divide-by-zero cases of IEEE 754-2008 7.2-7.3.
#[test]
fn special_operands_follow_the_f_chapter() {
    let rne = Rm::Rne;
    // NaNs: canonical result; NV only for a signaling input
    assert_eq!(sf::add(QNAN, ONE, rne), (CANONICAL_NAN, 0));
    assert_eq!(sf::mul(ONE, SNAN, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::sqrt(SNAN | 0x8000_0000, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::fma(QNAN, ONE, SNAN, rne), (CANONICAL_NAN, NV));
    // invalid operations
    assert_eq!(sf::add(INF, NINF, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::sub(INF, INF, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::mul(0, NINF, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::div(0, NZERO, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::div(INF, NINF, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::sqrt(0xbf80_0000, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::sqrt(NINF, rne), (CANONICAL_NAN, NV));
    // ∞ × 0 + qNaN is invalid in RISC-V, which IEEE leaves optional
    assert_eq!(sf::fma(INF, 0, QNAN, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::fma(INF, ONE, NINF, rne), (CANONICAL_NAN, NV));
    assert_eq!(sf::fma(ONE, ONE, QNAN, rne), (CANONICAL_NAN, 0));
    // divide by zero, and exact infinities and zeros
    assert_eq!(sf::div(0xbf80_0000, 0, rne), (NINF, DZ));
    assert_eq!(sf::div(ONE, NZERO, rne), (NINF, DZ));
    assert_eq!(sf::div(ONE, INF, rne), (0, 0));
    assert_eq!(sf::div(INF, 0, rne), (INF, 0));
    assert_eq!(sf::sqrt(NZERO, rne), (NZERO, 0));
    assert_eq!(sf::sqrt(INF, rne), (INF, 0));
    assert_eq!(sf::fma(INF, ONE, INF, rne), (INF, 0));
    assert_eq!(sf::fma(ONE, ONE, NINF, rne), (NINF, 0));
    assert_eq!(sf::mul(NZERO, 0x4000_0000, rne), (NZERO, 0));
}

/// An exact zero sum is +0 unless rounding down, and keeps the sign the operands share
/// (IEEE 754-2008 6.3); fnmsub/fnmadd negate the product, so they follow the same rule.
#[test]
fn exact_zero_sums_take_the_sign_ieee_gives_them() {
    for rm in MODES {
        let opposite = if rm == Rm::Rdn { NZERO } else { 0 };
        assert_eq!(sf::add(ONE, ONE | 0x8000_0000, rm), (opposite, 0), "{rm:?}");
        assert_eq!(sf::sub(ONE, ONE, rm), (opposite, 0), "{rm:?}");
        assert_eq!(sf::add(0, NZERO, rm), (opposite, 0), "{rm:?}");
        assert_eq!(sf::add(NZERO, NZERO, rm), (NZERO, 0), "{rm:?}");
        assert_eq!(sf::add(0, 0, rm), (0, 0), "{rm:?}");
        assert_eq!(sf::fma(ONE, ONE, ONE | 0x8000_0000, rm), (opposite, 0), "{rm:?}");
        assert_eq!(sf::fma(NZERO, ONE, 0, rm), (opposite, 0), "{rm:?}");
        assert_eq!(sf::fma(NZERO, ONE, NZERO, rm), (NZERO, 0), "{rm:?}");
        assert_eq!(sf::fma(0, ONE, ONE, rm), (ONE, 0), "{rm:?}");
    }
}

/// Overflow and underflow at the edges, tininess after rounding.
#[test]
fn overflow_and_underflow_edges() {
    // MAX + half an ulp: a tie that rounds to 2^128 in RNE and RMM, stays MAX toward zero
    let half_ulp = 0x7300_0000; // 2^103
    assert_eq!(sf::add(MAX, half_ulp, Rm::Rne), (INF, OF | NX));
    assert_eq!(sf::add(MAX, half_ulp, Rm::Rmm), (INF, OF | NX));
    assert_eq!(sf::add(MAX, half_ulp, Rm::Rtz), (MAX, NX));
    assert_eq!(sf::add(MAX, half_ulp, Rm::Rdn), (MAX, NX));
    assert_eq!(sf::add(MAX, half_ulp, Rm::Rup), (INF, OF | NX));
    assert_eq!(sf::mul(MAX, 0x4000_0000, Rm::Rtz), (MAX, OF | NX));
    assert_eq!(sf::mul(MAX | 0x8000_0000, 0x4000_0000, Rm::Rup), (MAX | 0x8000_0000, OF | NX));
    // 2^-126 × (1 - 2^-24) is tiny after rounding to 24 bits, though it rounds to the smallest normal
    assert_eq!(sf::mul(MIN_NORMAL, 0x3f7f_ffff, Rm::Rne), (MIN_NORMAL, UF | NX));
    // a subnormal result that is exact raises nothing
    assert_eq!(sf::mul(MIN_NORMAL, 0x3f00_0000, Rm::Rne), (0x0040_0000, 0));
    // the smallest subnormal halved: a tie to even (0), away (min subnormal), toward zero
    assert_eq!(sf::mul(1, 0x3f00_0000, Rm::Rne), (0, UF | NX));
    assert_eq!(sf::mul(1, 0x3f00_0000, Rm::Rmm), (1, UF | NX));
    assert_eq!(sf::mul(1 | 0x8000_0000, 0x3f00_0000, Rm::Rdn), (1 | 0x8000_0000, UF | NX));
    assert_eq!(sf::mul(1 | 0x8000_0000, 0x3f00_0000, Rm::Rup), (NZERO, UF | NX));
}

#[test]
fn comparisons_min_max_and_classify() {
    // FEQ is quiet; FLT and FLE signal on any NaN; all three are 0 with a NaN
    assert_eq!(sf::eq(QNAN, ONE), (false, 0));
    assert_eq!(sf::eq(SNAN, ONE), (false, NV));
    assert_eq!(sf::lt(QNAN, ONE), (false, NV));
    assert_eq!(sf::le(ONE, QNAN), (false, NV));
    assert_eq!(sf::eq(0, NZERO), (true, 0));
    assert_eq!(sf::lt(NZERO, 0), (false, 0));
    assert_eq!(sf::le(0, NZERO), (true, 0));
    assert_eq!(sf::lt(0xbf80_0000, NZERO), (true, 0));
    assert_eq!(sf::lt(0xc000_0000, 0xbf80_0000), (true, 0));
    // minimumNumber/maximumNumber: -0 < +0, one NaN gives the other operand, two give the canonical NaN
    assert_eq!(sf::min(0, NZERO), (NZERO, 0));
    assert_eq!(sf::max(NZERO, 0), (0, 0));
    assert_eq!(sf::min(QNAN, ONE), (ONE, 0));
    assert_eq!(sf::max(ONE, SNAN), (ONE, NV));
    assert_eq!(sf::min(SNAN, QNAN), (CANONICAL_NAN, NV));
    assert_eq!(sf::max(QNAN, QNAN), (CANONICAL_NAN, 0));
    assert_eq!(sf::min(0xc000_0000, ONE), (0xc000_0000, 0));
    assert_eq!(sf::max(0xc000_0000, 0xbf80_0000), (0xbf80_0000, 0));
    let classes = [(NINF, 0), (0xbf80_0000, 1), (0x8000_0001, 2), (NZERO, 3), (0, 4), (0x007f_ffff, 5), (ONE, 6), (INF, 7), (SNAN, 8), (QNAN, 9)];
    for (v, bit) in classes { assert_eq!(sf::classify(v), 1 << bit, "{v:08x}"); }
}

/// F, "Single-Precision Floating-Point Conversion and Move Instructions": the table of invalid inputs.
#[test]
fn conversions_saturate_as_the_table_says() {
    let rne = Rm::Rne;
    assert_eq!(sf::to_i32(QNAN, rne), (0x7fff_ffff, NV));
    assert_eq!(sf::to_i32(INF, rne), (0x7fff_ffff, NV));
    assert_eq!(sf::to_i32(NINF, rne), (0x8000_0000, NV));
    assert_eq!(sf::to_u32(QNAN, rne), (u32::MAX, NV));
    assert_eq!(sf::to_u32(INF, rne), (u32::MAX, NV));
    assert_eq!(sf::to_u32(NINF, rne), (0, NV));
    assert_eq!(sf::to_i32(0xcf00_0000, rne), (0x8000_0000, 0));          // -2^31 is in range
    assert_eq!(sf::to_i32(0x4f00_0000, rne), (0x7fff_ffff, NV));         // 2^31 is not
    assert_eq!(sf::to_u32(0x4f80_0000, rne), (u32::MAX, NV));            // 2^32
    assert_eq!(sf::to_u32(0x4f7f_ffff, rne), (0xffff_ff00, 0));
    // negative inputs to fcvt.wu.s: valid when they round to 0, otherwise clipped and invalid
    assert_eq!(sf::to_u32(0xbe99_999a, Rm::Rtz), (0, NX));               // -0.3
    assert_eq!(sf::to_u32(0xbf00_0000, rne), (0, NX));                   // -0.5 to even = -0
    assert_eq!(sf::to_u32(0xbf33_3333, rne), (0, NV));                   // -0.7 rounds to -1
    assert_eq!(sf::to_u32(0xbf00_0000, Rm::Rdn), (0, NV));
    assert_eq!(sf::to_u32(NZERO, rne), (0, 0));
    assert_eq!(sf::to_i32(0x3fc0_0000, Rm::Rmm), (2, NX));               // 1.5 away from zero
    assert_eq!(sf::to_i32(0x4020_0000, rne), (2, NX));                   // 2.5 to even
    assert_eq!(sf::from_i32(0x8000_0000, rne), (0xcf00_0000, 0));
    assert_eq!(sf::from_u32(u32::MAX, Rm::Rtz), (0x4f7f_ffff, NX));
    assert_eq!(sf::from_u32(u32::MAX, rne), (0x4f80_0000, NX));
}
