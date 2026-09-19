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
