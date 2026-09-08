//! The hardware random-number register (`WDEV_RND_REG`) of the RISC-V chips, as a device.
//!
//! Real silicon seeds it from radio noise. Here it is an xorshift32 generator whose every read
//! also folds in the CPU cycle count the chip reports through `now`, which is enough for the
//! bootloader's stack canary and for `esp_random` to make progress. The C3 mounts it at
//! APB_CTRL + 0xB0 and the C6 at LPPERI + 0x8; where it sits is the chip's table's business.
//! The S3 keeps its own xorshift64 register inside its SYSTEM-clone block: same job, different
//! sequence.
use crate::device::{Device, WriteEffect};

/// The default seed. Every chip that mounts the model starts from it on every reset, so a run is
/// reproducible from the first read onward.
pub const DEFAULT_SEED: u32 = 0x2545_f491;

pub struct Rng {
    state: u32,
    /// The chip's cycle count at the moment of the access; the mounting chip refreshes it from
    /// its clock tree in `pre_access` before every read of the block.
    pub now: u32,
}

impl Rng {
    /// The generator every chip starts from: [`DEFAULT_SEED`], no cycles yet.
    pub fn new() -> Self { Rng::with_seed(DEFAULT_SEED) }
    /// The same generator from another seed (zero would stay zero: xorshift's fixed point).
    pub fn with_seed(seed: u32) -> Self { Rng { state: seed, now: 0 } }
}

impl Default for Rng {
    fn default() -> Self { Rng::new() }
}

impl Device for Rng {
    /// The next xorshift32 word plus the cycle count, whatever the offset: the block is one register.
    fn read(&mut self, _off: u32) -> u32 {
        self.state ^= self.state << 13; self.state ^= self.state >> 17; self.state ^= self.state << 5;
        self.state.wrapping_add(self.now)
    }
    fn write(&mut self, _off: u32, _v: u32) -> WriteEffect { WriteEffect::NONE }
}
