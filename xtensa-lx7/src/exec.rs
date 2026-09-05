//! Instruction execution (interpreter) for the LX7 core.
use crate::bus::Bus;
use crate::decode::{decode, Insn, Op};
use crate::state::*;
use emu_core::{CacheOperation, ControlEvent, ControlEventKind, StepKind, StepOutcome, TlbOperation};

pub use emu_core::Trap;

#[inline(always)]
fn bit(w: u32) -> u32 { 1 << (w & (NUM_WINDOWS - 1)) }

impl Cpu {
    /// Rotate the register window by `n` frames (may be negative).
    #[inline(always)]
    fn rotate(&mut self, n: i32) { self.windowbase = (self.windowbase as i32 + n).rem_euclid(NUM_WINDOWS as i32) as u32; }

    /// Take a synchronous exception (XEA2 general vector, or double-exception vector).
    pub fn raise(&mut self, cause: u32) -> Trap {
        self.exccause = cause;
        if self.excm() {
            self.depc = self.pc;
            self.pc = self.vecbase.wrapping_add(vec::DOUBLE);
        } else {
            self.epc[1] = self.pc;
            self.ps |= ps::EXCM;
            self.pc = self.vecbase.wrapping_add(if self.ps & ps::UM != 0 { vec::USER } else { vec::KERNEL });
        }
        self.waiting = false;
        Trap::Exception(cause)
    }

    pub fn raise_mem(&mut self, cause: u32, vaddr: u32) -> Trap { self.excvaddr = vaddr; self.raise(cause) }

    /// Debug exception (BREAK, ICOUNT, IBREAK...). Level 6 on the S3.
    fn raise_debug(&mut self, debugcause: u32) -> Trap {
        self.debugcause = debugcause;
        self.epc[6] = self.pc;
        self.eps[6] = self.ps;
        self.ps = (self.ps & !ps::INTLEVEL_MASK) | 6 | ps::EXCM;
        self.pc = self.vecbase.wrapping_add(vec::DEBUG);
        self.waiting = false;
        Trap::Exception(0x100 | debugcause)
    }

    /// Unmasked pending interrupts (non-zero if `check_interrupts` would deliver one).
    #[inline]
    pub fn check_interrupts_pending(&self) -> u32 {
        let pending = self.interrupt & self.intenable;
        if pending == 0 { return 0; }
        let mask_level = if self.excm() { self.intlevel().max(EXCM_LEVEL) } else { self.intlevel() };
        pending & INT_ABOVE[mask_level as usize]
    }

    /// Deliver the highest-priority enabled pending interrupt, if any is unmasked.
    pub fn check_interrupts(&mut self) -> Option<Trap> {
        let pending = self.interrupt & self.intenable;
        if pending == 0 { return None; }
        let mask_level = if self.excm() { self.intlevel().max(EXCM_LEVEL) } else { self.intlevel() };
        if pending & INT_ABOVE[mask_level as usize] == 0 { return None; }
        let mut best: Option<(u32, u32)> = None;   // (level, irq)
        let mut p = pending;
        while p != 0 {
            let irq = p.trailing_zeros();
            p &= p - 1;
            let level = INT_LEVEL[irq as usize] as u32;
            if level > mask_level && best.is_none_or(|(l, _)| level > l) { best = Some((level, irq)); }
        }
        let (level, irq) = best?;
        self.waiting = false;
        if level == 1 {
            self.exccause = exc::LEVEL1_INTERRUPT;
            self.epc[1] = self.pc;
            self.ps |= ps::EXCM;
            self.pc = self.vecbase.wrapping_add(if self.ps & ps::UM != 0 { vec::USER } else { vec::KERNEL });
        } else {
            self.epc[level as usize] = self.pc;
            self.eps[level as usize] = self.ps;
            self.ps = (self.ps & !ps::INTLEVEL_MASK) | level | ps::EXCM;
            self.pc = self.vecbase.wrapping_add(match level { 2 => vec::LEVEL2, 3 => vec::LEVEL3, 4 => vec::LEVEL4, 5 => vec::LEVEL5, 6 => vec::DEBUG, _ => vec::NMI });
        }
        Some(Trap::Interrupt(irq))
    }

    /// Window overflow check for an instruction touching AR[0..=max_ar].
    pub(crate) fn check_overflow(&mut self, max_ar: u8) -> Option<Trap> {
        if max_ar < 4 || !self.woe() || self.excm() { return None; }
        let w = (max_ar / 4) as u32;
        for n in 1..=w {
            if self.windowstart & bit(self.windowbase + n) != 0 {
                let old = self.windowbase;
                self.rotate(n as i32);
                self.ps = (self.ps & !ps::OWB_MASK) | (old << ps::OWB_SHIFT);
                self.epc[1] = self.pc;
                self.ps |= ps::EXCM;
                // frame m's size = distance to the next live frame (the CALLINC of the call it made)
                let m = self.windowbase;
                let size = if self.windowstart & bit(m + 1) != 0 { 1 } else if self.windowstart & bit(m + 2) != 0 { 2 } else { 3 };
                self.pc = self.vecbase.wrapping_add(match size { 1 => vec::WINDOW_OF4, 2 => vec::WINDOW_OF8, _ => vec::WINDOW_OF12 });
                return Some(Trap::Exception(0x200 + size));
            }
        }
        None
    }

    /// Advance CCOUNT and raise timer interrupts on CCOMPARE match.
    #[inline(always)]
    pub fn advance_ccount(&mut self, cycles: u32) {
        let before = self.ccount;
        self.ccount = self.ccount.wrapping_add(cycles);
        for (&c, &irq) in self.ccompare.iter().zip(&TIMER_INTERRUPT) {
            // matched if c in (before, ccount]
            if c.wrapping_sub(before).wrapping_sub(1) < cycles { self.interrupt |= 1 << irq; }
        }
    }

    pub fn read_sr(&mut self, n: u32) -> Option<u32> {
        Some(match n {
            sr::LBEG => self.lbeg, sr::LEND => self.lend, sr::LCOUNT => self.lcount, sr::SAR => self.sar, sr::BR => self.br,
            sr::SCOMPARE1 => self.scompare1, sr::ACCLO => self.acclo, sr::ACCHI => self.acchi,
            32..=35 => self.m[(n - 32) as usize],
            sr::WINDOWBASE => self.windowbase, sr::WINDOWSTART => self.windowstart,
            sr::IBREAKENABLE => self.ibreakenable, sr::MEMCTL => self.memctl, sr::ATOMCTL => self.atomctl, sr::DDR => self.ddr,
            128 | 129 => self.ibreaka[(n - 128) as usize], 144 | 145 => self.dbreaka[(n - 144) as usize], 160 | 161 => self.dbreakc[(n - 160) as usize],
            sr::CONFIGID0 => self.configid[0], sr::CONFIGID1 => self.configid[1],
            177..=183 => self.epc[(n - 176) as usize], sr::DEPC => self.depc, 194..=199 => self.eps[(n - 192) as usize],
            209..=215 => self.excsave[(n - 208) as usize], sr::CPENABLE => self.cpenable,
            sr::INTERRUPT => self.interrupt, sr::INTENABLE => self.intenable, sr::PS => self.ps, sr::VECBASE => self.vecbase,
            sr::EXCCAUSE => self.exccause, sr::DEBUGCAUSE => self.debugcause, sr::CCOUNT => self.ccount, sr::PRID => self.prid,
            sr::ICOUNT => self.icount, sr::ICOUNTLEVEL => self.icountlevel, sr::EXCVADDR => self.excvaddr,
            240..=242 => self.ccompare[(n - 240) as usize], 244..=247 => self.misc[(n - 244) as usize],
            _ => return None,
        })
    }

    pub fn write_sr(&mut self, n: u32, v: u32) -> Option<()> {
        match n {
            sr::LBEG => self.lbeg = v, sr::LEND => self.lend = v, sr::LCOUNT => self.lcount = v, sr::SAR => self.sar = v & 0x3f, sr::BR => self.br = v & 0xffff,
            sr::SCOMPARE1 => self.scompare1 = v, sr::ACCLO => self.acclo = v, sr::ACCHI => self.acchi = v & 0xff,
            32..=35 => self.m[(n - 32) as usize] = v,
            sr::WINDOWBASE => self.windowbase = v & (NUM_WINDOWS - 1), sr::WINDOWSTART => self.windowstart = v & 0xffff,
            sr::IBREAKENABLE => self.ibreakenable = v & 3, sr::MEMCTL => self.memctl = v, sr::ATOMCTL => self.atomctl = v, sr::DDR => self.ddr = v,
            128 | 129 => self.ibreaka[(n - 128) as usize] = v, 144 | 145 => self.dbreaka[(n - 144) as usize] = v, 160 | 161 => self.dbreakc[(n - 160) as usize] = v,
            177..=183 => self.epc[(n - 176) as usize] = v, sr::DEPC => self.depc = v, 194..=199 => self.eps[(n - 192) as usize] = v,
            209..=215 => self.excsave[(n - 208) as usize] = v, sr::CPENABLE => self.cpenable = v & 0xff,
            sr::INTSET => self.interrupt |= v & INTTYPE_SOFTWARE,
            sr::INTCLEAR => self.interrupt &= !(v & (INTTYPE_SOFTWARE | INTTYPE_EDGE | INTTYPE_PROFILING)),
            sr::INTENABLE => self.intenable = v, sr::PS => self.ps = v & 0x0007_ff3f, sr::VECBASE => self.vecbase = v & !0x3ff,
            sr::EXCCAUSE => self.exccause = v & 0x3f, sr::DEBUGCAUSE => {}, sr::CCOUNT => self.ccount = v, sr::PRID => {},
            sr::ICOUNT => self.icount = v, sr::ICOUNTLEVEL => self.icountlevel = v & 0xf, sr::EXCVADDR => self.excvaddr = v,
            240..=242 => { let i = (n - 240) as usize; self.ccompare[i] = v; self.interrupt &= !(1 << TIMER_INTERRUPT[i]); }
            244..=247 => self.misc[(n - 244) as usize] = v,
            sr::CONFIGID0 | sr::CONFIGID1 => {}
            _ => return None,
        }
        Some(())
    }

    fn read_ur(&self, n: u32) -> Option<u32> {
        Some(match n {
            0 | 1 => self.accx[n as usize], 2..=6 => self.qacc_h[(n - 2) as usize], 7..=11 => self.qacc_l[(n - 7) as usize], 12 => self.gpio_out,
            13 => self.sar_byte, 14 => self.fft_bit_width, 15..=18 => self.ua_state[(n - 15) as usize],
            231 => self.threadptr, 232 => self.fcr, 233 => self.fsr,
            _ => return None,
        })
    }
    fn write_ur(&mut self, n: u32, v: u32) -> Option<()> {
        match n {
            0 | 1 => self.accx[n as usize] = v, 2..=6 => self.qacc_h[(n - 2) as usize] = v, 7..=11 => self.qacc_l[(n - 7) as usize] = v, 12 => self.gpio_out = v,
            13 => self.sar_byte = v, 14 => self.fft_bit_width = v, 15..=18 => self.ua_state[(n - 15) as usize] = v,
            231 => self.threadptr = v, 232 => self.fcr = v & 0x7f, 233 => self.fsr = v & 0xfff80,
            _ => return None,
        }
        Some(())
    }
}

/// Highest AR index an instruction touches (for the window-overflow check).
pub(crate) fn max_ar(i: &Insn) -> u8 {
    i.gpr_effects().max_ar()
}

#[inline(always)]
fn f32b(v: u32) -> f32 { f32::from_bits(v) }

fn sat_i32(v: f32) -> u32 {
    if v.is_nan() { 0x8000_0000 } else if v >= 2147483648.0 { 0x7fff_ffff } else if v <= -2147483648.0 { 0x8000_0000 } else { (v as i32) as u32 }
}
fn sat_u32(v: f32) -> u32 {
    if v.is_nan() || v >= 4294967296.0 { 0xffff_ffff } else if v <= 0.0 { 0 } else { v as u32 }
}

/// Execute one instruction. Returns `Ok(())` when an instruction completed normally.
pub fn step<B: Bus>(cpu: &mut Cpu, bus: &mut B) -> Result<(), Trap> { step_outcome(cpu, bus).result() }

/// Execute one slow-path event and retain the fetch and control facts that cannot be recovered by
/// wrapping the bus.
pub fn step_outcome<B: Bus>(cpu: &mut Cpu, bus: &mut B) -> StepOutcome {
    let pc = cpu.pc;
    if let Some(t) = cpu.check_interrupts() {
        return StepOutcome { pc, next_pc: cpu.pc, bytes: None, length: 0, kind: StepKind::TrapBefore(t), control: None };
    }
    if cpu.waiting {
        cpu.advance_ccount(1);
        return StepOutcome { pc, next_pc: pc, bytes: None, length: 0, kind: StepKind::Idle, control: None };
    }

    let idx = crate::decode::icache_index(pc);
    let e = &cpu.icache[idx];
    let versions = bus.page_versions();
    let hit = e.pc == pc
        && versions.get(e.vidx as usize).copied().unwrap_or(0) == e.ver
        && versions.get(e.vidx2 as usize).copied().unwrap_or(0) == e.ver2;
    let (i, mar, bytes) = if hit { (e.insn, e.max_ar, e.bytes) } else {
        let bytes = match bus.fetch(pc) {
            Ok(b) => b,
            Err(_) => {
                let trap = cpu.raise_mem(exc::IFETCH_ERROR, pc);
                return StepOutcome { pc, next_pc: cpu.pc, bytes: None, length: 0, kind: StepKind::TrapBefore(trap), control: None };
            }
        };
        let vidx = bus.code_page(pc);
        let ver = bus.page_versions().get(vidx as usize).copied().unwrap_or(0);
        let vidx2 = if pc >> emu_core::bus::VPAGE_SHIFT == pc.wrapping_add(3) >> emu_core::bus::VPAGE_SHIFT { vidx } else { bus.code_page(pc.wrapping_add(3)) };
        let ver2 = bus.page_versions().get(vidx2 as usize).copied().unwrap_or(0);
        let i = decode(pc, bytes); let m = max_ar(&i);
        cpu.icache[idx] = crate::decode::CacheEntry { pc, ver, vidx, ver2, vidx2, bytes, insn: i, max_ar: m };
        (i, m, bytes)
    };
    if let Some(t) = cpu.check_overflow(mar) {
        return StepOutcome { pc, next_pc: cpu.pc, bytes: Some(bytes), length: i.len, kind: StepKind::TrapBefore(t), control: None };
    }

    let control = control_event(cpu, &i);
    let r = exec_insn(cpu, bus, &i);
    cpu.insn_count += 1;
    cpu.advance_ccount(1);
    let kind = match r { Ok(()) => StepKind::Retired, Err(trap) => StepKind::TrapDuring(trap) };
    StepOutcome { pc, next_pc: cpu.pc, bytes: Some(bytes), length: i.len, kind, control }
}

fn control_event(cpu: &Cpu, i: &Insn) -> Option<ControlEvent> {
    use CacheOperation as Cache;
    use ControlEventKind::{Cache as CacheEvent, Tlb as TlbEvent};
    use Op::*;
    use TlbOperation as Tlb;
    let kind = match i.op {
        Dpfr => CacheEvent(Cache::DataPrefetchRead), Dpfw => CacheEvent(Cache::DataPrefetchWrite),
        Dpfro => CacheEvent(Cache::DataPrefetchReadOnce), Dpfwo => CacheEvent(Cache::DataPrefetchWriteOnce),
        Dhwb => CacheEvent(Cache::DataHitWriteback), Dhwbi => CacheEvent(Cache::DataHitWritebackInvalidate),
        Dhi => CacheEvent(Cache::DataHitInvalidate), Dii => CacheEvent(Cache::DataIndexInvalidate),
        Dpfl => CacheEvent(Cache::DataPrefetchLocked), Dhu => CacheEvent(Cache::DataHitUnlock),
        Diu => CacheEvent(Cache::DataIndexUnlock), Ipf => CacheEvent(Cache::InstructionPrefetch),
        Ihi => CacheEvent(Cache::InstructionHitInvalidate), Iii => CacheEvent(Cache::InstructionIndexInvalidate),
        Ipfl => CacheEvent(Cache::InstructionPrefetchLocked), Ihu => CacheEvent(Cache::InstructionHitUnlock),
        Iiu => CacheEvent(Cache::InstructionIndexUnlock),
        Ritlb0 => TlbEvent(Tlb::ReadInstructionEntry0), Ritlb1 => TlbEvent(Tlb::ReadInstructionEntry1),
        Rdtlb0 => TlbEvent(Tlb::ReadDataEntry0), Rdtlb1 => TlbEvent(Tlb::ReadDataEntry1),
        Pitlb => TlbEvent(Tlb::ProbeInstruction), Pdtlb => TlbEvent(Tlb::ProbeData),
        Iitlb => TlbEvent(Tlb::InvalidateInstruction), Idtlb => TlbEvent(Tlb::InvalidateData),
        Witlb => TlbEvent(Tlb::WriteInstruction), Wdtlb => TlbEvent(Tlb::WriteData),
        _ => return None,
    };
    let address = match kind {
        CacheEvent(_) => cpu.get_ar(i.s).wrapping_add(i.imm as u32),
        TlbEvent(_) => cpu.get_ar(i.s),
    };
    Some(ControlEvent { kind, address })
}

macro_rules! ld {
    ($cpu:expr, $bus:expr, $f:ident, $addr:expr) => {
        match $bus.$f($addr) { Ok(v) => v as u32, Err(_) => return Err($cpu.raise_mem(exc::LOAD_PROHIBITED, $addr)) }
    };
}
macro_rules! st {
    ($cpu:expr, $bus:expr, $f:ident, $addr:expr, $v:expr) => {
        if $bus.$f($addr, $v).is_err() { return Err($cpu.raise_mem(exc::STORE_PROHIBITED, $addr)); }
    };
}

pub(crate) fn exec_insn<B: Bus>(cpu: &mut Cpu, bus: &mut B, i: &Insn) -> Result<(), Trap> {
    use Op::*;
    let pc = cpu.pc;
    let next = pc.wrapping_add(i.len as u32);
    let (r, s, t) = (i.r, i.s, i.t);
    let imm = i.imm;
    let immu = i.imm as u32;
    // Most instructions fall through; branches set cpu.pc explicitly and return early via `jump!`.
    let mut new_pc = next;
    let mut taken = false;   // set by every control transfer; loop-back only happens on fall-through

    macro_rules! ar { ($n:expr) => { cpu.get_ar($n) }; }
    macro_rules! set { ($n:expr, $v:expr) => { { let v = $v; cpu.set_ar($n, v); } }; }
    macro_rules! br { ($cond:expr) => { if $cond { new_pc = immu; taken = true; } }; }
    macro_rules! cp0 { () => { if cpu.cpenable & 1 == 0 { return Err(cpu.raise(exc::COPROCESSOR0_DISABLED)); } }; }
    macro_rules! fr { ($n:expr) => { f32b(cpu.fr[$n as usize]) }; }
    macro_rules! setf { ($n:expr, $v:expr) => { { let v: f32 = $v; cpu.fr[$n as usize] = v.to_bits(); } }; }
    macro_rules! setb { ($n:expr, $v:expr) => { { if $v { cpu.br |= 1 << $n; } else { cpu.br &= !(1 << $n); } } }; }
    macro_rules! getb { ($n:expr) => { (cpu.br >> $n) & 1 != 0 }; }

    match i.op {
        // ------------------------------------------------------------ control
        Ill | IllN => return Err(cpu.raise(exc::ILLEGAL)),
        Nop | NopN | Isync | Rsync | Esync | Dsync | Excw | Memw | Extw => {}
        Break | BreakN => return Err(cpu.raise_debug(1 << 3)),
        Syscall => return Err(cpu.raise(exc::SYSCALL)),
        Simcall => { cpu.pc = next; return Err(Trap::Simcall); }
        Waiti => { cpu.ps = (cpu.ps & !ps::INTLEVEL_MASK) | (immu & 0xf); cpu.waiting = true; }
        Rsil => { let old = cpu.ps; cpu.ps = (cpu.ps & !ps::INTLEVEL_MASK) | (immu & 0xf); set!(t, old); }
        Rfe => { cpu.ps &= !ps::EXCM; new_pc = cpu.epc[1]; taken = true; }
        Rfue => { cpu.ps &= !ps::EXCM; new_pc = cpu.epc[1]; taken = true; }
        Rfde => { new_pc = if cpu.excm() { cpu.depc } else { cpu.epc[1] }; taken = true; }
        Rfi => { let l = (immu & 0xf) as usize; if !(2..=7).contains(&l) { return Err(cpu.raise(exc::ILLEGAL)); } cpu.ps = cpu.eps[l]; new_pc = cpu.epc[l]; taken = true; }
        Rfme => return Err(Trap::Unimplemented(pc, i.raw)),
        Rfwo => { cpu.windowstart &= !bit(cpu.windowbase); cpu.windowbase = (cpu.ps & ps::OWB_MASK) >> ps::OWB_SHIFT; cpu.ps &= !ps::EXCM; new_pc = cpu.epc[1]; taken = true; }
        Rfwu => { cpu.windowstart |= bit(cpu.windowbase); cpu.windowbase = (cpu.ps & ps::OWB_MASK) >> ps::OWB_SHIFT; cpu.ps &= !ps::EXCM; new_pc = cpu.epc[1]; taken = true; }

        // ------------------------------------------------------------ jumps / calls
        J => { new_pc = immu; taken = true; }
        Jx => { new_pc = ar!(s); taken = true; }
        Call0 => { set!(0, next); new_pc = immu; taken = true; }
        Callx0 => { let tgt = ar!(s); set!(0, next); new_pc = tgt; taken = true; }
        Call4 | Call8 | Call12 | Callx4 | Callx8 | Callx12 => {
            if !cpu.woe() { return Err(cpu.raise(exc::ILLEGAL)); }
            let n = match i.op { Call4 | Callx4 => 1u32, Call8 | Callx8 => 2, _ => 3 };
            let tgt = match i.op { Callx4 | Callx8 | Callx12 => ar!(s), _ => immu };
            cpu.ps = (cpu.ps & !ps::CALLINC_MASK) | (n << ps::CALLINC_SHIFT);
            set!((n * 4) as u8, (n << 30) | (next & 0x3fff_ffff));
            new_pc = tgt; taken = true;
        }
        Entry => {
            if !cpu.woe() || s > 3 { return Err(cpu.raise(exc::ILLEGAL)); }
            let inc = cpu.callinc() as i32;
            let sp = ar!(s).wrapping_sub(immu);
            cpu.rotate(inc);
            cpu.windowstart |= bit(cpu.windowbase);
            set!(s, sp);
        }
        Ret | RetN => { new_pc = ar!(0); taken = true; }
        Retw | RetwN => {
            if !cpu.woe() { return Err(cpu.raise(exc::ILLEGAL)); }
            let a0 = ar!(0);
            let n = a0 >> 30;
            if n == 0 { return Err(cpu.raise(exc::ILLEGAL)); }
            let ret = (a0 & 0x3fff_ffff) | (pc & 0xc000_0000);
            let newbase = (cpu.windowbase.wrapping_sub(n)) & (NUM_WINDOWS - 1);
            if cpu.windowstart & bit(newbase) != 0 {
                cpu.windowstart &= !bit(cpu.windowbase);
                cpu.windowbase = newbase;
                cpu.ps = (cpu.ps & !ps::CALLINC_MASK) | (n << ps::CALLINC_SHIFT);
                new_pc = ret; taken = true;
            } else {
                cpu.ps = (cpu.ps & !ps::OWB_MASK) | (cpu.windowbase << ps::OWB_SHIFT);
                cpu.windowbase = newbase;
                cpu.epc[1] = pc;
                cpu.ps |= ps::EXCM;
                cpu.pc = cpu.vecbase.wrapping_add(match n { 1 => vec::WINDOW_UF4, 2 => vec::WINDOW_UF8, _ => vec::WINDOW_UF12 });
                return Err(Trap::Exception(0x300 + n));
            }
        }
        Movsp => {
            let live = bit(cpu.windowbase.wrapping_sub(1)) | bit(cpu.windowbase.wrapping_sub(2)) | bit(cpu.windowbase.wrapping_sub(3));
            if cpu.windowstart & live == 0 { return Err(cpu.raise(exc::ALLOCA)); }
            set!(t, ar!(s));
        }
        Rotw => cpu.rotate(imm),
        L32e => { let a = ar!(s).wrapping_add(immu); set!(t, ld!(cpu, bus, read32, a)); }
        S32e => { let a = ar!(s).wrapping_add(immu); st!(cpu, bus, write32, a, ar!(t)); }
        S32nb => { let a = ar!(s).wrapping_add(immu); st!(cpu, bus, write32, a, ar!(t)); }

        // ------------------------------------------------------------ branches
        Beqz | BeqzN => br!(ar!(s) == 0),
        Bnez | BnezN => br!(ar!(s) != 0),
        Bltz => br!((ar!(s) as i32) < 0),
        Bgez => br!((ar!(s) as i32) >= 0),
        Beqi => br!(ar!(s) == i.imm2 as u32),
        Bnei => br!(ar!(s) != i.imm2 as u32),
        Blti => br!((ar!(s) as i32) < i.imm2),
        Bgei => br!((ar!(s) as i32) >= i.imm2),
        Bltui => br!(ar!(s) < i.imm2 as u32),
        Bgeui => br!(ar!(s) >= i.imm2 as u32),
        Bnone => br!(ar!(s) & ar!(t) == 0),
        Bany => br!(ar!(s) & ar!(t) != 0),
        Ball => br!(!ar!(s) & ar!(t) == 0),
        Bnall => br!(!ar!(s) & ar!(t) != 0),
        Beq => br!(ar!(s) == ar!(t)),
        Bne => br!(ar!(s) != ar!(t)),
        Blt => br!((ar!(s) as i32) < (ar!(t) as i32)),
        Bge => br!((ar!(s) as i32) >= (ar!(t) as i32)),
        Bltu => br!(ar!(s) < ar!(t)),
        Bgeu => br!(ar!(s) >= ar!(t)),
        Bbc => br!(ar!(s) & (1 << (ar!(t) & 31)) == 0),
        Bbs => br!(ar!(s) & (1 << (ar!(t) & 31)) != 0),
        Bbci => br!(ar!(s) & (1 << i.imm2) == 0),
        Bbsi => br!(ar!(s) & (1 << i.imm2) != 0),
        Bf => br!(!getb!(s)),
        Bt => br!(getb!(s)),

        // ------------------------------------------------------------ loops
        Loop | Loopnez | Loopgtz => {
            let v = ar!(s);
            cpu.lcount = v.wrapping_sub(1);
            cpu.lbeg = next;
            cpu.lend = immu;
            let skip = match i.op { Loopnez => v == 0, Loopgtz => (v as i32) <= 0, _ => false };
            if skip { new_pc = immu; taken = true; }
        }

        // ------------------------------------------------------------ loads / stores
        L8ui => { let a = ar!(s).wrapping_add(immu); set!(t, ld!(cpu, bus, read8, a)); }
        L16ui => { let a = ar!(s).wrapping_add(immu); set!(t, ld!(cpu, bus, read16, a)); }
        L16si => { let a = ar!(s).wrapping_add(immu); set!(t, ld!(cpu, bus, read16, a) as u16 as i16 as i32 as u32); }
        L32i | L32iN | L32ai => { let a = ar!(s).wrapping_add(immu); set!(t, ld!(cpu, bus, read32, a)); }
        L32r => { set!(t, ld!(cpu, bus, read32, immu)); }
        S8i => { let a = ar!(s).wrapping_add(immu); st!(cpu, bus, write8, a, ar!(t) as u8); }
        S16i => { let a = ar!(s).wrapping_add(immu); st!(cpu, bus, write16, a, ar!(t) as u16); }
        S32i | S32iN | S32ri => { let a = ar!(s).wrapping_add(immu); st!(cpu, bus, write32, a, ar!(t)); }
        S32c1i => {
            let a = ar!(s).wrapping_add(immu);
            let old = ld!(cpu, bus, read32, a);
            if old == cpu.scompare1 { st!(cpu, bus, write32, a, ar!(t)); }
            set!(t, old);
        }
        Dpfr | Dpfw | Dpfro | Dpfwo | Dhwb | Dhwbi | Dhi | Dii | Ipf | Ihi | Iii | Ipfl | Ihu | Iiu | Dpfl | Dhu | Diu => {}

        // ------------------------------------------------------------ ALU
        Movi | MoviN => set!(if i.op == Movi { t } else { s }, immu),
        Mov | MovN => set!(t, ar!(s)),
        Add | AddN => set!(r, ar!(s).wrapping_add(ar!(t))),
        Addi | AddiN | Addmi => set!(if i.op == AddiN { r } else { t }, ar!(s).wrapping_add(immu)),
        Sub => set!(r, ar!(s).wrapping_sub(ar!(t))),
        Addx2 => set!(r, (ar!(s) << 1).wrapping_add(ar!(t))),
        Addx4 => set!(r, (ar!(s) << 2).wrapping_add(ar!(t))),
        Addx8 => set!(r, (ar!(s) << 3).wrapping_add(ar!(t))),
        Subx2 => set!(r, (ar!(s) << 1).wrapping_sub(ar!(t))),
        Subx4 => set!(r, (ar!(s) << 2).wrapping_sub(ar!(t))),
        Subx8 => set!(r, (ar!(s) << 3).wrapping_sub(ar!(t))),
        And => set!(r, ar!(s) & ar!(t)),
        Or => set!(r, ar!(s) | ar!(t)),
        Xor => set!(r, ar!(s) ^ ar!(t)),
        Neg => set!(r, (ar!(t) as i32).wrapping_neg() as u32),
        Abs => set!(r, (ar!(t) as i32).wrapping_abs() as u32),
        Extui => { let mask = if i.imm2 >= 32 { u32::MAX } else { (1u32 << i.imm2) - 1 }; set!(r, (ar!(t) >> imm) & mask); }
        Sext => { let b = imm as u32; let v = ar!(s); set!(r, (((v << (31 - b)) as i32) >> (31 - b)) as u32); }
        Clamps => { let b = imm; let v = ar!(s) as i32; let lo = -(1i32 << b); let hi = (1i32 << b) - 1; set!(r, v.clamp(lo, hi) as u32); }
        Min => set!(r, (ar!(s) as i32).min(ar!(t) as i32) as u32),
        Max => set!(r, (ar!(s) as i32).max(ar!(t) as i32) as u32),
        Minu => set!(r, ar!(s).min(ar!(t))),
        Maxu => set!(r, ar!(s).max(ar!(t))),
        Moveqz => if ar!(t) == 0 { set!(r, ar!(s)); },
        Movnez => if ar!(t) != 0 { set!(r, ar!(s)); },
        Movltz => if (ar!(t) as i32) < 0 { set!(r, ar!(s)); },
        Movgez => if (ar!(t) as i32) >= 0 { set!(r, ar!(s)); },
        Movf => if !getb!(t) { set!(r, ar!(s)); },
        Movt => if getb!(t) { set!(r, ar!(s)); },

        // ------------------------------------------------------------ shifts
        Slli => set!(r, ar!(s) << (imm & 31)),
        Srai => set!(r, ((ar!(t) as i32) >> (imm & 31)) as u32),
        Srli => set!(r, ar!(t) >> (imm & 31)),
        Ssr => cpu.sar = ar!(s) & 31,
        Ssl => cpu.sar = 32 - (ar!(s) & 31),
        Ssa8l => cpu.sar = (ar!(s) & 3) * 8,
        Ssa8b => cpu.sar = 32 - (ar!(s) & 3) * 8,
        Ssai => cpu.sar = immu & 31,
        Sll => { let sh = 32u32.wrapping_sub(cpu.sar) & 63; set!(r, if sh >= 32 { 0 } else { ar!(s) << sh }); }
        Srl => set!(r, if cpu.sar >= 32 { 0 } else { ar!(t) >> cpu.sar }),
        Sra => set!(r, if cpu.sar >= 32 { ((ar!(t) as i32) >> 31) as u32 } else { ((ar!(t) as i32) >> cpu.sar) as u32 }),
        Src => { let v = ((ar!(s) as u64) << 32) | ar!(t) as u64; set!(r, (v >> (cpu.sar & 63)) as u32); }
        Nsau => set!(t, ar!(s).leading_zeros()),
        Nsa => { let v = ar!(s) as i32; set!(t, if v == 0 || v == -1 { 31 } else { (v ^ (v >> 31)).leading_zeros() - 1 }); }

        // ------------------------------------------------------------ multiply / divide
        Mull => set!(r, ar!(s).wrapping_mul(ar!(t))),
        Salt => set!(r, ((ar!(s) as i32) < (ar!(t) as i32)) as u32),
        Saltu => set!(r, (ar!(s) < ar!(t)) as u32),
        Muluh => set!(r, ((ar!(s) as u64 * ar!(t) as u64) >> 32) as u32),
        Mulsh => set!(r, (((ar!(s) as i32 as i64) * (ar!(t) as i32 as i64)) >> 32) as u32),
        Mul16u => set!(r, (ar!(s) & 0xffff).wrapping_mul(ar!(t) & 0xffff)),
        Mul16s => set!(r, ((ar!(s) as i16 as i32).wrapping_mul(ar!(t) as i16 as i32)) as u32),
        Quou => { let d = ar!(t); if d == 0 { return Err(cpu.raise(exc::DIVIDE_BY_ZERO)); } set!(r, ar!(s) / d); }
        Remu => { let d = ar!(t); if d == 0 { return Err(cpu.raise(exc::DIVIDE_BY_ZERO)); } set!(r, ar!(s) % d); }
        Quos => { let d = ar!(t) as i32; if d == 0 { return Err(cpu.raise(exc::DIVIDE_BY_ZERO)); } set!(r, (ar!(s) as i32).wrapping_div(d) as u32); }
        Rems => { let d = ar!(t) as i32; if d == 0 { return Err(cpu.raise(exc::DIVIDE_BY_ZERO)); } set!(r, (ar!(s) as i32).wrapping_rem(d) as u32); }

        // ------------------------------------------------------------ special / user registers
        Rsr => { let v = match cpu.read_sr(immu) { Some(v) => v, None => return Err(cpu.raise(exc::ILLEGAL)) }; set!(t, v); }
        Wsr => {
            let v = ar!(t);
            if immu == sr::WINDOWBASE { cpu.windowbase = v & (NUM_WINDOWS - 1); }   // takes effect immediately
            else if cpu.write_sr(immu, v).is_none() { return Err(cpu.raise(exc::ILLEGAL)); }
        }
        Xsr => {
            let v = ar!(t);
            let old = match cpu.read_sr(immu) { Some(v) => v, None => return Err(cpu.raise(exc::ILLEGAL)) };
            cpu.write_sr(immu, v);
            set!(t, old);
        }
        Rur => { if immu == 232 || immu == 233 { cp0!(); } let v = match cpu.read_ur(immu) { Some(v) => v, None => return Err(cpu.raise(exc::ILLEGAL)) }; set!(r, v); }
        Wur => { if immu == 232 || immu == 233 { cp0!(); } let v = ar!(t); if cpu.write_ur(immu, v).is_none() { return Err(cpu.raise(exc::ILLEGAL)); } }
        Rer => set!(t, 0),
        Wer => {}

        // ------------------------------------------------------------ booleans
        Andb => setb!(r, getb!(s) & getb!(t)),
        Andbc => setb!(r, getb!(s) & !getb!(t)),
        Orb => setb!(r, getb!(s) | getb!(t)),
        Orbc => setb!(r, getb!(s) | !getb!(t)),
        Xorb => setb!(r, getb!(s) ^ getb!(t)),
        Any4 => setb!(t, (cpu.br >> (s & 12)) & 0xf != 0),
        All4 => setb!(t, (cpu.br >> (s & 12)) & 0xf == 0xf),
        Any8 => setb!(t, (cpu.br >> (s & 8)) & 0xff != 0),
        All8 => setb!(t, (cpu.br >> (s & 8)) & 0xff == 0xff),

        // ------------------------------------------------------------ TLB / region protection: identity map, no effect
        Ritlb0 | Ritlb1 | Rdtlb0 | Rdtlb1 | Pitlb | Pdtlb => set!(t, 0),
        Iitlb | Idtlb | Witlb | Wdtlb => {}

        // ------------------------------------------------------------ FPU (coprocessor 0)
        Lsi | Lsip => { cp0!(); let a = ar!(s).wrapping_add(immu); let v = ld!(cpu, bus, read32, a); cpu.fr[t as usize] = v; if i.op == Lsip { set!(s, a); } }
        Ssi | Ssip => { cp0!(); let a = ar!(s).wrapping_add(immu); st!(cpu, bus, write32, a, cpu.fr[t as usize]); if i.op == Ssip { set!(s, a); } }
        Lsx | Lsxp => { cp0!(); let a = ar!(s).wrapping_add(ar!(t)); let v = ld!(cpu, bus, read32, a); cpu.fr[r as usize] = v; if i.op == Lsxp { set!(s, a); } }
        Ssx | Ssxp => { cp0!(); let a = ar!(s).wrapping_add(ar!(t)); st!(cpu, bus, write32, a, cpu.fr[r as usize]); if i.op == Ssxp { set!(s, a); } }
        AddS => { cp0!(); setf!(r, fr!(s) + fr!(t)); }
        SubS => { cp0!(); setf!(r, fr!(s) - fr!(t)); }
        MulS => { cp0!(); setf!(r, fr!(s) * fr!(t)); }
        MaddS => { cp0!(); setf!(r, fr!(s).mul_add(fr!(t), fr!(r))); }
        MsubS => { cp0!(); setf!(r, (-fr!(s)).mul_add(fr!(t), fr!(r))); }
        // FP divide/sqrt sequences: like QEMU, the seed/iteration steps are no-ops and the
        // exact result is produced at MKDADJ.S (quotient) / MKSADJ.S (sqrt); ADDEXPM.S moves it.
        MaddnS | DivnS => { cp0!(); }
        RoundS => { cp0!(); let v = fr!(s) * (1u64 << imm) as f32; set!(r, sat_i32(v.round_ties_even())); }
        TruncS => { cp0!(); let v = fr!(s) * (1u64 << imm) as f32; set!(r, sat_i32(v.trunc())); }
        FloorS => { cp0!(); let v = fr!(s) * (1u64 << imm) as f32; set!(r, sat_i32(v.floor())); }
        CeilS => { cp0!(); let v = fr!(s) * (1u64 << imm) as f32; set!(r, sat_i32(v.ceil())); }
        UtruncS => { cp0!(); let v = fr!(s) * (1u64 << imm) as f32; set!(r, sat_u32(v.trunc())); }
        FloatS => { cp0!(); setf!(r, (ar!(s) as i32 as f32) / (1u64 << imm) as f32); }
        UfloatS => { cp0!(); setf!(r, (ar!(s) as f32) / (1u64 << imm) as f32); }
        MovS => { cp0!(); cpu.fr[r as usize] = cpu.fr[s as usize]; }
        AbsS => { cp0!(); cpu.fr[r as usize] = cpu.fr[s as usize] & 0x7fff_ffff; }
        NegS => { cp0!(); cpu.fr[r as usize] = cpu.fr[s as usize] ^ 0x8000_0000; }
        Rfr => { cp0!(); set!(r, cpu.fr[s as usize]); }
        Wfr => { cp0!(); cpu.fr[r as usize] = ar!(s); }
        ConstS => { cp0!(); setf!(r, match imm { 0 => 0.0, 1 => 1.0, 2 => 2.0, 3 => 0.5, _ => 0.0 }); }
        Div0S | Nexp01S | Recip0S | Rsqrt0S | Sqrt0S | AddexpS => { cp0!(); }
        MkdadjS => { cp0!(); setf!(r, fr!(s) / fr!(r)); }
        MksadjS => { cp0!(); setf!(r, fr!(s).sqrt()); }
        AddexpmS => { cp0!(); cpu.fr[r as usize] = cpu.fr[s as usize]; }
        UnS => { cp0!(); setb!(r, fr!(s).is_nan() || fr!(t).is_nan()); }
        OeqS => { cp0!(); setb!(r, fr!(s) == fr!(t)); }
        UeqS => { cp0!(); setb!(r, fr!(s) == fr!(t) || fr!(s).is_nan() || fr!(t).is_nan()); }
        OltS => { cp0!(); setb!(r, fr!(s) < fr!(t)); }
        UltS => { cp0!(); setb!(r, fr!(s) < fr!(t) || fr!(s).is_nan() || fr!(t).is_nan()); }
        OleS => { cp0!(); setb!(r, fr!(s) <= fr!(t)); }
        UleS => { cp0!(); setb!(r, fr!(s) <= fr!(t) || fr!(s).is_nan() || fr!(t).is_nan()); }
        MoveqzS => { cp0!(); if ar!(t) == 0 { cpu.fr[r as usize] = cpu.fr[s as usize]; } }
        MovnezS => { cp0!(); if ar!(t) != 0 { cpu.fr[r as usize] = cpu.fr[s as usize]; } }
        MovltzS => { cp0!(); if (ar!(t) as i32) < 0 { cpu.fr[r as usize] = cpu.fr[s as usize]; } }
        MovgezS => { cp0!(); if (ar!(t) as i32) >= 0 { cpu.fr[r as usize] = cpu.fr[s as usize]; } }
        MovfS => { cp0!(); if !getb!(t) { cpu.fr[r as usize] = cpu.fr[s as usize]; } }
        MovtS => { cp0!(); if getb!(t) { cpu.fr[r as usize] = cpu.fr[s as usize]; } }

        // ------------------------------------------------------------ MAC16
        Mac16 => exec_mac16(cpu, bus, i)?,

        // ------------------------------------------------------------ PIE: not implemented yet
        Pie => { crate::pie::exec(cpu, bus, i)?; }
    }

    // zero-overhead loop back-edge (straight-line only, like hardware fetch semantics)
    if !taken && new_pc == cpu.lend && cpu.lcount != 0 {
        cpu.lcount = cpu.lcount.wrapping_sub(1);
        new_pc = cpu.lbeg;
    }
    cpu.pc = new_pc;
    Ok(())
}

fn exec_mac16<B: Bus>(cpu: &mut Cpu, bus: &mut B, i: &Insn) -> Result<(), Trap> {
    let op1 = (i.raw >> 16) & 0xf;
    let op2 = (i.raw >> 20) & 0xf;
    let half = op1 & 3;           // 0=ll 1=hl 2=lh 3=hh  (first letter = x operand half, second = y)
    let kind = (op1 >> 2) & 3;    // 0=umul 1=mul 2=mula 3=muls
    let (r, s, t) = (i.r, i.s, i.t);
    let hl = |v: u32, hi: bool| -> i64 { if hi { (v >> 16) as u16 as i16 as i64 } else { v as u16 as i16 as i64 } };
    let uhl = |v: u32, hi: bool| -> i64 { if hi { (v >> 16) as i64 } else { (v & 0xffff) as i64 } };
    let acc = |cpu: &Cpu| -> i64 { (((cpu.acchi as i64) << 32 | cpu.acclo as i64) << 24) >> 24 };
    let set_acc = |cpu: &mut Cpu, v: i64| { cpu.acclo = v as u32; cpu.acchi = ((v >> 32) as u32) & 0xff; };
    // operand sources: .aa = AR[s], AR[t]; .ad = AR[s], MR[2+t[2]]; .da = MR[r[2]], AR[t]; .dd = MR[r[2]], MR[2+t[2]]
    let (x, y) = match op2 {
        7 => (cpu.get_ar(s), cpu.get_ar(t)),
        3 => (cpu.get_ar(s), cpu.m[2 + ((t >> 2) & 1) as usize]),
        4..=6 => (cpu.m[((r >> 2) & 1) as usize], cpu.get_ar(t)),
        0..=2 => (cpu.m[((r >> 2) & 1) as usize], cpu.m[2 + ((t >> 2) & 1) as usize]),
        8 | 9 => {   // LDINC / LDDEC mw, as
            let a = if op2 == 8 { cpu.get_ar(s).wrapping_add(4) } else { cpu.get_ar(s).wrapping_sub(4) };
            let v = match bus.read32(a) { Ok(v) => v, Err(_) => return Err(cpu.raise_mem(exc::LOAD_PROHIBITED, a)) };
            cpu.m[(r & 3) as usize] = v;
            cpu.set_ar(s, a);
            cpu.pc = cpu.pc.wrapping_add(3);
            return Ok(());
        }
        _ => return Err(Trap::Unimplemented(cpu.pc, i.raw)),
    };
    let xh = half & 2 != 0;
    let yh = half & 1 != 0;
    let kind_eff = if op2 == 0 || op2 == 1 || op2 == 4 || op2 == 5 { 2 } else { kind };   // *.ldinc/lddec forms are always MULA
    let prod = if kind_eff == 0 { uhl(x, xh) * uhl(y, yh) } else { hl(x, xh) * hl(y, yh) };
    let v = match kind_eff { 0 | 1 => prod, 2 => acc(cpu) + prod, _ => acc(cpu) - prod };
    set_acc(cpu, v);
    if op2 == 0 || op2 == 1 || op2 == 4 || op2 == 5 {   // with LDINC/LDDEC of MR[r&3]? (mw = r field) — load after multiply
        let a = if op2 == 0 || op2 == 4 { cpu.get_ar(s).wrapping_add(4) } else { cpu.get_ar(s).wrapping_sub(4) };
        let ld = match bus.read32(a) { Ok(v) => v, Err(_) => return Err(cpu.raise_mem(exc::LOAD_PROHIBITED, a)) };
        cpu.m[(r & 3) as usize] = ld;
        cpu.set_ar(s, a);
    }
    cpu.pc = cpu.pc.wrapping_add(3);
    Ok(())
}
