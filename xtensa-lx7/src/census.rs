//! EX155 ad-hoc fallback census (jit-profile builds only; never timed).
use crate::{Insn, Op};
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Default)]
pub struct Census {
    /// (core, op name, reason) -> interpreted instructions
    pub interp: HashMap<(u8, String, String), u64>,
    /// (core, blocking op) -> interpreted instructions in blocks rejected for that op
    pub blockers: HashMap<(u8, String), u64>,
    /// (core, op name, continues, addr page or 0) -> helper calls from generated code
    pub helpers: HashMap<(u8, String, u32), u64>,
    /// (core, op, region name) slow memory helper calls
    pub slowmem: HashMap<(u8, String, &'static str), u64>,
    pub slowpages: HashMap<(u8, u32), u64>,
    pub interp_total: [u64; 2],
    pub interp_dispatches: [u64; 2],
    pub reason_cache: HashMap<(u8, u32, u16), (String, String)>,
}
static mut CENSUS: Option<Census> = None;
#[allow(static_mut_refs)]
pub fn get() -> &'static mut Census { unsafe { CENSUS.get_or_insert_with(Census::default) } }

pub fn name(i: &Insn) -> String {
    if i.op == Op::Pie { format!("pie:{}", crate::pie::OPS[i.imm as usize].name) }
    else if matches!(i.op, Op::Rsr | Op::Wsr | Op::Xsr | Op::Rur | Op::Wur) { format!("{:?}({})", i.op, i.imm) }
    else { format!("{:?}", i.op) }
}
pub fn region(a: u32) -> &'static str {
    match a {
        0x3C00_0000..=0x3DFF_FFFF => "dbus(flash/psram)",
        0x3FC8_8000..=0x3FCF_FFFF => "dram",
        0x3FF0_0000..=0x3FF1_FFFF => "drom-mask",
        0x4000_0000..=0x4005_FFFF => "irom-mask",
        0x4037_0000..=0x403D_FFFF => "iram",
        0x4200_0000..=0x43FF_FFFF => "ibus(flash)",
        0x5000_0000..=0x5000_1FFF => "rtc-slow",
        0x600F_E000..=0x600F_FFFF => "rtc-fast",
        0x6000_0000..=0x600F_DFFF => "periph",
        _ => "other",
    }
}
pub fn core(cpu: &crate::Cpu) -> u8 { (cpu.prid == 0xABAB) as u8 }

pub fn report() -> String {
    let c = get();
    let mut t = String::new();
    writeln!(t, "[census] interp_total core0={} core1={} interp_dispatches core0={} core1={}", c.interp_total[0], c.interp_total[1], c.interp_dispatches[0], c.interp_dispatches[1]).unwrap();
    for core in 0..2u8 {
        let mut v: Vec<_> = c.interp.iter().filter(|(k, _)| k.0 == core).collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for ((_, op, why), n) in v.iter().take(40) { writeln!(t, "[census] interp core={core} n={n} op={op} why={why}").unwrap(); }
        let mut v: Vec<_> = c.blockers.iter().filter(|(k, _)| k.0 == core).collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for ((_, op), n) in v.iter().take(30) { writeln!(t, "[census] blocker core={core} n={n} why={op}").unwrap(); }
        let mut v: Vec<_> = c.helpers.iter().filter(|(k, _)| k.0 == core).collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for ((_, op, page), n) in v.iter().take(60) { writeln!(t, "[census] helper core={core} n={n} op={op} page={page:04x}").unwrap(); }
        let mut v: Vec<_> = c.slowmem.iter().filter(|(k, _)| k.0 == core).collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for ((_, op, r), n) in v.iter().take(40) { writeln!(t, "[census] slowmem core={core} n={n} op={op} region={r}").unwrap(); }
    }
    t
}
