//! `--profile-blocks`: where the time goes, at full speed. The fast path reports every block it
//! ran (start pc, instructions); attributing those to symbols gives a profile without disabling
//! the JIT or changing timing. Instructions executed on the slow path are counted per pc too.
use crate::observe::{Ctx, Observer, Wants};
use crate::soc::Soc;
use std::collections::HashMap;

pub struct BlockProfile { pub by_pc: HashMap<u32, u64>, pub top: usize, modeled: bool }
impl BlockProfile { pub fn new(top: usize) -> Self { BlockProfile { by_pc: HashMap::new(), top, modeled: false } } }
impl<S: Soc> Observer<S> for BlockProfile {
    fn name(&self) -> &'static str { "profile-blocks" }
    fn wants(&self) -> Wants { Wants::BLOCK }
    fn on_modeled_run(&mut self) { self.modeled = true; }
    fn on_block(&mut self, _cx: &Ctx, _core: usize, pc: u32, insns: u32) { *self.by_pc.entry(pc).or_insert(0) += insns as u64; }
    fn report(&mut self, cx: &Ctx) -> String {
        if self.modeled { return "[profile-blocks] unavailable during modeled single-step execution\n".into(); }
        // fold block starts into their symbols; a block never crosses a symbol in practice
        let mut by_sym: HashMap<String, u64> = HashMap::new();
        let total: u64 = self.by_pc.values().sum();
        for (&pc, &n) in &self.by_pc { let s = cx.sym(pc); let s = s.split('+').next().unwrap_or("").to_string(); *by_sym.entry(if s.is_empty() { format!("{:08x}", pc & !0xfff) } else { s }).or_insert(0) += n; }
        let mut v: Vec<(String, u64)> = by_sym.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut s = format!("[profile-blocks] top {} functions of {} instructions ({} blocks seen)\n", self.top, total, self.by_pc.len());
        for (name, n) in v.iter().take(self.top) { s += &format!("  {:>6.2}%  {:>12}  {}\n", *n as f64 * 100.0 / total.max(1) as f64, n, name); }
        s
    }
}
