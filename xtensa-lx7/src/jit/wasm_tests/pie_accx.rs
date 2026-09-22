//! Directed S8 reductions, checked against both the table executor and a scalar dot product.
use super::*;

struct Case {
    name: String,
    x: [i8; 16],
    y: [i8; 16],
    initial: i64,
}

pub(super) fn run_tests() -> u32 {
    use crate::pie::Role::{As, Imm, Qu, Qx, Qy};

    let mut cases = Vec::new();
    // Every lane must reach the reduction, including both widening-multiply halves.
    for lane in 0..16 {
        let mut x = [0; 16];
        let mut y = [0; 16];
        x[lane] = -128;
        y[lane] = if lane % 2 == 0 { -128 } else { 127 };
        cases.push(Case { name: format!("lane-{lane}"), x, y, initial: 0 });
    }
    let high = (1i64 << 39) - 1;
    let low = -(1i64 << 39);
    for (name, x, y, initial) in [
        ("maximum-dot", [-128; 16], [-128; 16], 0),
        ("negative-dot", [-128; 16], [127; 16], 0),
        ("saturate-high", [-128; 16], [-128; 16], high - 1),
        ("saturate-low", [-128; 16], [127; 16], low + 1),
        ("just-below-high", [-128; 16], [-128; 16], high - 262_145),
        ("just-above-low", [-128; 16], [127; 16], low + 260_097),
    ] {
        cases.push(Case { name: name.into(), x, y, initial });
    }

    let mut tests = 0;
    // The load form must multiply the old operands even when Qu replaces Qx or Qy.
    for load_q in [None, Some(0), Some(1), Some(2)] {
        let encoded = match load_q {
            None => asm::pie("ee.vmulas.s8.accx", &[(Qx, 0), (Qy, 1)]),
            Some(q) => asm::pie("ee.vmulas.s8.accx.ld.ip",
                &[(Qx, 0), (Qy, 1), (Qu, q), (As, 4), (Imm, 16)]),
        };
        let mut bytes = [0; 4];
        bytes[..encoded.len()].copy_from_slice(&encoded);
        let decoded = crate::decode::decode(BASE, bytes);
        assert_eq!(decoded.op, Op::Pie);
        let mut block = [BlockInsn {
            insn: decoded, max_ar: crate::exec::max_ar(&decoded), straddle: false, off: 0,
        }, insn(Op::Nop)];
        let mut cc = CodeCache::new(0).unwrap();
        let code = compile(&mut cc, &mut block, BASE, true).expect("S8 dot must compile");
        for _ in 0..HOT { ready(&cc, code, 0); }
        assert!(ready(&cc, code, 0));

        for case in &cases {
            // One instruction takes the checked body; two take the whole-block body.
            for budget in [1, 2] {
                let context = format!("S8 {} load_q={load_q:?} budget={budget}", case.name);
                CONTEXT.with(|c| *c.borrow_mut() = context.clone());
                let (mut reference, mut actual) = (cpu(0), cpu(0));
                for c in [&mut reference, &mut actual] {
                    c.cpenable = 1 << 3;
                    c.qr[0] = u128::from_le_bytes(case.x.map(|v| v as u8));
                    c.qr[1] = u128::from_le_bytes(case.y.map(|v| v as u8));
                    // The unused high bits must be ignored on read and cleared on write.
                    c.accx = [case.initial as u32, ((case.initial >> 32) & 0xff) as u32 | 0xa5a5_a500];
                    c.set_ar(4, BASE + 0x1000);
                }
                let (mut reference_ram, mut actual_ram) = (Ram::new(true, false), Ram::new(true, false));
                let fm = actual_ram.fast_mem();
                // SAFETY: code is ready, entry zero is valid and both helpers and mapping
                // belong to the exclusively borrowed test bus.
                let result = unsafe {
                    run(&cc, code, &mut actual, &mut actual_ram, &Helpers::new::<Ram>(), budget, 0, fm)
                };
                assert_eq!(result & 0xffff, budget, "{context}");
                assert!(actual.jit_trap.is_none(), "{context}");
                for bi in block.iter().take(budget as usize) {
                    let mut instruction = bi.insn;
                    if instruction.op == Op::Pie {
                        // Clear the packed marker: use the independent table executor.
                        instruction.r = 0;
                    }
                    exec_insn(&mut reference, &mut reference_ram, &instruction).unwrap();
                }
                same(&reference, &actual);

                let dot: i64 = case.x.iter().zip(&case.y)
                    .map(|(&x, &y)| i64::from(x) * i64::from(y)).sum();
                let expected = (case.initial + dot).clamp(low, high);
                assert_eq!(actual.accx, [expected as u32, ((expected >> 32) & 0xff) as u32], "{context}");
                assert_eq!(reference_ram.ram.mem, actual_ram.ram.mem, "{context}");
                tests += 1;
            }
        }
    }
    tests
}

fn decoded(name: &str, operands: &[(crate::pie::Role, i32)]) -> BlockInsn {
    let encoded = asm::pie(name, operands);
    let mut bytes = [0; 4];
    bytes[..encoded.len()].copy_from_slice(&encoded);
    let i = crate::decode::decode(BASE, bytes);
    BlockInsn { insn: i, max_ar: crate::exec::max_ar(&i), straddle: false, off: 0 }
}

/// Keep the backing allocation valid while limiting only the published fast mapping.
/// The slow bus can still serve every load, so these cases check fallback without an
/// out-of-allocation access even if a range check regresses.
fn whole_block(mut block: Vec<BlockInsn>, span: u32, setup: impl Fn(&mut Cpu)) -> (Cpu, u32) {
    let n = block.len() as u32;
    let mut cc = CodeCache::new(0).unwrap();
    let code = compile(&mut cc, &mut block, BASE, true).expect("PIE block must compile");
    for _ in 0..HOT { ready(&cc, code, 0); }
    assert!(ready(&cc, code, 0));
    let (mut reference, mut actual) = (cpu(0), cpu(0));
    for c in [&mut reference, &mut actual] {
        c.cpenable = 1 << 3;
        c.blocks.observed = true; // Exercise the emitted whole block, without region formation.
        setup(c);
    }
    let (mut ra, mut rb) = (Ram::new(true, false), Ram::new(true, false));
    rb.tlb[tlb_index(BASE)].hi = BASE + span;
    rb.tlb[tlb_index(BASE)].span = span;
    let mut retired = 0;
    while retired < n {
        let fm = rb.fast_mem();
        // SAFETY: this is a sequential instruction boundary in the ready block and the
        // helper table belongs to this live test bus. Memory helpers may return early.
        let result = unsafe { run(&cc, code, &mut actual, &mut rb, &Helpers::new::<Ram>(), n - retired, retired, fm) };
        let done = result & 0xffff;
        assert!(done > 0 && done <= n - retired);
        assert!(actual.jit_trap.is_none());
        retired += done;
    }
    for bi in &block {
        let mut i = bi.insn;
        i.r = 0; // Independent table executor, rather than packed PIE operands.
        exec_insn(&mut reference, &mut ra, &i).unwrap();
    }
    same(&reference, &actual);
    assert_eq!(ra.ram.mem, rb.ram.mem);
    (actual, rb.slow_reads)
}

pub(super) fn held_and_coalesced() -> u32 {
    use crate::pie::Role::{As, Imm, Qu, Qx, Qy};
    let mut cases = 0;
    for w in [8, 16] {
        for preceding in [10, 62, 63, 64] {
            for loads in [1, 2] {
                for span in [0, 65536] {
                    let mut block = vec![decoded("ee.zero.accx", &[])];
                    for _ in 0..preceding {
                        block.push(decoded(&format!("ee.vmulas.s{w}.accx"), &[(Qx, 0), (Qy, 1)]));
                    }
                    for _ in 0..loads {
                        block.push(decoded(&format!("ee.vmulas.s{w}.accx.ld.ip"),
                            &[(Qx, 0), (Qy, 1), (Qu, 2), (As, 4), (Imm, 16)]));
                    }
                    let (actual, reads) = whole_block(block, span, |c| {
                        c.accx = [0xaabb_ccdd, 0x77];
                        let lanes = if w == 8 { [100u8; 16] } else {
                            let mut lanes = [0; 16];
                            for lane in lanes.as_chunks_mut::<2>().0 { lane.copy_from_slice(&100i16.to_le_bytes()); }
                            lanes
                        };
                        c.qr[0] = u128::from_le_bytes(lanes);
                        c.qr[1] = c.qr[0];
                        c.set_ar(4, BASE + 0x1000);
                    });
                    let expected = (128 / w) * 10000 * (preceding + loads);
                    assert_eq!(actual.accx, [expected, 0]);
                    if span == 0 { assert!(reads > 0, "must execute the slow load path"); }
                    cases += 1;
                }
            }
        }
    }
    for loads in [2, 3, 4] {
        for inside in 1..loads {
            let mut block = vec![decoded("ee.zero.accx", &[])];
            for q in 0..loads {
                block.push(decoded("ee.vld.128.ip", &[(Qu, q), (As, 4), (Imm, 16)]));
            }
            let (_, reads) = whole_block(block, 0x1000 + inside as u32 * 16,
                |c| c.set_ar(4, BASE + 0x1000));
            assert!(reads > 0, "a run beyond the fast mapping must use the slow bus");
            cases += 1;
        }
    }
    cases
}
