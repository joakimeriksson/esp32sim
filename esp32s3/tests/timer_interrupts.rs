use esp32s3::periph::{Peripherals, SRC_TG0_T1, SRC_TG1_T1};

#[test]
fn both_timer_one_alarms_reach_the_s3_interrupt_matrix() {
    let mut peripherals = Peripherals::new([0; 6]);
    for (base, source) in [(0x6001_f000, SRC_TG0_T1), (0x6002_0000, SRC_TG1_T1)] {
        peripherals.write32(base + 0x24, (1 << 31) | (1 << 30) | (2 << 13) | (1 << 10));
        peripherals.write32(base + 0x34, 5);
        peripherals.write32(base + 0x70, 2);
        peripherals.tick(30); // 10 APB ticks at the S3's 240 MHz CPU clock
        let bit = 1 << (source % 32);
        assert_ne!(peripherals.source_status()[source / 32] & bit, 0);
        // CORE0_INTR_STATUS_REG_1 is refreshed from the device source table.
        assert_ne!(peripherals.read32(0x600c_2190) & bit, 0);
        peripherals.write32(base + 0x7c, 2);
        assert_eq!(peripherals.source_status()[source / 32] & bit, 0);
    }
}

/// q256: a busy round is one device tick, so a TIMG alarm inside it must not drop the steps after it.
/// TIMG0 T0 at divider 2 on the 80 MHz APB (CPU/3): 256 cycles are 85 APB ticks, 42 steps. The
/// register read must match at quantum 64 (the alarm lands on a round boundary) and 256.
#[test]
fn timg_steps_after_an_alarm_survive_a_long_round() {
    use emu_core::{Bus, Core};
    use esp_soc::{SocBus, Stop};
    // (increase, autoreload, start, load, alarm, count after 42 steps)
    for (inc, auto, start, load, alarm, want) in [
        (true, true, 0, 0, 32, 10),       // one crossing: 42 - 32
        (true, true, 0, 0, 5, 2),         // eight crossings: 42 % 5
        (false, true, 40, 40, 30, 38),    // counting down, four crossings: 40 - 42 % 10
        (true, false, 0, 0, 32, 42),      // one-shot: the alarm disarms, counting goes on
    ] {
        for q in [64, 256] {
            let mut m = esp32s3::machine([0; 6]);
            m.quantum = q;
            m.vq_max = 1;
            m.bb_max = 1;
            for c in &mut m.cores { c.set_jit(false); }
            SocBus::load_bytes(&mut m.bus, 0x4037_0000, &[0x06, 0xff, 0xff]).unwrap();
            m.cores[0].pc = 0x4037_0000;
            m.cores[0].ps = 0;
            let t = 0x6001_f000;
            m.bus.write32(t + 0x18, start).unwrap();
            m.bus.write32(t + 0x20, 1).unwrap();
            m.bus.write32(t + 0x18, load).unwrap();
            m.bus.write32(t + 0x10, alarm).unwrap();
            m.bus.write32(t, (1 << 31) | u32::from(inc) << 30 | u32::from(auto) << 29 | (2 << 13) | (1 << 10)).unwrap();
            assert!(matches!(m.run(256), Stop::MaxInsns));
            assert_eq!(m.bus.cycles, 256);
            m.bus.write32(t + 0xc, 1).unwrap();
            let label = format!("q={q} inc={inc} auto={auto} load={load} alarm={alarm}");
            assert_eq!(m.bus.read32(t + 4).unwrap(), want, "{label}");
            assert_eq!(m.bus.read32(t + 0x74).unwrap() & 1, 1, "{label}: alarm raised");
        }
    }
}
