use emu_core::Bus;
use esp_soc::{Soc, SocBus};
use esp32s3::bus::{IBUS_LOW, MMU_TABLE, PAGE};

#[test]
fn fetch_cache_cold_after_remap_and_chip_reset_keeps_core_reset() {
    const SPIN: [u8; 3] = [0x06, 0xff, 0xff]; // j .
    let mut m = esp32s3::machine([1, 2, 3, 4, 5, 6]);
    for off in [0usize, PAGE as usize] {
        SocBus::write_flash(&mut m.bus, off, &SPIN).unwrap();
    }
    m.bus.write32(MMU_TABLE, 0).unwrap();
    m.max_cycles = 100_000;
    let park = |m: &mut esp32s3::Machine| {
        m.cores[0].pc = IBUS_LOW;
        m.cores[0].ps = 0;
        m.cores[0].waiting = false;
    };
    m.cores[0].price_control = true;
    m.cores[0].icache_fill = 404;
    park(&mut m);
    assert!(matches!(m.run(8), esp_soc::Stop::MaxInsns));
    assert_eq!(m.cores[0].icache_misses, 1);
    m.run(8);
    assert_eq!(m.cores[0].icache_misses, 1, "the warm virtual tag hits");
    let warm_misses = m.cores[0].icache_misses;

    m.bus.write32(MMU_TABLE, 1).unwrap();
    m.run(8);
    assert_eq!(m.cores[0].icache_misses, warm_misses + 1, "a remapped physical line is not warm");

    SocBus::reboot(&mut m.bus, [1, 2, 3, 4, 5, 6]);
    m.bus.write32(MMU_TABLE, 0).unwrap();
    park(&mut m);
    m.run(8);
    assert_eq!(m.cores[0].icache_misses, warm_misses + 2, "a post-reset refill is not warm");

    SocBus::load_bytes(&mut m.bus, IBUS_LOW, &SPIN).unwrap();
    m.run(8);
    assert_eq!(m.cores[0].icache_misses, warm_misses + 2);
    esp32s3::S3::reset_core(&mut m.cores[0], 0);
    park(&mut m);
    m.run(8);
    assert_eq!(m.cores[0].icache_misses, warm_misses + 2, "a core reset keeps the shared fetch cache");
}

#[test]
fn fetch_cache_is_shared_by_cores_but_isolated_between_machines() {
    let mut first = esp32s3::machine([1; 6]);
    for core in &mut first.cores { core.icache_fill = 7; }
    first.cores[0].touch_fetch_lines(IBUS_LOW, IBUS_LOW + 2);
    first.cores[1].touch_fetch_lines(IBUS_LOW, IBUS_LOW + 2);
    assert_eq!(first.cores[0].icache_misses, 1);
    assert_eq!(first.cores[1].icache_misses, 0, "core 1 shares core 0's warm line");

    // Constructing another machine must neither borrow nor invalidate first's tags.
    let mut second = esp32s3::machine([2; 6]);
    second.cores[0].icache_fill = 7;
    second.cores[0].touch_fetch_lines(IBUS_LOW, IBUS_LOW + 2);
    assert_eq!(second.cores[0].icache_misses, 1, "a new machine starts cold");
    first.cores[1].touch_fetch_lines(IBUS_LOW, IBUS_LOW + 2);
    assert_eq!(first.cores[1].icache_misses, 0);

    second.bus.write32(MMU_TABLE, 0).unwrap();
    first.cores[0].touch_fetch_lines(IBUS_LOW, IBUS_LOW + 2);
    assert_eq!(first.cores[0].icache_misses, 1, "another machine's remap leaves this one warm");
    second.cores[0].touch_fetch_lines(IBUS_LOW, IBUS_LOW + 2);
    assert_eq!(second.cores[0].icache_misses, 2, "a remap invalidates its own machine");

    SocBus::reboot(&mut second.bus, [2; 6]);
    first.cores[0].touch_fetch_lines(IBUS_LOW, IBUS_LOW + 2);
    assert_eq!(first.cores[0].icache_misses, 1, "another machine's reset leaves this one warm");
    second.cores[0].touch_fetch_lines(IBUS_LOW, IBUS_LOW + 2);
    assert_eq!(second.cores[0].icache_misses, 3);
    assert_eq!(first.cores[0].timing_extra, 7);
    assert_eq!(second.cores[0].timing_extra, 21);
}
