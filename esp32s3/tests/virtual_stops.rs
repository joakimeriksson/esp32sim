use emu_core::{Bus, Core};
use esp_soc::Stop;

const IRAM: u32 = 0x4037_0000;

#[test]
fn peripheral_alarm_inside_a_batch_matches_single_round_scheduling() {
    const VECTOR: u32 = IRAM + 0x1000;
    // 16 MHz SYSTIMER deadlines straddle the 64-cycle scheduling boundary.
    for ticks in [9u32, 12, 13, 17] {
        for busy in 0..2 {
            let mut results = Vec::new();
            for vq in [1, 1024] {
                let mut m = esp32s3::machine([0; 6]);
                m.console.capture = true;
                m.vq_max = 1;
                for core in &mut m.cores { core.set_jit(false); }
                let spin = [0x06, 0xff, 0xff];
                m.bus.load_bytes(IRAM, &spin).unwrap();
                m.bus.load_bytes(0x4000_0400, &spin).unwrap();
                m.cores[0].pc = IRAM;
                m.cores[0].ps = 0;
                m.bus.write32(0x600c_0000, 2).unwrap();
                assert!(matches!(m.run(64), Stop::MaxInsns));
                m.cores[1 - busy].waiting = true;
                m.cores[busy].pc = IRAM;
                m.cores[busy].ps = 0;
                m.cores[busy].vecbase = VECTOR;
                m.cores[busy].intenable = 1 << 1; // level-one external interrupt
                // Capture interrupt delivery time once, then spin in the handler.
                m.bus.load_bytes(VECTOR + xtensa_lx7::state::vec::KERNEL,
                    &[0x20, 0xea, 0x03, 0x06, 0xff, 0xff]).unwrap(); // rsr a2,ccount; j .
                let source = esp32s3::periph::SRC_SYSTIMER_T0 as u32;
                m.bus.write32(0x600c_2000 + busy as u32 * 0x800 + source * 4, 1).unwrap();
                m.bus.write32(0x6002_3000, (1 << 30) | (1 << 24)).unwrap();
                m.bus.write32(0x6002_3020, ticks).unwrap();
                m.bus.write32(0x6002_3064, 1).unwrap();
                m.bus.write32(0x6002_3050, 1).unwrap();
                m.vq_max = vq;
                m.max_cycles = m.bus.cycles + 512;
                assert!(matches!(m.run(2048), Stop::Halted));
                assert_eq!(m.interrupts, 1, "alarm must reach the running core");
                assert_eq!(m.cores[busy].epc[1], IRAM);
                assert!((64..576).contains(&m.cores[busy].get_ar(2)), "handler must capture its arrival time");
                assert_eq!(m.bus.periph.systimer.int_raw & 1, 1);
                assert_eq!(m.bus.vq_violations, 0);
                if vq > 1 && std::env::var_os("ESP32SIM_VQ_NATIVE").is_some() {
                    assert!(m.vq_stats[0] > 0 && m.vq_stats[1] >= 2, "exercise batched rounds");
                }
                results.push((m.bus.cycles, m.cores.iter()
                    .map(|c| (c.ccount, c.insn_count, c.pc, c.ps, c.interrupt, c.epc, c.get_ar(2))).collect::<Vec<_>>()));
            }
            assert_eq!(results[0], results[1], "busy={busy} ticks={ticks}");
        }
    }
}

#[test]
fn architectural_stop_preserves_unfinished_round() {
    // Native virtual quanta require ESP32SIM_VQ_NATIVE=1 and the interpreter.
    // Cover both round boundaries and partial rounds, with either core busy.
    for (quantum, busy) in [32usize, 64, 128].into_iter().flat_map(|q| (0..2).map(move |b| (q, b))) {
        for instructions in [1, quantum - 1, quantum, quantum + 1, 2 * quantum - 1, 2 * quantum, 2 * quantum + 1] {
            let mut results = Vec::new();
            for vq in [1, 1024] {
                let mut m = esp32s3::machine([0; 6]);
                m.console.capture = true;
                m.vq_max = 1;
                for c in &mut m.cores { c.set_jit(false); }
                m.bus.load_bytes(IRAM, &[0x06, 0xff, 0xff]).unwrap();
                m.bus.load_bytes(0x4000_0400, &[0x06, 0xff, 0xff]).unwrap();
                m.cores[0].pc = IRAM;
                m.cores[0].ps = 0;
                if busy == 1 { m.bus.write32(0x600c_0000, 2).unwrap(); }
                assert!(matches!(m.run(64), Stop::MaxInsns));
                if busy == 1 { m.cores[0].waiting = true; }
                let mut code = [0x3d, 0xf0].repeat(instructions - 1); // nop.n
                code.extend([0, 0, 0]); // ill
                m.bus.load_bytes(IRAM + 0x100, &code).unwrap();
                m.cores[busy].pc = IRAM + 0x100;
                m.cores[busy].ps = 0;
                m.dbg.stop_after_exceptions = 1;
                m.quantum = quantum as u64;
                m.vq_max = vq;
                m.max_cycles = 64 + (instructions as u64).div_ceil(quantum as u64).max(2) * quantum as u64;
                let stop = m.run(1024);
                assert!(matches!(stop, Stop::Exceptions(1)), "quantum={quantum} busy={busy} instructions={instructions} vq={vq}: {stop:?}");
                if vq > 1 && std::env::var_os("ESP32SIM_VQ_NATIVE").is_some() {
                    assert!(m.vq_stats[0] > 0);
                }
                assert_eq!(m.bus.vq_violations, 0, "no undeferred device accesses");
                results.push((m.bus.cycles, m.cores.iter().map(|c| (c.ccount, c.insn_count, c.pc, c.ps)).collect::<Vec<_>>()));
            }
            assert_eq!(results[0], results[1], "quantum={quantum} busy={busy} instructions={instructions}");
        }
    }
}

#[test]
fn waiti_inside_virtual_round_preserves_timer_ordering() {
    const SPIN: [u8; 3] = [0x06, 0xff, 0xff];
    const START: u32 = IRAM + 0x100;
    const VECTOR_BASE: u32 = IRAM + 0x1000;
    let timer = xtensa_lx7::state::TIMER_INTERRUPT[0];
    for busy in 0..2 {
        // Include short partial rounds, exact boundaries and partial rounds after full ones.
        for wait_at in [3, 63, 64, 65, 67, 127, 128, 129] {
            for wake_after in [1, 2, 63, 65] {
                let mut results = Vec::new();
                for vq in [1, 1024] {
                    let mut m = esp32s3::machine([0; 6]);
                    m.console.capture = true;
                    m.vq_max = 1;
                    for core in &mut m.cores { core.set_jit(false); }
                    m.bus.load_bytes(IRAM, &SPIN).unwrap();
                    m.bus.load_bytes(0x4000_0400, &SPIN).unwrap();
                    m.cores[0].pc = IRAM;
                    m.cores[0].ps = 0;
                    m.bus.write32(0x600c_0000, 2).unwrap();
                    assert!(matches!(m.run(64), Stop::MaxInsns));
                    m.cores[1 - busy].waiting = true;
                    let mut code = [0x3d, 0xf0].repeat(wait_at - 1); // nop.n
                    code.extend([0x00, 0x70, 0x00]); // waiti 0
                    code.extend(SPIN);
                    m.bus.load_bytes(START, &code).unwrap();
                    m.bus.load_bytes(VECTOR_BASE + xtensa_lx7::state::vec::KERNEL, &SPIN).unwrap();
                    let cpu = &mut m.cores[busy];
                    cpu.pc = START;
                    cpu.ps = 0;
                    cpu.vecbase = VECTOR_BASE;
                    cpu.intenable = 1 << timer;
                    cpu.ccompare[0] = cpu.ccount.wrapping_add((wait_at + wake_after) as u32);
                    m.vq_max = vq;
                    m.max_cycles = m.bus.cycles + 384;
                    assert!(matches!(m.run(1024), Stop::Halted));
                    assert_eq!(m.interrupts, 1, "one wakeup, busy={busy} wait_at={wait_at} wake_after={wake_after} vq={vq}");
                    assert_eq!(m.cores[busy].epc[1], START + 2 * (wait_at as u32 - 1) + 3, "timer resumes after WAITI");
                    if vq > 1 && std::env::var_os("ESP32SIM_VQ_NATIVE").is_some() {
                        assert!(m.vq_stats[0] > 0 && m.vq_stats[3] > 0, "exercise a virtual run cut by WAITI");
                    }
                    assert_eq!(m.bus.vq_violations, 0, "no undeferred device accesses");
                    results.push((m.bus.cycles, m.run_steps(), m.irq_hist.clone(), m.cores.iter()
                        .map(|c| (c.ccount, c.insn_count, c.pc, c.ps, c.interrupt, c.epc, c.waiting)).collect::<Vec<_>>()));
                }
                assert_eq!(results[0], results[1], "busy={busy} wait_at={wait_at} wake_after={wake_after}");
            }
        }
    }
}

#[test]
fn virtual_runs_stop_before_register_reads_and_script_events() {
    let mut results = Vec::new();
    for vq in [1, 1024] {
        let mut m = esp32s3::machine([0; 6]);
        m.console.capture = true;
        for core in &mut m.cores { core.set_jit(false); }
        let mut code = [0x3d, 0xf0].repeat(150); // nop.n
        code.extend([0x22, 0x23, 0x00]); // l32i a2,a3,0: device access cuts virtual run
        code.extend([0x06, 0xff, 0xff]); // j .
        m.bus.load_bytes(IRAM, &code).unwrap();
        m.cores[0].pc = IRAM;
        m.cores[0].ps = 0;
        m.cores[0].set_ar(3, 0x6002_3000); // SYSTIMER configuration register
        m.vq_max = vq;
        m.script.log = false;
        m.script.events = vec![(192, esp_soc::ScriptAction::Serial("event".into())), (320, esp_soc::ScriptAction::Stop)];
        assert!(matches!(m.run(1024), Stop::Halted));
        assert_eq!(m.bus.vq_violations, 0);
        assert_eq!(m.script.pos, 2);
        assert_eq!(m.bus.periph.usb.rx.iter().copied().collect::<Vec<_>>(), b"event");
        if vq > 1 && std::env::var_os("ESP32SIM_VQ_NATIVE").is_some() {
            assert!(m.vq_stats[0] > 0 && m.vq_stats[2] > 0, "exercise register deferral");
        }
        results.push((m.bus.cycles, m.insns(), m.cores[0].ccount, m.cores[0].pc, m.cores[0].get_ar(2)));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0].0, 320);
}

/// EX177: batching whole rounds while both cores are busy must leave exactly the state the
/// per-round schedule leaves, including in the round a batch is cut in. Native deferral needs
/// ESP32SIM_VQ_NATIVE=1 and the interpreter; without it the batch never arms and the comparison
/// only proves the knob is inert.
#[test]
fn round_batches_match_the_per_round_schedule() {
    const NOP: [u8; 2] = [0x3d, 0xf0];
    const SPIN: [u8; 3] = [0x06, 0xff, 0xff];       // j .
    const BACK: [u8; 3] = [0x06, 0xfd, 0xff];       // j -12: back over four nop.n
    const CODE: [u32; 2] = [IRAM + 0x100, IRAM + 0x400];
    const VECTORS: u32 = IRAM + 0x1000;
    let timer = xtensa_lx7::state::TIMER_INTERRUPT[0];
    let armed = std::env::var_os("ESP32SIM_VQ_NATIVE").is_some();
    // kinds: 0 uncut, 1 device read, 2 waiti, 3 core-local timer, 4 script events, 5 illegal
    for kind in 0..6 {
        for at in [1usize, 63, 64, 65, 129] {
            for cut in 0..2usize {
                let mut results = Vec::new();
                for bb in [1u64, 16] {
                    let mut m = esp32s3::machine([0; 6]);
                    m.console.capture = true;
                    m.vq_max = 1;
                    for c in &mut m.cores { c.set_jit(false); }
                    // core 0 spins while the scheduler releases core 1 through its reset path
                    m.bus.load_bytes(IRAM, &SPIN).unwrap();
                    m.bus.load_bytes(0x4000_0400, &SPIN).unwrap();
                    m.cores[0].pc = IRAM;
                    m.cores[0].ps = 0;
                    m.bus.write32(0x600c_0000, 2).unwrap();
                    assert!(matches!(m.run(64), Stop::MaxInsns));
                    let peer: Vec<u8> = NOP.repeat(4).into_iter().chain(BACK).collect();
                    let mut code: Vec<u8> = NOP.repeat(at);
                    match kind {
                        1 => code.extend([0x22, 0x23, 0x00]),       // l32i a2,a3,0: SYSTIMER
                        2 => code.extend([0x00, 0x70, 0x00]),       // waiti 0
                        5 => code.extend([0x00, 0x00, 0x00]),       // ill
                        _ => {}
                    }
                    code.extend(SPIN);
                    m.bus.load_bytes(VECTORS + xtensa_lx7::state::vec::KERNEL, &SPIN).unwrap();
                    for (i, &entry) in CODE.iter().enumerate().take(2) {
                        m.bus.load_bytes(entry, if i == cut { &code } else { &peer }).unwrap();
                        let c = &mut m.cores[i];
                        c.pc = entry; c.ps = 0; c.waiting = false;
                        c.intenable = 0; c.interrupt = 0; c.vecbase = VECTORS;
                        c.set_ar(3, 0x6002_3000);
                    }
                    if kind == 3 {
                        let c = &mut m.cores[cut];
                        c.ccompare[0] = c.ccount.wrapping_add(at as u32);
                        c.intenable = 1 << timer;
                    }
                    if kind == 4 {
                        m.script.log = false;
                        m.script.events = vec![
                            (m.bus.cycles + at as u64, esp_soc::ScriptAction::Serial("event".into())),
                            (m.bus.cycles + 300, esp_soc::ScriptAction::Stop),
                        ];
                    }
                    if kind == 5 { m.dbg.stop_after_exceptions = 1; }
                    m.bb_max = bb;
                    m.max_cycles = m.bus.cycles + 512;
                    let label = format!("kind={kind} at={at} cut={cut} bb={bb}");
                    let stop = m.run(1 << 20);
                    if kind == 5 { assert!(matches!(stop, Stop::Exceptions(1)), "{label}: {stop:?}"); }
                    else { assert!(matches!(stop, Stop::Halted), "{label}: {stop:?}"); }
                    assert_eq!(m.bus.vq_violations, 0, "{label}: undeferred device access");
                    if armed && bb > 1 {
                        assert!(m.bb_stats[0] > 0, "{label}: no batch ran");
                        if kind == 1 { assert!(m.bb_stats[2] > 0, "{label}: no device-register cut"); }
                        if kind == 2 { assert!(m.bb_stats[3] > 0, "{label}: no waiti cut"); }
                        if kind == 3 { assert!(m.interrupts > 0, "{label}: timer never fired"); }
                        if kind == 4 { assert_eq!(m.script.pos, 2, "{label}: script events never applied"); }
                    }
                    results.push((m.bus.cycles, m.run_steps(), m.insns(), m.script.pos, m.console.all.clone(),
                        m.exceptions, m.interrupts, m.irq_hist.clone(), m.bus.periph.usb.rx.iter().copied().collect::<Vec<_>>(),
                        m.cores.iter().map(|c| (c.pc, c.ps, c.ccount, c.insn_count, c.interrupt, c.epc, c.waiting, c.ar)).collect::<Vec<_>>()));
                }
                assert_eq!(results[0], results[1], "kind={kind} at={at} cut={cut}");
            }
        }
    }
}

#[test]
fn frontier_instruction_observers_do_not_arm_mmio_deferral() {
    let mut m = esp32s3::machine([0; 6]);
    m.console.capture = true;
    m.vq_max = 1024;
    m.set_approximate_jit_timing(1, 64).unwrap();
    m.set_approximate_jit_frontiers(true).unwrap();
    m.add_observer(Box::new(esp_soc::observers::Breakpoints { pcs: vec![IRAM + 100] }));
    // The second instruction reads MMIO after the batch has already spent cycles.
    m.bus.load_bytes(IRAM, &[0x3d, 0xf0, 0x22, 0x23, 0, 0x06, 0xff, 0xff]).unwrap();
    m.cores[0].pc = IRAM;
    m.cores[0].ps = 0;
    m.cores[0].set_ar(3, 0x6002_3000);
    m.max_cycles = 128;
    assert!(matches!(m.run(1024), Stop::Halted));
    assert_eq!(m.bus.vq_violations, 0);
}
