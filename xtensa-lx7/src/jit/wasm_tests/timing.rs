use super::*;

pub(super) fn priced_cases() -> u32 {
    use Op::*;
    use std::sync::atomic::Ordering::Relaxed;
    assert!(!PRICED.swap(true, Relaxed));
    let mut cases = 0;
    // Shared-table goldens are needed in addition to backend parity: both backends
    // could otherwise agree on the same incorrect readiness calculation.
    let mut producer = insn(AddS); producer.insn.r = 0; producer.insn.s = 1; producer.insn.t = 2;
    let mut overwrite = insn(Wfr); overwrite.insn.r = 0;
    let mut reader = insn(Rfr); reader.insn.s = 0;
    for middle in [overwrite, insn(Quou)] {
        let block = [producer, middle, reader];
        assert_eq!(crate::exec::static_extras(block.iter().map(|b| &b.insn)), vec![0, 0, 0]);
        cases += 1;
    }
    let mut consumer = insn(AddS); consumer.insn.s = 0;
    assert_eq!(crate::exec::static_extras([producer, consumer].iter().map(|b| &b.insn)), vec![0, 3]);
    cases += 1;
    for op in [Quou, Quos, Remu, Rems] {
        for (left, right) in [(7, 2), (0x8000_0000, u32::MAX), (5, 0)] {
            for entry in 0..2 { for budget in 1..=2 {
                compare(&mut [insn(Nop), insn(op)], Case { entry, budget, ..Case::default() }, |c| { c.set_ar(4, left); c.set_ar(5, right); });
                cases += 1;
            } }
        }
    }
    for op in [Loop, Loopnez, Loopgtz, Beqz, Bnez, Bltz, Bgez] {
        for value in [0, 1, u32::MAX] { for loop_end in [false, true] {
            let mut transfer = insn(op); transfer.insn.imm = (BASE + 0x100) as i32;
            compare(&mut [insn(Nop), transfer], Case { budget: 2, loop_end, ..Case::default() }, |c| c.set_ar(4, value));
            cases += 1;
        } }
    }
    // An actual taken transfer can target the fall-through PC; pc!=next alone
    // must not determine whether it gets its branch/alignment price.
    for op in [J, Call0, Beqz] {
        let mut transfer = insn(op); transfer.insn.imm = (BASE + 6) as i32;
        compare(&mut [insn(Nop), transfer], Case { budget: 2, ..Case::default() }, |c| c.set_ar(4, 0));
        cases += 1;
    }
    // Inline JX/CALLX dynamic-target alignment is a documented model limitation.
    // Test their base prices with aligned targets rather than hiding a timing delta.
    for op in [Jx, Callx0, Callx4, Callx8, Callx12] {
        for flags in [0, ps::WOE] {
            compare(&mut [insn(Nop), insn(op)], Case { budget: 2, ..Case::default() }, |c| { c.ps = flags; c.set_ar(4, BASE + 0x100); });
            cases += 1;
        }
    }
    // A dependency wait must not survive a faulting helper when the interpreter
    // charges extras only for successful instructions.
    let mut load = insn(L32i); load.insn.t = 4; load.insn.imm = 0;
    load.max_ar = crate::exec::max_ar(&load.insn);
    compare(&mut [load, insn(FloatS)], Case { budget: 2, fast: true, ..Case::default() }, |c| { c.set_ar(4, BASE + 0x1000); c.cpenable = 0; });
    cases += 1;
    let mut store = insn(Ssi); store.insn.t = 0; store.insn.imm = 0;
    store.max_ar = crate::exec::max_ar(&store.insn);
    compare(&mut [producer, store], Case { budget: 2, fast: true, readonly: true, ..Case::default() }, |c| { c.set_ar(4, BASE + 0x1000); c.cpenable = 1; });
    cases += 1;
    cases += arithmetic::integer_ops() + float::floating_point() + float::floating_point_guard_proof()
        + control::entry_and_shifts() + control::terminal_helpers() + control::special_register_blocks();
    PRICED.store(false, Relaxed);
    cases
}
