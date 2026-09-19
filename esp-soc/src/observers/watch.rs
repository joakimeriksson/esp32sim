//! `--watch ADDR`: stop when the 32-bit word at an address changes.
use crate::observe::{Ctx, Observer, Wants};
use crate::soc::{Soc, Stop};
use emu_core::Bus;

pub struct Watch { pub addr: u32, pub value: u32 }
impl<S: Soc> Observer<S> for Watch {
    fn name(&self) -> &'static str { "watch" }
    fn wants(&self) -> Wants { Wants::INSN | Wants::NO_IDLE_SKIP }
    fn after_insn(&mut self, _cx: &Ctx, _core: usize, _cpu: &S::Core, bus: &mut S::Bus) -> Option<Stop> {
        if let Ok(v) = bus.read32_unpriced(self.addr) { if v != self.value { let old = self.value; self.value = v; return Some(Stop::Watch(self.addr, old, v)); } }
        None
    }
}
