//! `--break ADDR`: stop at a matching PC, including the reset vector and sleeping cores.
use crate::observe::{Ctx, Observer, Wants};
use crate::soc::{Soc, Stop};

pub struct Breakpoints { pub pcs: Vec<u32> }
impl<S: Soc> Observer<S> for Breakpoints {
    fn name(&self) -> &'static str { "breakpoints" }
    fn wants(&self) -> Wants { Wants::INSN | Wants::NO_IDLE_SKIP }
    fn on_insn(&mut self, _cx: &Ctx, _core: usize, _cpu: &S::Core, _bus: &mut S::Bus, pc: u32) -> Option<Stop> {
        if self.pcs.contains(&pc) { Some(Stop::Breakpoint(pc)) } else { None }
    }
}
