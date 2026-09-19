use esp_periph::{Device, RtcCntl, TimerGroup, RST_RTCWDT_SYS};

const UNLOCK: u32 = 0x50d8_3aa1;
const RESET_STAGE0: u32 = (1 << 31) | (3 << 28);

#[test]
fn timer_one_alarm_has_an_independent_interrupt_source() {
    let mut group = TimerGroup::new();
    group.write(0x24, (1 << 31) | (1 << 30) | (2 << 13) | (1 << 10));
    group.write(0x34, 5);
    group.tick(10);
    assert_eq!(group.read(0x74), 2, "T1 alarm sets raw bit 1");
    assert_eq!(Device::irq_sources(&group), 0, "masked until enabled");
    group.write(0x70, 2);
    assert_eq!(Device::irq_sources(&group), 2);
    group.write(0x7c, 1);
    assert_eq!(Device::irq_sources(&group), 2, "clearing T0 preserves T1");
    group.write(0x7c, 2);
    assert_eq!(Device::irq_sources(&group), 0);
}

#[test]
fn rtc_watchdog_layouts_protect_feed_and_reset_at_their_own_offsets() {
    for (mut rtc, base) in [(RtcCntl::new(), 0x98), (RtcCntl::new_c3(), 0x90)] {
        rtc.write(base, RESET_STAGE0);
        assert_eq!(rtc.read(base), 0, "configuration starts write-protected");
        rtc.write(base + 0x18, UNLOCK);
        rtc.write(base + 4, 10);
        rtc.write(base, RESET_STAGE0);
        rtc.wdt_tick(9);
        assert!(!rtc.sw_reset);
        rtc.write(base + 0x14, 1 << 31);
        rtc.wdt_tick(9);
        assert!(!rtc.sw_reset, "feed restarts the stage timeout");
        rtc.write(base + 0x18, 0);
        rtc.write(base, 0);
        rtc.write(base + 0x14, 1 << 31);
        rtc.wdt_tick(1);
        assert!(rtc.sw_reset, "locked disable and feed writes have no effect");
        assert_eq!(rtc.reset_cause, RST_RTCWDT_SYS);
    }
}

#[test]
fn rtc_watchdog_interrupt_stage_sets_the_documented_raw_bit() {
    for (mut rtc, base) in [(RtcCntl::new(), 0x98), (RtcCntl::new_c3(), 0x90)] {
        rtc.write(base + 0x18, UNLOCK);
        rtc.write(base + 4, 5);
        rtc.write(base, (1 << 31) | (1 << 28));
        rtc.wdt_tick(5);
        assert_eq!(rtc.read(0x44) & (1 << 3), 1 << 3);
        assert!(!rtc.sw_reset);
    }
}

#[test]
fn s3_watchdog_unlock_address_does_not_unlock_c3() {
    let mut rtc = RtcCntl::new_c3();
    rtc.write(0xb0, UNLOCK);
    rtc.write(0x90, RESET_STAGE0);
    assert_eq!(rtc.read(0x90), 0);
    rtc.wdt_tick(100);
    assert!(!rtc.sw_reset);
}
