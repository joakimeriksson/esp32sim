//! Provisional cache timing, not hardware-calibrated behavior.
//! TinyDraw esp32/sdkconfig.defaults:7-10 selects 64-byte lines and describes
//! a 32 KiB data cache. Replacement, associativity and prices below are knobs.
//! Functional writes remain immediate. No DMA coherence, prefetch, cache
//! instructions, MMU alias handling or overlapping misses are modeled.

#[derive(Clone, Copy, Debug)]
pub struct CacheConfig {
    pub capacity_bytes: usize,
    pub line_bytes: u32,
    pub ways: usize,
    pub hit_cycles: u32,
    pub fill_cycles: u32,
    pub writeback_cycles: u32,
}
impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            capacity_bytes: 32768,
            line_bytes: 64,
            ways: 8,
            hit_cycles: 0,
            fill_cycles: 120,
            writeback_cycles: 96,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheAccess {
    pub hits: u64,
    pub line_fills: u64,
    pub dirty_writebacks: u64,
    pub extra_cycles: u64,
}
impl CacheAccess {
    fn add(&mut self, other: Self) {
        self.hits += other.hits;
        self.line_fills += other.line_fills;
        self.dirty_writebacks += other.dirty_writebacks;
        self.extra_cycles += other.extra_cycles;
    }
}
use emu_core::bus::{FastCache, FastCacheLine as Line};

/// Feed both CPUs' data accesses into one shared instance. Only pass cacheable
/// external addresses. Virtual keys misrepresent MMU aliases; callers with
/// physical identity should normalize keys. Use separate I/D instances.
#[derive(Clone, Debug)]
pub struct CacheTiming {
    config: CacheConfig,
    lines: Vec<Line>,
    next_way: Vec<usize>,
    stats: CacheAccess,
}
impl CacheTiming {
    pub fn new(config: CacheConfig) -> Self {
        assert!(config.line_bytes.is_power_of_two());
        assert!(config.ways > 0);
        let bytes_per_set = config.line_bytes as usize * config.ways;
        assert!(config.capacity_bytes >= bytes_per_set);
        assert_eq!(config.capacity_bytes % bytes_per_set, 0);
        let sets = config.capacity_bytes / bytes_per_set;
        Self {
            config,
            lines: vec![Line::default(); sets * config.ways],
            next_way: vec![0; sets],
            stats: CacheAccess::default(),
        }
    }
    pub fn config(&self) -> CacheConfig {
        self.config
    }
    pub fn stats(&self) -> CacheAccess {
        self.stats
    }
    /// Fixed-geometry prototype; other configurations retain the helper path.
    pub fn inline_view(&mut self) -> Option<FastCache> {
        (matches!(self.config.capacity_bytes, 32768 | 65536) && self.config.line_bytes == 64
            && self.config.ways == 8 && self.config.hit_cycles == 0)
            .then_some(FastCache { lines: self.lines.as_mut_ptr(), hits: &mut self.stats.hits })
    }
    pub fn reset(&mut self) {
        self.lines.fill(Line::default());
        self.next_way.fill(0);
        self.stats = CacheAccess::default();
    }
    /// Write allocate/back with round-robin replacement. No allocation on access.
    /// Either use extra_cycles OR price fills/writebacks through a shared memory
    /// resource model. Charging both would double-count external traffic.
    pub fn access(&mut self, address: u32, width: u32, is_write: bool) -> CacheAccess {
        self.access_with_fill_cycles(address, width, is_write, self.config.fill_cycles)
    }

    /// Override only demand-fill readiness; geometry, hit cost and writeback cost are unchanged.
    pub fn access_with_fill_cycles(&mut self, address: u32, width: u32, is_write: bool, fill_cycles: u32) -> CacheAccess {
        let mut result = CacheAccess::default();
        if width == 0 {
            return result;
        }
        let first = address as u64 / self.config.line_bytes as u64;
        let last = (address as u64 + width as u64 - 1) / self.config.line_bytes as u64;
        for line_number in first..=last {
            let tag = line_number as u32;
            let set = tag as usize % self.next_way.len();
            let start = set * self.config.ways;
            let ways = &mut self.lines[start..start + self.config.ways];
            if let Some(line) = ways.iter_mut().find(|line| line.valid != 0 && line.tag == tag) {
                line.dirty |= u32::from(is_write);
                result.hits += 1;
                result.extra_cycles += self.config.hit_cycles as u64;
            } else {
                let victim = ways
                    .iter()
                    .position(|line| line.valid == 0)
                    .unwrap_or(self.next_way[set]);
                let dirty = ways[victim].valid != 0 && ways[victim].dirty != 0;
                result.line_fills += 1;
                result.dirty_writebacks += u64::from(dirty);
                result.extra_cycles += fill_cycles as u64
                    + u64::from(dirty) * self.config.writeback_cycles as u64;
                ways[victim] = Line {
                    tag,
                    dirty: u32::from(is_write),
                    valid: 1,
                };
                self.next_way[set] = (victim + 1) % self.config.ways;
            }
        }
        self.stats.add(result);
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequential_words_amortize_fill() {
        let mut c = CacheTiming::new(CacheConfig::default());
        for a in (0..32768).step_by(4) {
            c.access(a, 4, false);
        }
        assert_eq!(c.stats().line_fills, 512);
        assert_eq!(c.stats().hits, 7680);
        for a in (0..32768).step_by(4) {
            c.access(a, 4, false);
        }
        assert_eq!(c.stats().line_fills, 512);
        assert_eq!(c.stats().dirty_writebacks, 0);
    }
    #[test]
    fn dirty_conflict_costs_more() {
        let mut c = CacheTiming::new(CacheConfig {
            capacity_bytes: 128,
            ways: 2,
            ..CacheConfig::default()
        });
        c.access(0, 4, true);
        c.access(64, 4, false);
        let eviction = c.access(128, 4, false);
        assert_eq!(eviction.dirty_writebacks, 1);
        assert_eq!(eviction.extra_cycles, 216);
        assert_eq!(c.access(192, 4, false).dirty_writebacks, 0);
    }
    #[test]
    fn crossing_line_and_reset() {
        let mut c = CacheTiming::new(CacheConfig::default());
        assert_eq!(c.access(63, 4, true).line_fills, 2);
        assert_eq!(c.access(63, 4, false).hits, 2);
        c.reset();
        assert_eq!(c.stats(), CacheAccess::default());
        assert_eq!(c.access(63, 4, false).line_fills, 2);
    }
}
