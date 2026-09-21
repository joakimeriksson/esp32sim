//! Approximate memory service for latency/bandwidth sensitivity experiments.
//!
//! Schedules occupancy only: it does not defer load/store effects, model SRAM banks or
//! establish hardware accuracy. Supply shared simulated time, not instruction counts.
//! Flash and PSRAM deliberately share one resource. No per-access allocation.
use crate::bus::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryKind { Internal, Rom, Flash, Psram, Mmio }

/// Follow the simulator's MMU, including remapping. None means unmapped.
pub fn memory_kind(address: u32, mmu: &[u32; MMU_ENTRIES]) -> Option<MemoryKind> {
    Some(match address {
        DRAM_LOW..DRAM_HIGH | IRAM_LOW..IRAM_HIGH | RTC_FAST_LOW..RTC_FAST_HIGH |
        RTC_SLOW_LOW..RTC_SLOW_HIGH => MemoryKind::Internal,
        IROM_MASK_LOW..IROM_MASK_HIGH | DROM_MASK_LOW..DROM_MASK_HIGH => MemoryKind::Rom,
        DBUS_LOW..DBUS_HIGH | IBUS_LOW..IBUS_HIGH => {
            let entry = mmu[((address & 0x1ff_ffff) >> 16) as usize];
            if entry & MMU_INVALID != 0 { return None; }
            if entry & MMU_SPIRAM != 0 { MemoryKind::Psram } else { MemoryKind::Flash }
        }
        crate::periph::PERIPH_BASE..crate::periph::PERIPH_END => MemoryKind::Mmio,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct MemoryPrice {
    pub latency: u32,
    pub bytes_per_beat: u32,
    pub cycles_per_beat: u32,
}
impl MemoryPrice {
    /// A free comparison baseline, not a hardware claim.
    pub const FREE: Self = Self { latency: 0, bytes_per_beat: 4, cycles_per_beat: 0 };
    pub fn cycles(self, bytes: u32) -> u64 {
        if bytes == 0 { return 0; }
        u64::from(self.latency) + u64::from(bytes).div_ceil(u64::from(self.bytes_per_beat.max(1)))
            * u64::from(self.cycles_per_beat)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MemoryConfig {
    pub internal: MemoryPrice,
    pub rom: MemoryPrice,
    pub flash: MemoryPrice,
    pub psram: MemoryPrice,
    pub mmio: MemoryPrice,
    /// False prices transactions independently. True queues them behind shared resources.
    pub contention: bool,
}
impl Default for MemoryConfig {
    fn default() -> Self {
        Self { internal: MemoryPrice::FREE, rom: MemoryPrice::FREE, flash: MemoryPrice::FREE,
            psram: MemoryPrice::FREE, mmio: MemoryPrice::FREE, contention: false }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryCharge {
    pub finish: u64,
    pub service_cycles: u64,
    pub wait_cycles: u64,
}
impl MemoryCharge {
    pub fn cycles(self) -> u64 { self.service_cycles.saturating_add(self.wait_cycles) }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryStats {
    pub transactions: u64,
    pub bytes: u64,
    pub service_cycles: u64,
    pub wait_cycles: u64,
}

#[derive(Clone, Debug)]
pub struct MemoryTiming {
    pub config: MemoryConfig,
    // Internal, ROM, external, MMIO.
    busy_until: [u64; 4],
    /// Internal, ROM, flash, PSRAM, MMIO; flash and PSRAM still share occupancy.
    pub stats: [MemoryStats; 5],
}
impl MemoryTiming {
    pub fn new(config: MemoryConfig) -> Self {
        Self { config, busy_until: [0; 4], stats: [MemoryStats::default(); 5] }
    }
    pub fn reset(&mut self) { self.busy_until = [0; 4]; self.stats = [MemoryStats::default(); 5]; }
    pub fn access(&mut self, now: u64, kind: MemoryKind, bytes: u32) -> MemoryCharge {
        let price = match kind { MemoryKind::Internal => self.config.internal,
            MemoryKind::Rom => self.config.rom, MemoryKind::Flash => self.config.flash,
            MemoryKind::Psram => self.config.psram, MemoryKind::Mmio => self.config.mmio };
        self.reserve(now, kind, bytes, price.cycles(bytes))
    }
    /// Reserve a DMA burst or externally priced transaction. Reserve memory service, not
    /// the entire screen's wire-transfer time. Small bursts allow CPU interleaving.
    pub fn reserve(&mut self, now: u64, kind: MemoryKind, bytes: u32, service: u64) -> MemoryCharge {
        let resource = match kind { MemoryKind::Internal => 0, MemoryKind::Rom => 1,
            MemoryKind::Flash | MemoryKind::Psram => 2, MemoryKind::Mmio => 3 };
        if bytes == 0 { return MemoryCharge { finish: now, ..Default::default() }; }
        let start = if self.config.contention { now.max(self.busy_until[resource]) } else { now };
        let finish = start.saturating_add(service);
        if self.config.contention && service != 0 { self.busy_until[resource] = finish; }
        let wait = start - now;
        let stats = &mut self.stats[kind as usize];
        stats.transactions += 1;
        stats.bytes += u64::from(bytes);
        stats.service_cycles = stats.service_cycles.saturating_add(service);
        stats.wait_cycles = stats.wait_cycles.saturating_add(wait);
        MemoryCharge { finish, service_cycles: service, wait_cycles: wait }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flash_and_psram_contend_but_internal_does_not() {
        let mut timing = MemoryTiming::new(MemoryConfig { contention: true,
            psram: MemoryPrice { latency: 10, bytes_per_beat: 4, cycles_per_beat: 3 },
            ..Default::default() });
        timing.reserve(100, MemoryKind::Flash, 64, 40);
        let charge = timing.access(105, MemoryKind::Psram, 5);
        assert_eq!((charge.wait_cycles, charge.service_cycles, charge.finish), (35, 16, 156));
        assert_eq!(timing.access(105, MemoryKind::Internal, 4).finish, 105);
    }
    #[test]
    fn independent_mode_has_no_wait_and_empty_bursts_do_nothing() {
        let mut timing = MemoryTiming::new(MemoryConfig::default());
        timing.reserve(100, MemoryKind::Psram, 64, 40);
        assert_eq!(timing.reserve(105, MemoryKind::Flash, 64, 10).finish, 115);
        timing.config.contention = true;
        assert_eq!(timing.reserve(0, MemoryKind::Flash, 0, 1000).finish, 0);
        assert_eq!(timing.reserve(0, MemoryKind::Flash, 1, 1).finish, 1);
    }
    #[test]
    fn classification_tracks_mmu_remap_and_aliases() {
        let mut mmu = [MMU_INVALID; MMU_ENTRIES];
        assert_eq!(memory_kind(DBUS_LOW, &mmu), None);
        mmu[0] = 0;
        assert_eq!(memory_kind(DBUS_LOW, &mmu), Some(MemoryKind::Flash));
        mmu[0] = MMU_SPIRAM;
        assert_eq!(memory_kind(IBUS_LOW, &mmu), Some(MemoryKind::Psram));
        assert_eq!(memory_kind(DRAM_LOW, &mmu), Some(MemoryKind::Internal));
    }
    #[test]
    fn free_access_at_later_instruction_phase_does_not_reserve_future_time() {
        let mut timing = MemoryTiming::new(MemoryConfig { contention: true, ..Default::default() });
        timing.access(100, MemoryKind::Internal, 4);
        assert_eq!(timing.access(1, MemoryKind::Internal, 4).wait_cycles, 0);
        timing.reserve(10, MemoryKind::Internal, 4, 5);
        assert_eq!(timing.access(12, MemoryKind::Internal, 4).wait_cycles, 3);
    }
}
