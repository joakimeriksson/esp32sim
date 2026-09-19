use super::*;

fn timed_bus() -> SocBus {
    let mut bus = SocBus::new(65536, 65536, [0; 6]);
    bus.mmu[0] = MMU_SPIRAM;
    bus.enable_approximate_cache(crate::approximate_cache::CacheConfig {
        capacity_bytes: 64, ways: 1, ..Default::default()
    });
    bus.set_approximate_cache_contention(true);
    bus.begin_timing_batch(1, 100);
    bus
}

#[test]
fn dma_does_not_allocate_or_charge_cpu() {
    let mut bus = timed_bus();
    assert!(bus.dma_copy(DBUS_LOW, DRAM_LOW, 64).is_ok());
    assert_eq!(bus.take_timing_penalty(), 0);
    assert_eq!(bus.approximate_cache_stats().unwrap(), Default::default());
    assert_eq!(bus.cache_resource.cursor, 100);
    assert_eq!(bus.cache_resource.busy_until, 0);
    bus.read32(DBUS_LOW).unwrap();
    assert_eq!(bus.take_timing_penalty(), 120);
}

#[test]
fn dma_does_not_evict_or_dirty_cpu_lines() {
    let mut bus = timed_bus();
    bus.read32(DBUS_LOW).unwrap();
    assert_eq!(bus.take_timing_penalty(), 120);
    let stats = bus.approximate_cache_stats();
    assert!(bus.dma_copy(DRAM_LOW, DBUS_LOW, 4).is_ok());
    assert!(bus.dma_copy(DBUS_LOW + 64, DRAM_LOW, 64).is_ok());
    assert!(bus.dma_copy(DRAM_LOW, DBUS_LOW + 128, 64).is_ok());
    assert_eq!(bus.approximate_cache_stats(), stats);
    assert_eq!(bus.take_timing_penalty(), 0);
    bus.read32(DBUS_LOW).unwrap();
    assert_eq!(bus.take_timing_penalty(), 0);
    bus.read32(DBUS_LOW + 64).unwrap();
    assert_eq!(bus.take_timing_penalty(), 120, "DMA did not dirty the victim");
}

#[test]
fn unpriced_accesses_preserve_data_versions_faults_and_cpu_debt() {
    let mut bus = timed_bus();
    // Alias the next MMU page so split accesses exercise the byte fallback.
    bus.mmu[1] = MMU_SPIRAM;
    bus.write32(DBUS_LOW + 128, 1).unwrap();
    let stats = bus.approximate_cache_stats();
    let versions = bus.page_ver.clone();
    bus.write8_unpriced(DBUS_LOW, 0xab).unwrap();
    assert_eq!(bus.read8_unpriced(DBUS_LOW).unwrap(), 0xab);
    bus.write16_unpriced(DBUS_LOW + PAGE - 1, 0x1234).unwrap();
    assert_eq!(bus.read16_unpriced(DBUS_LOW + PAGE - 1).unwrap(), 0x1234);
    bus.write32_unpriced(DBUS_LOW + PAGE - 2, 0x12345678).unwrap();
    assert_eq!(bus.read32_unpriced(DBUS_LOW + PAGE - 2).unwrap(), 0x12345678);
    assert_ne!(bus.page_ver, versions, "DMA/host writes still invalidate decoded code");
    assert!(bus.read32_unpriced(0).is_err());
    assert_eq!(bus.last_fault, Some((0, false)));
    assert_eq!(bus.approximate_cache_stats(), stats);
    assert_eq!(bus.take_timing_penalty(), 120, "existing CPU debt is retained");
    bus.read32(DBUS_LOW + 64).unwrap();
    assert_eq!(bus.take_timing_penalty(), 216, "CPU writes still dirty the victim");
}

#[test]
fn chip_reset_makes_cached_memory_cold() {
    let mut bus = timed_bus();
    bus.read32(DBUS_LOW).unwrap();
    assert_eq!(bus.take_timing_penalty(), 120);
    esp_soc::SocBus::reboot(&mut bus, [0; 6]);
    bus.mmu[0] = MMU_SPIRAM;
    bus.read32(DBUS_LOW).unwrap();
    assert_eq!(bus.take_timing_penalty(), 120);
}

#[test]
fn chip_reset_clears_timing_state_but_keeps_configuration() {
    let mut bus = timed_bus();
    bus.set_approximate_cache_fast_internal(true);
    bus.set_approximate_cache_yield_miss(true);
    bus.set_approximate_cache_fill_service(200);
    bus.set_approximate_flash_timing(80, 300);
    bus.write32(DBUS_LOW, 42).unwrap();
    bus.begin_timing_batch(0, 100);
    bus.write32(DBUS_LOW + 64, 43).unwrap();
    assert_ne!(bus.approximate_cache_wait_cycles(), [0; 2]);
    bus.cycles = 150;
    esp_soc::SocBus::reboot(&mut bus, [0; 6]);
    assert_eq!(bus.take_timing_penalty(), 0);
    assert_eq!(bus.approximate_cache_stats().unwrap(), Default::default());
    assert_eq!(bus.approximate_cache_wait_cycles(), [0; 2]);
    assert_eq!(bus.cache_resource.cursor, 150);
    assert_eq!(bus.cache_resource.busy_until, 150);
    assert!(bus.cache_resource.enabled);
    assert_eq!(bus.cache_resource.fill_service_cycles, 200);
    assert_eq!(bus.cache_resource.flash_timing, Some((80, 300)));
    assert!(bus.approximate_cache_fast_internal);
    assert!(bus.approximate_cache_yield_miss);
    bus.mmu[0] = MMU_SPIRAM;
    bus.begin_timing_batch(1, 150);
    assert_eq!(bus.read32(DBUS_LOW + 64).unwrap(), 43);
    assert_eq!(bus.take_timing_penalty(), 120, "no pre-reset dirty victim or resource debt");
    bus.begin_timing_batch(0, 270);
    bus.read32(DBUS_LOW).unwrap();
    assert_eq!(bus.take_timing_penalty(), 80 + 120, "configured burst service survives");
    bus.mmu[1] = 0;
    bus.begin_timing_batch(0, 1000);
    bus.read32(DBUS_LOW + PAGE).unwrap();
    assert_eq!(bus.take_timing_penalty(), 80, "flash override survives");
}

#[test]
fn core_reset_preserves_the_shared_cache() {
    use esp_soc::Soc;
    let mut bus = timed_bus();
    bus.write32(DBUS_LOW, 42).unwrap();
    assert_eq!(bus.take_timing_penalty(), 120);
    let mut core = crate::soc::S3::new_core(1);
    crate::soc::S3::reset_core(&mut core, 1);
    bus.begin_timing_batch(0, 220);
    assert_eq!(bus.read32(DBUS_LOW).unwrap(), 42);
    assert_eq!(bus.take_timing_penalty(), 0);
    bus.read32(DBUS_LOW + 64).unwrap();
    assert_eq!(bus.take_timing_penalty(), 216, "shared dirty line survives core reset");
}

#[test]
fn host_peek_does_not_warm_cpu_cache() {
    let mut machine = crate::machine([0; 6]);
    machine.bus = timed_bus();
    machine.peek(DBUS_LOW, 1);
    assert_eq!(machine.bus.take_timing_penalty(), 0);
    machine.bus.read32(DBUS_LOW).unwrap();
    assert_eq!(machine.bus.take_timing_penalty(), 120);
}
