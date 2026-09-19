#[test]
fn zero_overhead_loop_does_not_pay_a_taken_branch_penalty() {
    let mut machine = esp32s3::machine([0; 6]);
    let pc = 0x4038_0000;
    esp_soc::SocBus::load_bytes(&mut machine.bus, pc, &[0x3d, 0xf0, 0x3d, 0xf0]).unwrap();
    machine.cores[0].pc = pc;
    machine.cores[0].lbeg = pc;
    machine.cores[0].lend = pc + 2;
    machine.cores[0].lcount = 1;
    let model = esp32s3::ApproximateCostModel::default();
    machine.set_cost_model(Box::new(model.clone())).unwrap();
    assert!(matches!(machine.run(2), esp_soc::Stop::MaxInsns));
    assert_eq!(machine.cores[0].pc, pc + 2);
    assert_eq!(model.stats().cycles[0], 2);
    assert_eq!(model.stats().zero_overhead_loop_edges, 1);
}

/// A taken `J` costs CPI plus two priced cycles. Instruction budgets count retired
/// instructions, never cycles, in both approximate scheduling modes.
#[test]
fn both_approximate_schedulers_drain_control_prices_in_cycles() {
    for blocks in [true, false] {
        for frontiers in [false, true] {
            for cpi in [1u32, 2] {
                let mut machine = esp32s3::machine([0; 6]);
                let pc = 0x4038_0000;
                esp_soc::SocBus::load_bytes(&mut machine.bus, pc, &[0x06, 0xff, 0xff]).unwrap(); // j .
                machine.cores[0].pc = pc;
                machine.set_approximate_jit_timing(cpi, 64).unwrap();
                machine.set_approximate_jit_frontiers(frontiers).unwrap();
                if !blocks { machine.add_observer(Box::new(esp_soc::observers::Breakpoints { pcs: vec![1] })); }
                for core in &mut machine.cores { xtensa_lx7::Core::set_jit(core, false); core.price_control = true; }
                assert!(matches!(machine.run(640), esp_soc::Stop::MaxInsns));
                let retired = machine.cores[0].insn_count;
                let cycles = esp_soc::SocBus::cycles(&machine.bus);
                let label = format!("blocks={blocks} frontiers={frontiers} cpi={cpi}: retired={retired} cycles={cycles}");
                assert_eq!(machine.cores[0].timing_extra, 0, "extras drained: {label}");
                assert_eq!(machine.run_steps(), retired, "run steps count instructions: {label}");
                let per_insn = cpi + 2;
                assert_eq!(machine.cores[0].ccount as u64, retired * u64::from(per_insn), "core clock: {label}");
                assert!((640..=640 + 64).contains(&retired), "budget counts instructions: {label}");
                assert!(cycles.abs_diff(retired * u64::from(per_insn)) <= u64::from(cpi + 2) * 64, "machine clock: {label}");
            }
        }
    }
}

#[test]
#[should_panic(expected = "scheduling quantum must be nonzero")]
fn zero_quantum_is_rejected() {
    let mut machine = esp32s3::machine([0; 6]);
    machine.quantum = 0;
    machine.run(1);
}

#[test]
fn frontier_reset_preserves_prior_control_cycles_without_counting_them_as_instructions() {
    for blocks in [true, false] {
        let mut machine = esp32s3::machine([0; 6]);
        let pc = 0x4038_0000;
        // j +0 reaches pc+4; s32i a2,a3,0 requests a chip reset.
        esp_soc::SocBus::load_bytes(&mut machine.bus, pc, &[0x06, 0x00, 0x00, 0x00, 0x22, 0x63, 0x00]).unwrap();
        machine.cores[0].pc = pc;
        machine.cores[0].set_ar(2, 1 << 31);
        machine.cores[0].set_ar(3, 0x6000_8000);
        machine.set_approximate_jit_timing(3, 64).unwrap();
        machine.set_approximate_jit_frontiers(true).unwrap();
        if !blocks { machine.add_observer(Box::new(esp_soc::observers::Breakpoints { pcs: vec![1] })); }
        for core in &mut machine.cores { xtensa_lx7::Core::set_jit(core, false); core.price_control = true; }
        assert!(matches!(machine.run(64), esp_soc::Stop::SwReset));
        assert_eq!(machine.run_steps(), 2);
        assert_eq!(machine.cores[0].insn_count, 2);
        assert_eq!(machine.cores[0].ccount, 8);
        assert_eq!(esp_soc::SocBus::cycles(&machine.bus), 8);
        assert_eq!(machine.cores[0].timing_extra, 0);
    }
}
