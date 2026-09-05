//! Observers: analyses that watch a run without living in the scheduler. Each declares what it
//! wants to see; the machine only pays for hooks somebody asked for, and only observers that
//! need every instruction (`INSN`) force single-stepping. Exact trap attribution (`TRAP_PC`)
//! bounds fast-path callbacks to one-instruction fragments.
use crate::soc::{Soc, Stop};
use emu_core::Trap;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Wants(pub u32);
impl Wants {
    pub const NONE: Wants = Wants(0);
    /// `on_insn`/`after_insn` for every instruction — the run single-steps
    pub const INSN: Wants = Wants(1);
    /// `on_block` after every block the fast path executed (full speed)
    pub const BLOCK: Wants = Wants(2);
    /// `on_trap` without fragmenting blocks; PC is the run-entry PC on the fast path.
    pub const TRAP: Wants = Wants(4);
    /// `on_mmio` for every peripheral register access
    pub const MMIO: Wants = Wants(8);
    /// `on_gpio` for every GPIO edge (outputs and inputs)
    pub const GPIO: Wants = Wants(16);
    /// `on_round` after every scheduling round
    pub const ROUND: Wants = Wants(32);
    /// `on_irq_raised` when a core's interrupt input gains a line
    pub const IRQ: Wants = Wants(64);
    /// keep sleeping cores stepping instead of skipping their idle time (changes emulated
    /// timing: an idle core shows as a hot `waiti`; only for observers that count instructions)
    pub const NO_IDLE_SKIP: Wants = Wants(128);
    /// `on_trap` at the exact instruction PC; bounds fast-path runs to one instruction.
    /// Requests trap callbacks itself; combine with `BLOCK` to observe those fragments.
    pub const TRAP_PC: Wants = Wants(256);
    pub fn contains(self, o: Wants) -> bool { self.0 & o.0 != 0 }
}
impl std::ops::BitOr for Wants { type Output = Wants; fn bitor(self, o: Wants) -> Wants { Wants(self.0 | o.0) } }

/// What every hook gets besides its own arguments.
pub struct Ctx<'a> { pub symbols: &'a BTreeMap<u32, String>, pub cycles: u64, pub cpu_hz: u64 }
impl Ctx<'_> {
    pub fn sym(&self, addr: u32) -> String {
        match self.symbols.range(..=addr).next_back() {
            Some((&a, n)) if addr - a < 0x10000 => if a == addr { n.clone() } else { format!("{}+{:#x}", n, addr - a) },
            _ => String::new(),
        }
    }
    pub fn seconds(&self) -> f64 { self.cycles as f64 / self.cpu_hz as f64 }
}

pub trait Observer<S: Soc> {
    fn name(&self) -> &'static str;
    fn wants(&self) -> Wants;
    /// Called before a modeled run. Block callbacks are unavailable on its single-step path.
    fn on_modeled_run(&mut self) {}
    /// Before `pc` executes on `core`. A `Stop` ends the run.
    fn on_insn(&mut self, _cx: &Ctx, _core: usize, _cpu: &S::Core, _bus: &mut S::Bus, _pc: u32) -> Option<Stop> { None }
    /// After the instruction (and its trap, if any) on `core`.
    fn after_insn(&mut self, _cx: &Ctx, _core: usize, _cpu: &S::Core, _bus: &mut S::Bus) -> Option<Stop> { None }
    /// The fast path ran `insns` instructions of the block starting at `pc`.
    fn on_block(&mut self, _cx: &Ctx, _core: usize, _pc: u32, _insns: u32) {}
    /// CPU state is after trap delivery. PC is the run-entry PC unless `TRAP_PC`
    /// (or the single-step path) provides exact instruction attribution.
    fn on_trap(&mut self, _cx: &Ctx, _core: usize, _cpu: &S::Core, _pc: u32, _trap: &Trap) {}
    fn on_irq_raised(&mut self, _cx: &Ctx, _core: usize, _line: u32) {}
    fn on_mmio(&mut self, _cx: &Ctx, _pc: u32, _addr: u32, _value: u32, _write: bool) {}
    fn on_gpio(&mut self, _cycle: u64, _pin: u8, _level: bool) {}
    fn on_round(&mut self, _cx: &Ctx) {}
    /// The end-of-run report (files are written here too).
    fn report(&mut self, _cx: &Ctx) -> String { String::new() }
}
