use super::*;

pub(super) fn floating_point_guard_proof() -> u32 {
    use Op::*;
    let mut tests = 0;
    // Ineligible helper-containing blocks still test the emitter's continuation
    // machinery. A helper may disable or enable CP before the scalar instruction.
    for op in [Wsr, Xsr] {
        for enabled in [0, 1] {
            for entry in 0..4 {
                for budget in [1, 2, 4] {
                    let mut block = [insn(Add), insn(op), insn(AddS), insn(Rfr)];
                    block[1].insn.imm = crate::state::sr::CPENABLE as i32;
                    compare(&mut block, Case { entry, budget, fast: true, ..Case::default() }, |c| {
                        c.cpenable = enabled; c.set_ar(5, enabled ^ 1);
                        c.fr[3] = 2f32.to_bits(); c.fr[4] = 3f32.to_bits(); c.fr[5] = 4f32.to_bits();
                    });
                    tests += 1;
                }
            }
        }
    }
    // Disabled CP must not prevent a prefix store or cause a trap when a budget
    // cut or taken branch exits before the FP instruction.
    for prefix in [S32i, Bt] {
        for enabled in [0, 1] {
            for entry in 0..3 {
                for budget in [1, 3] {
                    let mut block = [insn(prefix), insn(AddS), insn(Rfr)];
                    if prefix == Bt { block[0].insn.imm = (BASE + 48) as i32; }
                    compare(&mut block, Case { entry, budget, addr: Some(BASE + 512), fast: true, ..Case::default() }, |c| {
                        c.cpenable = enabled; c.br = 1 << 4;
                    });
                    tests += 1;
                }
            }
        }
    }
    tests
}

pub(super) fn floating_point() -> u32 {
    use Op::*;
    let ops = [AddS, SubS, MulS, MaddS, MsubS, MovS, AbsS, NegS, Rfr, Wfr, ConstS,
        FloatS, UfloatS, RoundS, TruncS, FloorS, CeilS, UtruncS, UnS, OeqS, UeqS, OltS,
        UltS, OleS, UleS, MoveqzS, MovnezS, MovltzS, MovgezS, MovfS, MovtS,
        MaddnS, DivnS, Div0S, Nexp01S, Recip0S, Rsqrt0S, Sqrt0S, AddexpS,
        MkdadjS, MksadjS, AddexpmS, Movf, Movt, Bf, Bt];
    let values = [0, 0x8000_0000, 1, 0x007f_ffff, 0x0080_0000, 0x3f80_0001,
        0xbf80_0000, 0x3fc0_0000, 0xc020_0000, 0x4eff_ffff, 0x4f00_0000,
        0x4f7f_ffff, 0x4f80_0000, 0xcf00_0000, 0x7f7f_ffff, 0x7f80_0000,
        0xff80_0000, 0x7fc1_2345, 0xffc5_4321, 0x7f81_2345];
    let mut tests = 0;
    for op in ops {
        assert!(super::emitter::supported(op, true));
        for (n, &bits) in values.iter().enumerate() {
            for entry in 0..3 {
                for budget in [1, 3] {
                    let mut block = [insn(Add), insn(op), insn(Xor)];
                    block[1].insn.imm = if matches!(op, Bf | Bt) { (BASE + 48) as i32 } else { (n % 16) as i32 };
                    compare(&mut block, Case { seed: n as u32, entry, budget, fast: true, loop_end: n % 3 == 0, overflow: n % 7 == 0, ..Case::default() }, |c| {
                        c.cpenable = if n % 9 == 0 { 0 } else { 1 };
                        c.br = if n % 2 == 0 { 0xaaaa } else { 0x5555 };
                        c.fr[3] = 0xbf80_0000;
                        c.fr[4] = bits;
                        c.fr[5] = values[(n + 5) % values.len()];
                    });
                    tests += 1;
                }
            }
        }
    }
    // A drawing-like bundle connects float, boolean and integer state through a
    // conversion/coverage decision, including partial execution and aliased operands.
    let raster_ops = [Wfr, FloatS, SubS, MulS, MaddS, OltS, MovtS, TruncS, Movf, Bt];
    for entry in 0..raster_ops.len() as u32 {
        for budget in [1, 4, 12] {
            let mut block: Vec<_> = raster_ops.into_iter().map(insn).collect();
            for bi in &mut block {
                if matches!(bi.insn.op, MovtS | Movf) { bi.insn.t = 3; }
                if bi.insn.op == Bt { bi.insn.s = 3; bi.insn.imm = (BASE + 48) as i32; }
                bi.max_ar = crate::exec::max_ar(&bi.insn);
            }
            compare(&mut block, Case { seed: 15, entry, budget, fast: true, ..Case::default() }, |c| {
                c.cpenable = 1; c.fr[3] = 0.5f32.to_bits();
                c.fr[4] = 2.25f32.to_bits(); c.fr[5] = 3.5f32.to_bits();
            });
            tests += 1;
        }
    }
    // Cancellation distinguishes one fused rounding from multiply followed by add.
    for op in [MaddS, MsubS] {
        compare(&mut [insn(op), insn(Rfr)], Case { budget: 2, fast: true, ..Case::default() }, |c| {
            c.cpenable = 1; c.fr[3] = (-1f32).to_bits();
            c.fr[4] = if op == MaddS { 0x3f80_0001 } else { 0xbf80_0001 };
            c.fr[5] = 0x3f7f_fffe;
        });
        tests += 1;
    }
    // Both native TLB accesses and helper paths must preserve raw FP bits and code versions.
    for op in [Lsi, Ssi] {
        for addr in [BASE + 512, BASE + 513, BASE + 65536] {
            for fast in [false, true] {
                for readonly in [false, true] {
                    for enabled in [0, 1] {
                        compare(&mut [insn(Add), insn(op), insn(Xor)], Case { seed: 1, budget: 3, addr: Some(addr), fast, readonly, ..Case::default() }, |c| {
                                c.cpenable = enabled; c.fr[5] = 0x7f81_2345;
                            });
                        tests += 1;
                    }
                }
            }
        }
    }
    tests
}
