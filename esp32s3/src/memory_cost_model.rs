//! Rough memory service layered over the whole-firmware instruction model.
//! MMU writes are observed from firmware; use ROM boot, not synthetic app boot.
use crate::{bus::*, rough_memory::*, ApproximateCostModel};
use crate::approximate_cache::{CacheConfig, CacheTiming};
use emu_core::{CostModel, ExecutionFacts, LifecycleFacts, LifecycleKind, MemoryAccessKind};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug)]
pub struct MemoryCostModel {
    pub cpu: ApproximateCostModel,
    pub memory: Rc<RefCell<MemoryTiming>>,
    pub cache: Option<Rc<RefCell<CacheTiming>>>,
    cache_line_bytes: u32,
    mmu: [u32; MMU_ENTRIES],
}
impl MemoryCostModel {
    pub fn new(cpu: ApproximateCostModel, config: MemoryConfig) -> Self {
        Self { cpu, memory: Rc::new(RefCell::new(MemoryTiming::new(config))),
            mmu: [MMU_INVALID; MMU_ENTRIES], cache: None, cache_line_bytes: 64 }
    }
    /// Data-cache misses use the memory service model. Instruction cache is assumed warm.
    /// The wrapped CPU model must have its own data_cache disabled to avoid double charging.
    pub fn with_cache(mut self, mut config: CacheConfig) -> Self {
        assert!(self.cpu.config.data_cache.is_none());
        config.hit_cycles = 0; config.fill_cycles = 0; config.writeback_cycles = 0;
        self.cache_line_bytes = config.line_bytes;
        self.cache = Some(Rc::new(RefCell::new(CacheTiming::new(config))));
        self
    }
}
impl CostModel for MemoryCostModel {
    fn lifecycle(&mut self, facts: &LifecycleFacts) -> Result<(), String> {
        self.cpu.lifecycle(facts)?;
        if matches!(facts.kind, LifecycleKind::Attach | LifecycleKind::ChipReset) {
            self.mmu.fill(MMU_INVALID);
            self.memory.borrow_mut().reset();
            if let Some(cache) = &self.cache { cache.borrow_mut().reset(); }
        }
        Ok(())
    }
    fn cycles(&mut self, _: &ExecutionFacts<'_>) -> Result<u32, String> {
        Err("memory contention requires shared time: call cycles_at".into())
    }
    fn cycles_at(&mut self, facts: &ExecutionFacts<'_>, now: u64) -> Result<u32, String> {
        let cpu_cycles = self.cpu.cycles_at(facts, now)?;
        let mut cursor = now;
        let mut memory = self.memory.borrow_mut();
        for access in facts.accesses {
            if access.fault.is_some() { continue; }
            if access.kind == MemoryAccessKind::Write && access.width == 4 &&
                (MMU_TABLE..MMU_TABLE + MMU_ENTRIES as u32 * 4).contains(&access.address) {
                self.mmu[((access.address - MMU_TABLE) >> 2) as usize] = access.value & 0xffff;
            }
            if let Some(kind) = memory_kind(access.address, &self.mmu) {
                if matches!(kind, MemoryKind::Flash | MemoryKind::Psram) {
                    if let Some(cache) = &self.cache {
                        if access.kind == MemoryAccessKind::Fetch { continue; }
                        let entry = self.mmu[((access.address & 0x1ff_ffff) >> 16) as usize];
                        let physical = ((entry & 0x3fff) << 16) | (access.address & 0xffff)
                            | if kind == MemoryKind::Psram { 1 << 30 } else { 0 };
                        let cost = cache.borrow_mut().access(physical, u32::from(access.width),
                            access.kind == MemoryAccessKind::Write);
                        for _ in 0..cost.dirty_writebacks {
                            cursor = memory.access(cursor, MemoryKind::Psram, self.cache_line_bytes).finish;
                        }
                        for _ in 0..cost.line_fills {
                            cursor = memory.access(cursor, kind, self.cache_line_bytes).finish;
                        }
                        continue;
                    }
                }
                cursor = memory.access(cursor, kind, u32::from(access.width)).finish;
            }
        }
        Ok(u64::from(cpu_cycles).saturating_add(cursor - now).min(u64::from(u32::MAX)) as u32)
    }
}
