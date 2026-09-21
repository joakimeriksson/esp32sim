use esp32c3::periph::Peripherals;
use esp_periph::RST_RTCWDT_SYS;

#[test]
fn c3_mmio_arms_the_rtc_watchdog_at_c3_addresses() {
    let mut peripherals = Peripherals::new([0; 6]);
    peripherals.write32(0x6000_80a8, 0x50d8_3aa1);
    peripherals.write32(0x6000_8094, 10);
    peripherals.write32(0x6000_8090, (1 << 31) | (3 << 28));
    peripherals.tick(10_000); // Nine ticks of the 150 kHz RTC slow clock.
    assert!(!peripherals.rtc.sw_reset);
    peripherals.tick(1_000);
    assert!(peripherals.rtc.sw_reset);
    assert_eq!(peripherals.rtc.reset_cause, RST_RTCWDT_SYS);
}
