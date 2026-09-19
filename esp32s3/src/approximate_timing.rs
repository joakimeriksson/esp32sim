//! Deliberately provisional whole-firmware timing. Not a cycle-accuracy claim.
//! Fixed-size counters replace the strict model's per-instruction receipt ledger.
use emu_core::{CostModel, ExecutionFacts, LifecycleFacts, LifecycleKind, MemoryAccessKind, StepKind};
use std::{cell::RefCell, rc::Rc};
use xtensa_lx7::Op;
use crate::approximate_cache::{CacheAccess, CacheConfig, CacheTiming};

#[derive(Clone, Copy, Debug)]
pub struct ApproximateTimingConfig {
    pub issue: u32,
    pub taken_branch: u32,
    pub trap: u32,
    /// Extra cycles per mapped external-memory access, including instruction fetch.
    /// Zero assumes a warm cache. It does not model cache misses or contention.
    pub external_extra: u32,
    pub mmio_extra: u32,
    /// Data cache only; virtual tags, provisional geometry/prices, no maintenance.
    pub data_cache: Option<CacheConfig>,
}

impl Default for ApproximateTimingConfig {
    fn default() -> Self {
        Self { issue: 1, taken_branch: 3, trap: 3, external_extra: 0, mmio_extra: 0, data_cache: None }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ApproximateTimingStats {
    pub events: [u64; 2],
    pub cycles: [u64; 2],
    pub traps: u64,
    pub controls: u64,
    pub opcode_fallbacks: u64,
    pub zero_overhead_loop_edges: u64,
    pub external_fetches: u64,
    pub external_data: u64,
    pub mmio: u64,
    pub other_addresses: u64,
    pub faults: u64,
    pub data_cache: CacheAccess,
}

#[derive(Clone, Debug)]
pub struct ApproximateCostModel {
    pub config: ApproximateTimingConfig,
    stats: Rc<RefCell<ApproximateTimingStats>>,
    data_cache: Rc<RefCell<Option<CacheTiming>>>,
}

impl Default for ApproximateCostModel {
    fn default() -> Self { Self::new(ApproximateTimingConfig::default()) }
}

impl ApproximateCostModel {
    pub fn new(config: ApproximateTimingConfig) -> Self {
        Self { config, stats: Rc::new(RefCell::new(ApproximateTimingStats::default())),
            data_cache: Rc::new(RefCell::new(config.data_cache.map(CacheTiming::new))) }
    }
    pub fn stats(&self) -> ApproximateTimingStats { *self.stats.borrow() }
}

impl CostModel for ApproximateCostModel {
    fn lifecycle(&mut self, facts: &LifecycleFacts) -> Result<(), String> {
        if facts.chip != "esp32s3" || facts.cores != 2 {
            return Err("approximate timing supports ESP32-S3 only".into());
        }
        if facts.kind == LifecycleKind::Attach { *self.stats.borrow_mut() = ApproximateTimingStats::default(); }
        if matches!(facts.kind, LifecycleKind::Attach | LifecycleKind::ChipReset) {
            if let Some(cache) = self.data_cache.borrow_mut().as_mut() { cache.reset(); }
        }
        Ok(())
    }

    fn cycles(&mut self, facts: &ExecutionFacts<'_>) -> Result<u32, String> {
        if facts.core >= 2 { return Err("invalid S3 core".into()); }
        let mut stats = self.stats.borrow_mut();
        let mut cycles = self.config.issue.max(1);
        if !matches!(facts.outcome.kind, StepKind::Retired | StepKind::Idle) {
            stats.traps += 1;
            cycles = self.config.trap.max(1);
        } else if let Some(bytes) = facts.outcome.bytes {
            let op = xtensa_lx7::decode(facts.outcome.pc, bytes).op;
            cycles = match op {
                Op::Quos | Op::Quou => 4,
                Op::Rems | Op::Remu => 5,
                Op::Loop | Op::Loopnez | Op::Loopgtz => 5,
                Op::Jx => 6,
                _ => {
                    stats.opcode_fallbacks += 1;
                    if facts.outcome.next_pc != facts.outcome.pc.wrapping_add(facts.outcome.length as u32) {
                        if changes_pc(op) { cycles = self.config.taken_branch.max(1); }
                        else { stats.zero_overhead_loop_edges += 1; }
                    }
                    cycles
                }
            };
        }
        if facts.outcome.control.is_some() { stats.controls += 1; }
        for access in facts.accesses {
            if access.fault.is_some() { stats.faults += 1; }
            match access.address {
                0x3c00_0000..=0x3dff_ffff | 0x4200_0000..=0x43ff_ffff => {
                    if access.kind == MemoryAccessKind::Fetch { stats.external_fetches += 1; }
                    else { stats.external_data += 1; }
                    if access.kind != MemoryAccessKind::Fetch && access.fault.is_none() {
                        if let Some(cache) = self.data_cache.borrow_mut().as_mut() {
                            let cost = cache.access(access.address, u32::from(access.width), access.kind == MemoryAccessKind::Write);
                            cycles = cycles.saturating_add(cost.extra_cycles.min(u32::MAX as u64) as u32);
                            stats.data_cache = cache.stats();
                        } else { cycles = cycles.saturating_add(self.config.external_extra); }
                    } else { cycles = cycles.saturating_add(self.config.external_extra); }
                }
                0x6000_0000..=0x600f_dfff => {
                    stats.mmio += 1;
                    cycles = cycles.saturating_add(self.config.mmio_extra);
                }
                0x3fc8_8000..=0x3fcf_ffff | 0x4037_0000..=0x403d_ffff |
                0x4000_0000..=0x4005_ffff | 0x3ff0_0000..=0x3ff1_ffff |
                0x5000_0000..=0x5000_1fff | 0x600f_e000..=0x600f_ffff => {}
                _ => stats.other_addresses += 1,
            }
        }
        stats.events[facts.core] += 1;
        stats.cycles[facts.core] += u64::from(cycles);
        Ok(cycles)
    }
}

fn changes_pc(op: Op) -> bool {
    use Op::*;
    matches!(op,
        J | Call0 | Call4 | Call8 | Call12 | Callx0 | Callx4 | Callx8 | Callx12 |
        Ret | RetN | Retw | RetwN | Rfe | Rfue | Rfde | Rfwo | Rfwu | Rfi | Rfme |
        Beqz | Bnez | Bltz | Bgez | BeqzN | BnezN | Beqi | Bnei | Blti | Bgei | Bltui | Bgeui |
        Bnone | Beq | Blt | Bltu | Ball | Bbc | Bbci | Bany | Bne | Bge | Bgeu | Bnall | Bbs | Bbsi | Bf | Bt)
}
