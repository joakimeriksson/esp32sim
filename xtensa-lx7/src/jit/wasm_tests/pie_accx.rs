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
