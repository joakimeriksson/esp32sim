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
