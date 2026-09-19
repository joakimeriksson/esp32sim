use emu_core::Bus;
use esp_soc::{Soc, SocBus};
use esp32s3::bus::{IBUS_LOW, MMU_TABLE, PAGE};

// Run in a separate test process: the experimental fetch cache is module-global,
// so unrelated library tests remapping their own buses must not clear this fixture.
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
    xtensa_lx7::state::reset_shared_fetch_cache();
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
