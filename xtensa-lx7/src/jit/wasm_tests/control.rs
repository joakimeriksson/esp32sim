use super::*;

pub(super) fn special_register_blocks() -> u32 {
    use crate::state::sr;
    let mut tests = 0;
    let exact = [sr::PS, sr::PRID, sr::SCOMPARE1, sr::INTENABLE, sr::VECBASE,
        sr::CPENABLE, sr::EXCCAUSE, sr::EXCVADDR, sr::DEPC];
    for number in exact.into_iter().chain(177..=183).chain(194..=199).chain(209..=215).chain(244..=247) {
        let mut block = [insn(Op::Add), insn(Op::Rsr), insn(Op::Xor)];
        block[1].insn.imm = number as i32;
        let mut cc = CodeCache::new(0).unwrap();
        assert!(compile(&mut cc, &mut block, BASE, false).is_some());
        for entry in 0..3 {
            for budget in 1..=3 {
                compare(&mut block, Case { entry, budget, ..Case::default() }, |c| {
                        c.write_sr(number, 0xab00_0000 | number).unwrap();
                        c.prid = 0x1234_5678;
                    });
                tests += 1;
            }
        }
    }
    // Time-accounted registers must still start their own interpreter block.
    for number in [sr::CCOUNT, sr::INTERRUPT, sr::ICOUNT, 240, 241, 242] {
        let mut block = [insn(Op::Add), insn(Op::Rsr)];
        block[1].insn.imm = number as i32;
        let mut cc = CodeCache::new(0).unwrap();
        assert!(compile(&mut cc, &mut block, BASE, false).is_none());
        assert!(crate::block::must_start_block(&block[1].insn));
        tests += 1;
    }
    for op in [Op::Wsr, Op::Xsr] {
        for number in [sr::PS, sr::INTENABLE, sr::WINDOWBASE, sr::WINDOWSTART,
            sr::LBEG, sr::LEND, sr::LCOUNT, sr::SAR, sr::CPENABLE, sr::VECBASE,
            sr::SCOMPARE1, 177, 194, 209, 244, 255] {
            let mut block = [insn(Op::Add), insn(Op::MovN), insn(op)];
            block[2].insn.imm = number as i32;
            for wb in [0, 15] {
                for entry in 0..3 {
                    for budget in [1, 3, 12] {
                        compare(&mut block, Case { seed: wb, entry, budget, loop_end: true, ..Case::default() }, |c| {
                                let value = if number == sr::LEND { BASE + 9 } else { 9 };
                                c.set_ar(4, value);
                                c.set_ar(5, value);
                            });
                        tests += 1;
                    }
                }
            }
        }
    }
    for op in [Op::Rsil, Op::Rsync, Op::Esync, Op::Dsync] {
        let mut block = [insn(Op::Add), insn(Op::MovN), insn(op)];
        for entry in 0..3 {
            for budget in 1..=3 {
                compare(&mut block, Case { seed: 15, entry, budget, loop_end: true, ..Case::default() }, |_| {});
                tests += 1;
            }
        }
    }
    tests
}

pub(super) fn terminal_helpers() -> u32 {
    use Op::*;
    let mut tests = 0;
    for op in [Call0, Call4, Call8, Call12, Callx0, Callx4, Callx8, Callx12, Ret, RetN, Retw, RetwN] {
        let mut block = [insn(Add), insn(MovN), insn(op)];
        if matches!(op, Callx0 | Callx4 | Callx8 | Callx12) {
            block[2].insn.s = match op { Callx0 => 0, Callx4 => 4, Callx8 => 8, _ => 12 };
            block[2].max_ar = crate::exec::max_ar(&block[2].insn);
        }
        // Dirty the implicit return register: the helper must see its spilled value,
        // and helper writes/window rotations must not be overwritten after it returns.
        block[1].insn.t = 0;
        block[1].max_ar = crate::exec::max_ar(&block[1].insn);
        let mut cc = CodeCache::new(0).unwrap();
        assert!(compile(&mut cc, &mut block, BASE, false).is_some());
        assert!(compile(&mut cc, &mut [insn(op), insn(Add)], BASE, false).is_none());
        assert!(compile(&mut cc, &mut [insn(op)], BASE, false).is_none());
        for wb in [0, 7, 15] {
            for flags in [0, ps::WOE, ps::WOE | ps::EXCM] {
                for windows in [0, 1 << 2, 0xffff] {
                    for inc in 0..4 {
                        for entry in 0..3 {
                            for budget in 1..=3 {
                                compare(&mut block, Case { seed: wb, entry, budget, ..Case::default() }, |c| {
                                        c.ps = flags;
                                        c.windowbase = wb;
                                        c.windowstart = windows;
                                        let ret = (inc << 30) | ((BASE + 0x400) & 0x3fff_ffff);
                                        c.set_ar(0, ret);
                                        c.set_ar(4, ret);
                                    });
                                tests += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    tests
}

pub(super) fn whole_block_guards() -> u32 {
    let mut tests = 0;
    for offset in [-3i32, 0, 1, 3, 4, 6, 9, 10, 12] {
        for count in [0, 1, 0xffff_ffff] {
            for flags in [0, ps::WOE, ps::WOE | ps::EXCM] {
                for windows in [0, 0xffff] {
                    for entry in 0..3 {
                        for budget in 1..=3 {
                            compare(&mut [insn(Op::Add), insn(Op::MovN), insn(Op::Xor)], Case { seed: 15, entry, budget, ..Case::default() }, |c| {
                                    c.lend = BASE.wrapping_add(offset as u32);
                                    c.lbeg = BASE + 0x100;
                                    c.lcount = count;
                                    c.ps = flags;
                                    c.windowstart = windows;
                                });
                            tests += 1;
                        }
                    }
                }
            }
        }
    }
    tests
}

pub(super) fn window_masks() -> u32 {
    let mut cc = CodeCache::new(0).unwrap();
    let mut cases = 0;
    for high in [3, 7, 11, 15] {
        let mut low = insn(Op::Movi);
        low.insn.t = 1;
        low.max_ar = 1;
        let mut upper = insn(Op::Add);
        upper.insn.r = high;
        upper.insn.s = 2;
        upper.insn.t = 3;
        upper.max_ar = crate::exec::max_ar(&upper.insn);
        let mut block = [low, upper];
        let id = queue(&mut cc, &mut block, BASE, false);
        for _ in 0..HOT {
            ready(&cc, id, 0);
        }
        for wb in 0..16 {
            for frame in 1..=3 {
                for status in [0, ps::WOE, ps::WOE | ps::EXCM] {
                    for entry in 0..2 {
                        for budget in 1..=2 {
                            let (mut a, mut b) = (cpu(wb), cpu(wb));
                            for c in [&mut a, &mut b] {
                                c.pc = BASE + entry * 3;
                                c.ps = status;
                                c.windowstart = 1 << ((wb + frame) & 15);
                            }
                            let (mut ra, mut rb) = (Ram::new(false, false), Ram::new(false, false));
                            let actual = unsafe {
                                run(
                                    &cc,
                                    id,
                                    &mut b,
                                    &mut rb,
                                    &Helpers::new::<Ram>(),
                                    budget,
                                    entry,
                                    None,
                                )
                            };
                            let (mut done, mut trap) = (0, None);
                            for bi in block.iter().skip(entry as usize).take(budget as usize) {
                                if let Some(t) = a.check_overflow(bi.max_ar) {
                                    trap = Some(t);
                                    break;
                                }
                                exec_insn(&mut a, &mut ra, &bi.insn).unwrap();
                                done += 1;
                            }
                            assert_eq!(actual & 0xffff, done);
                            assert_eq!(trap, b.jit_trap.take());
                            same(&a, &b);
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    cases
}

pub(super) fn entry_and_shifts() -> u32 {
    use Op::*;
    let mut tests = 0;
    for op in [Sll, Srl] {
        for sar in (0..=64).chain([127, u32::MAX]) {
            for value in [0, 1, 0x8000_0000, 0xffff_ffff, 0xa5a5_5a5a] {
                for alias in [false, true] {
                    let mut shift = insn(op);
                    if alias { shift.insn.r = if op == Sll { 4 } else { 5 }; }
                    shift.max_ar = crate::exec::max_ar(&shift.insn);
                    let mut block = [insn(Nop), shift, insn(Xor)];
                    let mut cc = CodeCache::new(0).unwrap();
                    assert!(compile(&mut cc, &mut block, BASE, false).is_some());
                    compare(&mut block, Case { seed: 15, budget: 3, ..Case::default() }, |c| {
                            c.sar = sar;
                            c.set_ar(4, value);
                            c.set_ar(5, value);
                        });
                    tests += 1;
                }
            }
        }
    }
    for wb in [0, 14, 15] {
        for flags in [0, ps::WOE, ps::WOE | ps::EXCM] {
            // The +4 frame is outside the initial a15 guard, but can collide
            // after ENTRY rotates. This catches reuse of the whole-block proof.
            for windows in [0, 1 << ((wb + 4) & 15), 0xffff] {
                for inc in 0..4 {
                    for s in [0, 1, 3, 4] {
                        let mut prefix = insn(Movi);
                        prefix.insn.t = 1;
                        prefix.insn.imm = -1;
                        let mut enter = insn(Entry);
                        enter.insn.s = s;
                        enter.insn.imm = 32;
                        let mut upper = insn(Add);
                        upper.insn.r = 15;
                        let mut block = [prefix, enter, upper, enter, insn(Xor)];
                        for bi in &mut block { bi.max_ar = crate::exec::max_ar(&bi.insn); }
                        let mut cc = CodeCache::new(0).unwrap();
                        assert!(compile(&mut cc, &mut block, BASE, false).is_some());
                        for entry in 0..5 {
                            for budget in 0..=5 {
                                compare(&mut block, Case { seed: wb, entry, budget, ..Case::default() }, |c| {
                                        c.ps = flags | (inc << ps::CALLINC_SHIFT);
                                        c.windowstart = windows;
                                        // Alternate active loop ends directly after ENTRY.
                                        c.lcount = inc & 1;
                                        c.lend = BASE + 6;
                                        c.lbeg = BASE;
                                    });
                                tests += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    tests
}

pub(super) fn helper_continuation() -> u32 {
    use Op::*;
    let mut tests = 0;
    for overflow in [false, true] {
        for entry in 0..3 {
            // Keep an unsupported opcode here to exercise helper continuation.
            compare(&mut [insn(Add), insn(Nsa), insn(Xor)],
                Case { seed: 15, entry, budget: 3, loop_end: true, overflow, ..Case::default() }, |_| {});
            tests += 1;
        }
    }
    tests
}

// Assert the generated path itself, so silently falling back cannot pass this oracle.
pub(super) fn guarded_loop_sites() -> u32 {
    let mut tests = 0;
    for site in [1, 2, 3] {
        for entry in 0..3 {
            for budget in [1, 2, 3, 8] {
                let mut block = [insn(Op::Add), insn(Op::Xor), insn(Op::Add)];
                let lend = BASE + site * 3;
                let configure = |c: &mut Cpu| { c.ps = 0; c.lend = lend; c.lbeg = BASE; c.lcount = 2; };
                let case = Case { entry, budget, ..Case::default() };
                assert!(compare_hinted(&mut block, case, &configure, lend), "guarded site={site} entry={entry} budget={budget}");
                assert!(!compare_hinted(&mut block, case, &configure, 0), "checked site={site} entry={entry} budget={budget}");
                tests += 2;
            }
        }
    }
    tests
}

pub(super) fn pie_wide_shifts() -> u32 {
    use crate::pie::Role::{Qa, Qs};
    let mut tests = 0;
    for name in ["ee.vsr.32", "ee.vsl.32"] {
        let bytes = asm::pie(name, &[(Qa, 1), (Qs, 0)]);
        let raw = bytes[0] as u32 | ((bytes[1] as u32) << 8) | ((bytes[2] as u32) << 16);
        let mut shift = insn(Op::Pie);
        shift.insn = crate::decode::decode(BASE + 3, raw.to_le_bytes());
        for sar in 33..64 {
            let mut block = [insn(Op::Nop), shift, insn(Op::Xor)];
            for entry in 0..=1 {
                for budget in [1, 3] {
                    compare(&mut block, Case { entry, budget, ..Case::default() }, |c| {
                        c.ps = 0; c.cpenable = 8;
                        c.write_sr(crate::state::sr::SAR, sar).unwrap();
                        c.qr[0] = u128::MAX;
                    });
                    tests += 1;
                }
            }
        }
    }
    tests
}
