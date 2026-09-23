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

/// q256: one long tick leaves a timer exactly as the same APB ticks one at a time. Steps after an
/// alarm survive the autoreload, counting up or down, over several crossings, for any divider.
#[test]
fn timer_long_tick_equals_single_ticks() {
    // (start, load, alarm): count up with start < alarm, count down with start > alarm
    let up = [(0, 0, 32), (0, 0, 5), (3, 0, 1), (0, 40, 32), (10, 32, 32), (40, 0, 32)];
    let down = [(40, 40, 30), (40, 40, 39), (40, 20, 30), (40, 30, 30), (20, 40, 30)];
    let mut crossings = 0;
    for inc in [true, false] {
        for &(start, load, alarm) in if inc { &up[..] } else { &down[..] } {
            for auto in [true, false] {
                for div in [1u32, 2, 3, 7] {
                    for n in [1u64, 5, 64, 256, 1000] {
                        let mut g = [TimerGroup::new(), TimerGroup::new()];
                        for g in &mut g {
                            g.write(0x18, start);
                            g.write(0x20, 1);                                  // count = load
                            g.write(0x18, load);
                            g.write(0x10, alarm);
                            g.write(0x0, (1 << 31) | u32::from(inc) << 30 | u32::from(auto) << 29 | div << 13 | (1 << 10));
                        }
                        g[0].tick(n);
                        for _ in 0..n { g[1].tick(1); }
                        let label = format!("inc={inc} auto={auto} div={div} start={start} load={load} alarm={alarm} n={n}");
                        let [a, b] = &g;
                        assert_eq!((a.t[0].count, a.t[0].prescale_acc, a.t[0].config, a.int_raw),
                                   (b.t[0].count, b.t[0].prescale_acc, b.t[0].config, b.int_raw), "{label}");
                        crossings += b.int_raw & 1;
                    }
                }
            }
        }
    }
    assert!(crossings > 100, "the cases must cross their alarms ({crossings})");
}
