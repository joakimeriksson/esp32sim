//! `--coverage [FILE]`: which code ran. Block starts are exact entry points; the report counts
//! them per symbol and, with a file, writes one `addr symbol` line per block start (sorted), a
//! format `diff` and a spreadsheet both take.
use crate::observe::{Ctx, Observer, Wants};
use crate::soc::Soc;
use std::collections::{BTreeMap, BTreeSet};

pub struct Coverage { pub starts: BTreeSet<u32>, pub path: Option<String> }
impl Coverage { pub fn new(path: Option<String>) -> Self { Coverage { starts: BTreeSet::new(), path } } }
impl<S: Soc> Observer<S> for Coverage {
    fn name(&self) -> &'static str { "coverage" }
    fn wants(&self) -> Wants { Wants::BLOCK }
    fn on_block(&mut self, _cx: &Ctx, _core: usize, pc: u32, _insns: u32) { self.starts.insert(pc); }
    fn report(&mut self, cx: &Ctx) -> String {
        let mut per_sym: BTreeMap<String, usize> = BTreeMap::new();
        for &pc in &self.starts { let s = cx.sym(pc); *per_sym.entry(s.split('+').next().unwrap_or("").to_string()).or_insert(0) += 1; }
        let named = per_sym.iter().filter(|(k, _)| !k.is_empty()).count();
        let mut s = format!("[coverage] {} block starts in {} functions\n", self.starts.len(), named);
        if let Some(p) = &self.path {
            let text: String = self.starts.iter().map(|pc| format!("{:08x} {}\n", pc, cx.sym(*pc))).collect();
            match std::fs::write(p, text) { Ok(()) => s += &format!("[coverage] wrote {}\n", p), Err(e) => s += &format!("[coverage] {}: {}\n", p, e) }
        }
        s
    }
}
