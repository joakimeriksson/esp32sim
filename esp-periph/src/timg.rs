//! Timer group: two 54-bit timers on APB with prescaler and alarm, plus the RTC calibration register.
use crate::device::{Device, WriteEffect};
use crate::regram::RegRam;
use crate::{RTC_SLOW_HZ, XTAL_HZ};
use emu_core::ClockDomain;

// ------------------------------------------------------------------ Timer group (T0/T1 + WDT + RTC calibration)
pub struct TimerGroup {
    ram: RegRam,
    pub t: [Timer; 2],
    pub int_raw: u32, pub int_ena: u32,
}
#[derive(Default, Clone, Copy)]
pub struct Timer { pub config: u32, pub count: u64, pub latch: u64, pub alarm: u64, pub load: u64, pub prescale_acc: u64 }
impl TimerGroup {
    pub fn new() -> Self { TimerGroup { ram: RegRam::new(), t: [Timer::default(); 2], int_raw: 0, int_ena: 0 } }
    pub fn tick(&mut self, apb_ticks: u64) {
        for i in 0..2 {
            let t = &mut self.t[i];
            if t.config & (1 << 31) == 0 { continue; }   // TIMG_T0_EN
            let div = ((t.config >> 13) & 0xffff) as u64;
            let div = if div == 0 { 65536 } else { div };
            t.prescale_acc += apb_ticks;
            let steps = t.prescale_acc / div;
            t.prescale_acc %= div;
            if steps == 0 { continue; }
            let inc = t.config & (1 << 30) != 0;   // TIMG_T0_INCREASE
            let old = t.count;
            t.count = if inc { (t.count + steps) & ((1 << 54) - 1) } else { t.count.wrapping_sub(steps) & ((1 << 54) - 1) };
            if t.config & (1 << 10) != 0 {   // TIMG_T0_ALARM_EN
                let hit = if inc { old < t.alarm && t.count >= t.alarm } else { old > t.alarm && t.count <= t.alarm };
                if hit {
                    self.int_raw |= 1 << i;
                    if t.config & (1 << 29) != 0 { t.count = t.load; } else { t.config &= !(1 << 10); }   // autoreload
                }
            }
        }
    }
    pub fn read(&mut self, off: u32) -> u32 {
        let (i, o) = if off < 0x24 { (0usize, off) } else if off < 0x48 { (1usize, off - 0x24) } else { (2, off) };
        if i < 2 {
            let t = &self.t[i];
            return match o { 0x0 => t.config, 0x4 => t.latch as u32, 0x8 => (t.latch >> 32) as u32, 0x10 => t.alarm as u32, 0x14 => (t.alarm >> 32) as u32, 0x18 => t.load as u32, 0x1c => (t.load >> 32) as u32, _ => 0 };
        }
        match off {
            0x68 => (self.ram.read(off) & !(1 << 15)) | (1 << 15),                       // RTCCALICFG: always RDY
            0x6c => { let n = (self.ram.read(0x68) >> 16) & 0x7fff; ((n as u64 * XTAL_HZ / RTC_SLOW_HZ) as u32) << 7 }   // RTCCALICFG1 value
            0x70 => self.int_ena, 0x74 => self.int_raw, 0x78 => self.int_raw & self.int_ena,
            0xf8 => 0x2006191,
            _ => self.ram.read(off),
        }
    }
    pub fn write(&mut self, off: u32, v: u32) {
        let (i, o) = if off < 0x24 { (0usize, off) } else if off < 0x48 { (1usize, off - 0x24) } else { (2, off) };
        if i < 2 {
            let t = &mut self.t[i];
            match o {
                0x0 => t.config = v,
                0xc => t.latch = t.count,
                0x10 => t.alarm = (t.alarm & !0xffff_ffff) | v as u64, 0x14 => t.alarm = (t.alarm & 0xffff_ffff) | ((v as u64 & 0x3fffff) << 32),
                0x18 => t.load = (t.load & !0xffff_ffff) | v as u64, 0x1c => t.load = (t.load & 0xffff_ffff) | ((v as u64 & 0x3fffff) << 32),
                0x20 => t.count = t.load,
                _ => {}
            }
            return;
        }
        match off {
            0x70 => self.int_ena = v, 0x7c => self.int_raw &= !v,
            _ => self.ram.write(off, v),
        }
    }
}
impl Default for TimerGroup { fn default() -> Self { Self::new() } }

impl Device for TimerGroup {
    fn read(&mut self, off: u32) -> u32 { TimerGroup::read(self, off) }
    fn write(&mut self, off: u32, v: u32) -> WriteEffect { TimerGroup::write(self, off, v); WriteEffect::NONE }
    fn irq_sources(&self) -> u64 { (self.int_raw & self.int_ena & 3) as u64 }
    fn clock(&self) -> Option<ClockDomain> { Some(ClockDomain::Apb) }
    fn tick(&mut self, apb_ticks: u64) { TimerGroup::tick(self, apb_ticks) }
    /// APB ticks until the earliest armed alarm.
    fn has_deadline(&self) -> bool { true }
    fn next_deadline(&self) -> Option<u64> {
        let mut best: Option<u64> = None;
        for t in &self.t {
            if t.config & (1 << 31) == 0 || t.config & (1 << 10) == 0 { continue; }
            let div = ((t.config >> 13) & 0xffff) as u64;
            let div = if div == 0 { 65536 } else { div };
            let steps = if t.config & (1 << 30) != 0 {
                if t.count >= t.alarm { continue } else { t.alarm - t.count }
            } else if t.count <= t.alarm { continue } else { t.count - t.alarm };
            let apb = (steps * div).saturating_sub(t.prescale_acc);
            best = Some(best.map_or(apb, |b| b.min(apb)));
        }
        best
    }
}
