//! ESP32-S3 Processor Instruction Extensions (PIE): the `ee.*` SIMD instructions on eight 128-bit Q
//! registers, the 40-bit ACCX and the 2x160-bit QACC accumulators. Encodings come from the TRM's
//! per-instruction "Instruction Word" layouts (`pie_table.rs`, generated), semantics from the
//! "Operation" pseudo-code of the same chapter. 24-bit forms live in op0 = 4, 32-bit forms in op0 = 0xe/0xf.
//! PIE is coprocessor 3: executing any of these with CPENABLE[3] clear raises the CP3-disabled exception,
//! which is how FreeRTOS lazily saves/restores the state per task.
use crate::bus::Bus;
use crate::decode::Insn;
use crate::exec::Trap;
use crate::state::{exc, Cpu};
pub use crate::pie_table::{Role, OPS};

pub struct Field { pub role: Role, pub pieces: &'static [(u8, u8, u8)], pub signed: bool, pub scale: u8 }
pub struct PieInsn { pub name: &'static str, pub len: u8, pub mask: u32, pub value: u32, pub fields: &'static [Field], pub kind: Kind }

#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Mode { None, Ip, Xp, Incp }
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum LdKind { None, Ip, Xp, Ldbc }
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum ArithOp { Adds, Subs, Max, Min, Mul { signed: bool } }
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Cmp { Eq, Lt, Gt }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Vld128(Mode), Vld64 { high: bool, mode: Mode }, LdUsar(Mode), Ldbc { w: u8, mode: Mode }, Ldhbc16, Ldqa { w: u8, signed: bool, mode: Mode },
    LdAccx, StAccx, LdQacc { h: bool, high32: bool }, StQacc { h: bool, high32: bool }, LdUa, StUa,
    Ldf { n: u8, mode: Mode }, Stf { n: u8, mode: Mode }, Vst128(Mode), Vst64 { high: bool, mode: Mode },
    MoviA, MoviQ, ZeroQ, ZeroQacc, ZeroAccx, MovQacc { w: u8, signed: bool },
    Andq, Orq, Xorq, Notq, Vsl32, Vsr32, Slcxxp, Srcxxp, Slci, Srci, SrcQ { qup: bool, ld: Mode }, Srcmb { w: u8 }, SrsAccx,
    Arith { op: ArithOp, w: u8, ld: bool, st: bool }, Vcmp { cmp: Cmp, w: u8 }, Vrelu { w: u8 }, Vprelu { w: u8 }, Vzip { w: u8 }, Vunzip { w: u8 },
    Vmulas { signed: bool, w: u8, accx: bool, ld: LdKind, qup: bool }, Vsmulas { w: u8, ld: bool }, Cmul { store: bool },
    LdQr, StQr, MvQr, Unimpl,
}

/// Decode a PIE instruction word (bytes 0..3 of the fetch, little-endian). Returns the table index.
pub fn decode(w: u32) -> Option<usize> {
    let op0 = w & 0xf;
    if op0 != 4 && op0 != 0xe && op0 != 0xf { return None; }
    let len = if op0 == 4 { 3 } else { 4 };
    let w = if len == 3 { w & 0xff_ffff } else { w };
    OPS.iter().position(|p| p.len == len && (w & p.mask) == p.value)
}

pub struct Ops { v: [i32; 10], r: [Option<Role>; 10], n: usize }
impl Ops {
    pub fn get(&self, role: Role) -> i32 { for k in 0..self.n { if self.r[k] == Some(role) { return self.v[k]; } } 0 }
    pub fn has(&self, role: Role) -> bool { (0..self.n).any(|k| self.r[k] == Some(role)) }
}
pub fn extract(w: u32, p: &PieInsn) -> Ops {
    let mut o = Ops { v: [0; 10], r: [None; 10], n: 0 };
    for f in p.fields {
        let mut val = 0u32; let mut width = 0u32;
        for &(hi, lo, wp) in f.pieces { let n = (hi - lo + 1) as u32; val |= ((w >> wp) & ((1 << n) - 1)) << lo; width = width.max(hi as u32 + 1); }
        let mut v = val as i32;
        if f.signed && width < 32 && (val >> (width - 1)) & 1 == 1 { v = val as i32 - (1i32 << width); }
        v = v.wrapping_mul(f.scale as i32);
        if o.n < 10 { o.v[o.n] = v; o.r[o.n] = Some(f.role); o.n += 1; }
    }
    o
}
/// Windowed AR operands from the extension's typed field descriptions.
pub fn gpr_mask(w: u32, idx: usize) -> u16 {
    let o = extract(w, &OPS[idx]);
    let mut mask = 0;
    for k in 0..o.n {
        if matches!(o.r[k], Some(Role::As | Role::Ad | Role::Au | Role::Ax | Role::Ay | Role::At)) {
            mask |= 1 << o.v[k];
        }
    }
    mask
}

/// Highest AR index used (window-overflow check).
pub fn max_ar(w: u32, idx: usize) -> u8 {
    crate::operands::GprEffects { unclassified: gpr_mask(w, idx), ..Default::default() }.max_ar()
}

pub fn format(w: u32, idx: usize) -> String {
    let p = &OPS[idx]; let o = extract(w, p);
    let mut parts = Vec::new();
    for k in 0..o.n {
        let r = o.r[k].unwrap();
        parts.push(match r {
            Role::Qa | Role::Qa0 | Role::Qa1 | Role::Qm | Role::Qs | Role::Qs0 | Role::Qs1 | Role::Qu | Role::Qu1 | Role::Qv | Role::Qx | Role::Qy | Role::Qz | Role::Qz1 => format!("q{}", o.v[k]),
            Role::As | Role::Ad | Role::Au | Role::Ax | Role::Ay | Role::At => format!("a{}", o.v[k]),
            Role::Fu0 | Role::Fu1 | Role::Fu2 | Role::Fu3 | Role::Fv0 | Role::Fv1 | Role::Fv2 | Role::Fv3 => format!("f{}", o.v[k]),
            _ => format!("{}", o.v[k]),
        });
    }
    if parts.is_empty() { p.name.to_string() } else { format!("{}\t{}", p.name, parts.join(", ")) }
}

// ------------------------------------------------------------------ lane helpers
#[inline] fn lane(q: u128, w: u32, i: u32) -> i64 { let v = ((q >> (w * i)) as u64) & ((1u64 << w) - 1); ((v << (64 - w)) as i64) >> (64 - w) }
#[inline] fn lane_u(q: u128, w: u32, i: u32) -> u64 { ((q >> (w * i)) as u64) & ((1u64 << w) - 1) }
#[inline] fn set_lane(q: &mut u128, w: u32, i: u32, v: u64) { let m = ((1u128 << w) - 1) << (w * i); *q = (*q & !m) | ((v as u128) << (w * i) & m); }
#[inline] fn sat(v: i64, bits: u32) -> i64 { let hi = (1i64 << (bits - 1)) - 1; let lo = -(1i64 << (bits - 1)); v.clamp(lo, hi) }
#[inline] fn usat(v: i64, bits: u32) -> i64 { v.clamp(0, (1i64 << bits) - 1) }
#[inline] fn sext(v: i64, bits: u32) -> i64 { (v << (64 - bits)) >> (64 - bits) }

struct Qacc { lo: u128, hi: u32 }
impl Qacc {
    fn from(a: &[u32; 5]) -> Qacc { Qacc { lo: a[0] as u128 | (a[1] as u128) << 32 | (a[2] as u128) << 64 | (a[3] as u128) << 96, hi: a[4] } }
    fn store(&self, a: &mut [u32; 5]) { a[0] = self.lo as u32; a[1] = (self.lo >> 32) as u32; a[2] = (self.lo >> 64) as u32; a[3] = (self.lo >> 96) as u32; a[4] = self.hi; }
    fn get(&self, lo: u32, w: u32) -> u64 {
        let m = (1u128 << w) - 1;
        let v = if lo + w <= 128 { (self.lo >> lo) & m } else if lo >= 128 { ((self.hi as u128) >> (lo - 128)) & m } else { ((self.lo >> lo) | ((self.hi as u128) << (128 - lo))) & m };
        v as u64
    }
    fn set(&mut self, lo: u32, w: u32, v: u64) {
        let m = (1u128 << w) - 1; let v = (v as u128) & m;
        if lo + w <= 128 { self.lo = (self.lo & !(m << lo)) | (v << lo); }
        else if lo >= 128 { let s = lo - 128; self.hi = (self.hi & !((m as u32) << s)) | ((v as u32) << s); }
        else { let nlo = 128 - lo; self.lo = (self.lo & !(m << lo)) | (v << lo); let mh = (m >> nlo) as u32; self.hi = (self.hi & !mh) | (((v >> nlo) as u32) & mh); }
    }
}
/// QACC lane `i` for element width `w` (8 -> 16 lanes of 20 bits, 16 -> 8 lanes of 40 bits): (half is H, bit offset, lane width).
#[inline] fn qlane(w: u32, i: u32) -> (bool, u32, u32) { if w == 8 { (i >= 8, (i % 8) * 20, 20) } else { (i >= 4, (i % 4) * 40, 40) } }
fn qacc_get(cpu: &Cpu, w: u32, i: u32, signed: bool) -> i64 { let (h, off, lw) = qlane(w, i); let a = Qacc::from(if h { &cpu.qacc_h } else { &cpu.qacc_l }); let v = a.get(off, lw) as i64; if signed { sext(v, lw) } else { v } }
fn qacc_set(cpu: &mut Cpu, w: u32, i: u32, v: i64) { let (h, off, lw) = qlane(w, i); let arr = if h { &mut cpu.qacc_h } else { &mut cpu.qacc_l }; let mut a = Qacc::from(arr); a.set(off, lw, v as u64); a.store(arr); }
fn accx_get(cpu: &Cpu) -> i64 { sext(cpu.accx[0] as i64 | (((cpu.accx[1] & 0xff) as i64) << 32), 40) }
fn accx_set(cpu: &mut Cpu, v: i64) { cpu.accx[0] = v as u32; cpu.accx[1] = ((v >> 32) & 0xff) as u32; }
fn src(qs0: u128, qs1: u128, sh_bytes: u32) -> u128 { let sh = (sh_bytes & 0xf) * 8; if sh == 0 { qs0 } else { (qs0 >> sh) | (qs1 << (128 - sh)) } }

// ------------------------------------------------------------------ memory
fn ld<B: Bus>(cpu: &mut Cpu, bus: &mut B, a: u32, bytes: u32) -> Result<u128, Trap> {
    let a = a & !(bytes - 1);
    let mut v = 0u128;
    let mut k = 0;
    while k < bytes {
        if bytes >= 4 { match bus.read32(a + k) { Ok(x) => v |= (x as u128) << (8 * k), Err(_) => return Err(cpu.raise_mem(exc::LOAD_PROHIBITED, a + k)) } k += 4; }
        else if bytes == 2 { match bus.read16(a) { Ok(x) => v = x as u128, Err(_) => return Err(cpu.raise_mem(exc::LOAD_PROHIBITED, a)) } k += 2; }
        else { match bus.read8(a) { Ok(x) => v = x as u128, Err(_) => return Err(cpu.raise_mem(exc::LOAD_PROHIBITED, a)) } k += 1; }
    }
    Ok(v)
}
fn st<B: Bus>(cpu: &mut Cpu, bus: &mut B, a: u32, bytes: u32, v: u128) -> Result<(), Trap> {
    let a = a & !(bytes - 1);
    let mut k = 0;
    while k < bytes { if bus.write32(a + k, (v >> (8 * k)) as u32).is_err() { return Err(cpu.raise_mem(exc::STORE_PROHIBITED, a + k)); } k += 4; }
    Ok(())
}

pub fn exec<B: Bus>(cpu: &mut Cpu, bus: &mut B, i: &Insn) -> Result<(), Trap> {
    if cpu.cpenable & (1 << 3) == 0 { return Err(cpu.raise(exc::COPROCESSOR0_DISABLED + 3)); }
    let p = &OPS[i.imm as usize];
    let w = i.raw;
    let o = extract(w, p);
    use Role::*;
    macro_rules! q { ($r:expr) => { cpu.qr[o.get($r) as usize & 7] }; }
    macro_rules! setq { ($r:expr, $v:expr) => { { let v: u128 = $v; cpu.qr[o.get($r) as usize & 7] = v; } }; }
    macro_rules! ar { ($r:expr) => { cpu.get_ar(o.get($r) as u8) }; }
    macro_rules! setar { ($r:expr, $v:expr) => { { let v: u32 = $v; cpu.set_ar(o.get($r) as u8, v); } }; }
    macro_rules! post { ($mode:expr) => { match $mode { Mode::Ip => setar!(As, ar!(As).wrapping_add(o.get(Imm) as u32)), Mode::Xp => setar!(As, ar!(As).wrapping_add(ar!(Ad))), Mode::Incp => setar!(As, ar!(As).wrapping_add(16)), Mode::None => {} } }; }
    let sar = cpu.sar & 0x3f;
    match p.kind {
        Kind::Vld128(m) => { let v = ld(cpu, bus, ar!(As), 16)?; setq!(Qu, v); post!(m); }
        Kind::Vld64 { high, mode } => { let v = ld(cpu, bus, ar!(As), 8)? as u64; let mut q = q!(Qu); if high { q = (q & 0xffff_ffff_ffff_ffff) | ((v as u128) << 64); } else { q = (q & !0xffff_ffff_ffff_ffffu128) | v as u128; } setq!(Qu, q); post!(mode); }
        Kind::LdUsar(m) => { let a = ar!(As); let v = ld(cpu, bus, a, 16)?; setq!(Qu, v); cpu.sar_byte = a & 0xf; post!(m); }
        Kind::Ldbc { w, mode } => { let v = ld(cpu, bus, ar!(As), (w / 8) as u32)?; let n = 128 / w as u32; let mut q = 0u128; for k in 0..n { q |= v << (k * w as u32); } setq!(Qu, q); post!(mode); }
        Kind::Ldhbc16 => { let d = ld(cpu, bus, ar!(As), 16)?; let (mut a, mut b) = (0u128, 0u128); for k in 0..4 { let lo = lane_u(d, 16, k); let hi = lane_u(d, 16, 4 + k); set_lane(&mut a, 16, 2 * k, lo); set_lane(&mut a, 16, 2 * k + 1, lo); set_lane(&mut b, 16, 2 * k, hi); set_lane(&mut b, 16, 2 * k + 1, hi); } setq!(Qu, a); setq!(Qu1, b); post!(Mode::Incp); }
        Kind::Ldqa { w, signed, mode } => { let d = ld(cpu, bus, ar!(As), 16)?; let n = 128 / w as u32; for k in 0..n { let v = if signed { lane(d, w as u32, k) } else { lane_u(d, w as u32, k) as i64 }; qacc_set(cpu, w as u32, k, v); } post!(mode); }
        Kind::LdAccx => { let v = ld(cpu, bus, ar!(As), 8)? as u64; accx_set(cpu, sext(v as i64 & 0xff_ffff_ffff, 40)); post!(Mode::Ip); }
        Kind::StAccx => { let v = (accx_get(cpu) as u64) & 0xff_ffff_ffff; st(cpu, bus, ar!(As), 8, v as u128)?; post!(Mode::Ip); }
        Kind::LdQacc { h, high32 } => { let a = ar!(As); if high32 { let v = ld(cpu, bus, a, 4)? as u32; if h { cpu.qacc_h[4] = v; } else { cpu.qacc_l[4] = v; } } else { let v = ld(cpu, bus, a, 16)?; let arr = if h { &mut cpu.qacc_h } else { &mut cpu.qacc_l }; for (k, word) in arr[..4].iter_mut().enumerate() { *word = (v >> (32 * k)) as u32; } } post!(Mode::Ip); }
        Kind::StQacc { h, high32 } => { let arr = if h { cpu.qacc_h } else { cpu.qacc_l }; if high32 { st(cpu, bus, ar!(As), 4, arr[4] as u128)?; } else { let v = arr[0] as u128 | (arr[1] as u128) << 32 | (arr[2] as u128) << 64 | (arr[3] as u128) << 96; st(cpu, bus, ar!(As), 16, v)?; } post!(Mode::Ip); }
        Kind::LdUa => { let v = ld(cpu, bus, ar!(As), 16)?; for k in 0..4 { cpu.ua_state[k] = (v >> (32 * k)) as u32; } post!(Mode::Ip); }
        Kind::StUa => { let u = cpu.ua_state; let v = u[0] as u128 | (u[1] as u128) << 32 | (u[2] as u128) << 64 | (u[3] as u128) << 96; st(cpu, bus, ar!(As), 16, v)?; post!(Mode::Ip); }
        Kind::Ldf { n, mode } => { let v = ld(cpu, bus, ar!(As), 4 * n as u32)?; let roles = [Fu0, Fu1, Fu2, Fu3]; for (k, &role) in roles.iter().enumerate().take(n as usize) { let r = o.get(role) as usize & 15; cpu.fr[r] = (v >> (32 * k)) as u32; } post!(mode); }
        Kind::Stf { n, mode } => { let roles = [Fv0, Fv1, Fv2, Fv3]; let mut v = 0u128; for (k, &role) in roles.iter().enumerate().take(n as usize) { v |= (cpu.fr[o.get(role) as usize & 15] as u128) << (32 * k); } st(cpu, bus, ar!(As), 4 * n as u32, v)?; post!(mode); }
        Kind::Vst128(m) => { st(cpu, bus, ar!(As), 16, q!(Qv))?; post!(m); }
        Kind::Vst64 { high, mode } => { let q = q!(Qv); st(cpu, bus, ar!(As), 8, if high { q >> 64 } else { q & 0xffff_ffff_ffff_ffff })?; post!(mode); }
        Kind::MoviA => { let v = lane_u(q!(Qs), 32, o.get(Sel) as u32 & 3) as u32; setar!(Au, v); }
        Kind::MoviQ => { let mut q = q!(Qu); set_lane(&mut q, 32, o.get(Sel) as u32 & 3, ar!(As) as u64); setq!(Qu, q); }
        Kind::ZeroQ => setq!(Qa, 0),
        Kind::ZeroQacc => { cpu.qacc_h = [0; 5]; cpu.qacc_l = [0; 5]; }
        Kind::ZeroAccx => { cpu.accx = [0; 2]; }
        Kind::MovQacc { w, signed } => { let d = q!(Qs); let n = 128 / w as u32; for k in 0..n { let v = if signed { lane(d, w as u32, k) } else { lane_u(d, w as u32, k) as i64 }; qacc_set(cpu, w as u32, k, v); } }
        Kind::Andq => setq!(Qa, q!(Qx) & q!(Qy)),
        Kind::Orq => setq!(Qa, q!(Qx) | q!(Qy)),
        Kind::Xorq => setq!(Qa, q!(Qx) ^ q!(Qy)),
        Kind::Notq => setq!(Qa, !q!(Qx)),
        Kind::Vsl32 => { let s = q!(Qs); let mut r = 0u128; for k in 0..4 { let v = lane_u(s, 32, k) as u32; set_lane(&mut r, 32, k, if sar >= 32 { 0 } else { (v << sar) as u64 }); } setq!(Qa, r); }
        Kind::Vsr32 => { let s = q!(Qs); let mut r = 0u128; for k in 0..4 { let v = lane_u(s, 32, k) as u32; set_lane(&mut r, 32, k, if sar >= 32 { 0 } else { (v >> sar) as u64 }); } setq!(Qa, r); }
        Kind::Slcxxp | Kind::Slci => { let sh = if p.kind == Kind::Slci { (o.get(Sar) as u32 + 1) * 8 } else { ((ar!(As) & 0xf) + 1) * 8 }; let (q0, q1) = (q!(Qs0), q!(Qs1)); let (lo, hi) = if sh >= 128 { (0, q0) } else { (q0 << sh, (q1 << sh) | (q0 >> (128 - sh))) }; setq!(Qs0, lo); setq!(Qs1, hi); if p.kind == Kind::Slcxxp { post!(Mode::Xp); } }
        Kind::Srcxxp | Kind::Srci => { let sh = if p.kind == Kind::Srci { (o.get(Sar) as u32 + 1) * 8 } else { ((ar!(As) & 0xf) + 1) * 8 }; let (q0, q1) = (q!(Qs0), q!(Qs1)); let (lo, hi) = if sh >= 128 { (q1, 0) } else { ((q0 >> sh) | (q1 << (128 - sh)), q1 >> sh) }; setq!(Qs0, lo); setq!(Qs1, hi); if p.kind == Kind::Srcxxp { post!(Mode::Xp); } }
        Kind::SrcQ { qup, ld: ldm } => {
            let (q0, q1) = (q!(Qs0), q!(Qs1)); let r = src(q0, q1, cpu.sar_byte);
            if ldm == Mode::None { setq!(Qa, r); if qup { setq!(Qs0, q1); } }
            else { setq!(Qs0, r); let v = ld(cpu, bus, ar!(As), 16)?; setq!(Qu, v); post!(ldm); }
        }
        Kind::Srcmb { w } => { let shift = ar!(As) & if w == 8 { 0x1f } else { 0x3f }; let n = 128 / w as u32; let mut r = 0u128; for k in 0..n { let v = qacc_get(cpu, w as u32, k, true) >> shift; qacc_set(cpu, w as u32, k, v); set_lane(&mut r, w as u32, k, sat(v, w as u32) as u64); } setq!(Qu, r); }
        Kind::SrsAccx => { let v = accx_get(cpu) >> (ar!(As) & 0x3f); accx_set(cpu, v); setar!(Au, sat(v, 32) as u32); }
        Kind::Arith { op, w, ld: ldq, st: stq } => {
            let (x, y) = (q!(Qx), q!(Qy)); let wd = w as u32; let n = 128 / wd; let mut r = 0u128;
            for k in 0..n {
                let (a, b) = (lane(x, wd, k), lane(y, wd, k));
                let v = match op {
                    ArithOp::Adds => sat(a + b, wd), ArithOp::Subs => sat(a - b, wd), ArithOp::Max => a.max(b), ArithOp::Min => a.min(b),
                    ArithOp::Mul { signed } => if signed { (a * b) >> sar } else { ((lane_u(x, wd, k) as i64 * lane_u(y, wd, k) as i64) as u64 >> sar) as i64 },
                };
                set_lane(&mut r, wd, k, v as u64);
            }
            let dst = if o.has(Qz) { Qz } else { Qa };
            setq!(dst, r);
            if ldq { let v = ld(cpu, bus, ar!(As), 16)?; setq!(Qu, v); post!(Mode::Incp); }
            if stq { st(cpu, bus, ar!(As), 16, q!(Qv))?; post!(Mode::Incp); }
        }
        Kind::Vcmp { cmp, w } => { let (x, y) = (q!(Qx), q!(Qy)); let wd = w as u32; let n = 128 / wd; let mut r = 0u128; for k in 0..n { let (a, b) = (lane(x, wd, k), lane(y, wd, k)); let t = match cmp { Cmp::Eq => a == b, Cmp::Lt => a < b, Cmp::Gt => a > b }; if t { set_lane(&mut r, wd, k, u64::MAX); } } setq!(Qa, r); }
        Kind::Vrelu { w } => { let wd = w as u32; let n = 128 / wd; let alpha = sext(ar!(Ax) as i64, wd); let sh = ar!(Ay) & if w == 8 { 0x1f } else { 0x3f }; let mut q = q!(Qs); for k in 0..n { let v = lane(q, wd, k); if v <= 0 { set_lane(&mut q, wd, k, ((v * alpha) >> sh) as u64); } } setq!(Qs, q); }
        Kind::Vprelu { w } => { let wd = w as u32; let n = 128 / wd; let (x, y) = (q!(Qx), q!(Qy)); let sh = ar!(Ay) & if w == 8 { 0x1f } else { 0x3f }; let mut r = x; for k in 0..n { let v = lane(x, wd, k); if v <= 0 { set_lane(&mut r, wd, k, ((v * lane(y, wd, k)) >> sh) as u64); } } setq!(Qz, r); }
        Kind::Vzip { w } => { let wd = w as u32; let n = 128 / wd; let (a, b) = (q!(Qs0), q!(Qs1)); let (mut r0, mut r1) = (0u128, 0u128); for k in 0..n / 2 { set_lane(&mut r0, wd, 2 * k, lane_u(a, wd, k)); set_lane(&mut r0, wd, 2 * k + 1, lane_u(b, wd, k)); set_lane(&mut r1, wd, 2 * k, lane_u(a, wd, n / 2 + k)); set_lane(&mut r1, wd, 2 * k + 1, lane_u(b, wd, n / 2 + k)); } setq!(Qs0, r0); setq!(Qs1, r1); }
        Kind::Vunzip { w } => { let wd = w as u32; let n = 128 / wd; let (a, b) = (q!(Qs0), q!(Qs1)); let (mut r0, mut r1) = (0u128, 0u128); for k in 0..n / 2 { set_lane(&mut r0, wd, k, lane_u(a, wd, 2 * k)); set_lane(&mut r0, wd, n / 2 + k, lane_u(b, wd, 2 * k)); set_lane(&mut r1, wd, k, lane_u(a, wd, 2 * k + 1)); set_lane(&mut r1, wd, n / 2 + k, lane_u(b, wd, 2 * k + 1)); } setq!(Qs0, r0); setq!(Qs1, r1); }
        Kind::Vmulas { signed, w, accx, ld: ldk, qup } => {
            let (x, y) = (q!(Qx), q!(Qy)); let wd = w as u32; let n = 128 / wd;
            if accx {
                let mut sum = accx_get(cpu);
                for k in 0..n { sum += if signed { lane(x, wd, k) * lane(y, wd, k) } else { (lane_u(x, wd, k) * lane_u(y, wd, k)) as i64 }; }
                accx_set(cpu, if signed { sat(sum, 40) } else { usat(sum, 40) });
            } else {
                let aw = if w == 8 { 20 } else { 40 };
                for k in 0..n { let prod = if signed { lane(x, wd, k) * lane(y, wd, k) } else { (lane_u(x, wd, k) * lane_u(y, wd, k)) as i64 }; let acc = qacc_get(cpu, wd, k, signed) + prod; qacc_set(cpu, wd, k, if signed { sat(acc, aw) } else { usat(acc, aw) }); }
            }
            match ldk {
                LdKind::None => {}
                LdKind::Ip => { let v = ld(cpu, bus, ar!(As), 16)?; setq!(Qu, v); post!(Mode::Ip); }
                LdKind::Xp => { let v = ld(cpu, bus, ar!(As), 16)?; setq!(Qu, v); post!(Mode::Xp); }
                LdKind::Ldbc => { let v = ld(cpu, bus, ar!(As), (w / 8) as u32)?; let mut q = 0u128; for k in 0..n { q |= v << (k * wd); } setq!(Qu, q); setar!(As, ar!(As).wrapping_add((w / 8) as u32)); }
            }
            if qup { let r = src(q!(Qs0), q!(Qs1), cpu.sar_byte); setq!(Qs0, r); }
        }
        Kind::Vsmulas { w, ld: ldq } => {
            let (x, y) = (q!(Qx), q!(Qy)); let wd = w as u32; let n = 128 / wd; let t = lane(y, wd, o.get(Sel) as u32 % n); let aw = if w == 8 { 20 } else { 40 };
            for k in 0..n { let acc = qacc_get(cpu, wd, k, true) + lane(x, wd, k) * t; qacc_set(cpu, wd, k, sat(acc, aw)); }
            if ldq { let v = ld(cpu, bus, ar!(As), 16)?; setq!(Qu, v); post!(Mode::Incp); }
        }
        Kind::Cmul { store } => {
            let (x, y) = (q!(Qx), q!(Qy)); let sel = o.get(Sel) as u32; let pair = sel / 2; let sub = sel & 1 == 1;
            if pair < 3 {
                let (xr, xi, yr, yi) = (lane(x, 16, 2 * pair), lane(x, 16, 2 * pair + 1), lane(y, 16, 2 * pair), lane(y, 16, 2 * pair + 1));
                let (re, im) = if !sub { ((xr * yr + xi * yi) >> sar, (xi * yr - xr * yi) >> sar) } else { ((xr * yr - xi * yi) >> sar, (xi * yr + xr * yi) >> sar) };
                let dst = if o.has(Qz) { Qz } else { Qa }; let mut r = q!(dst); set_lane(&mut r, 16, 2 * pair, re as u64); set_lane(&mut r, 16, 2 * pair + 1, im as u64); setq!(dst, r);
            }
            if store { st(cpu, bus, ar!(As), 16, q!(Qv))?; } else { let v = ld(cpu, bus, ar!(As), 16)?; setq!(Qu, v); }
            post!(Mode::Xp);
        }
        Kind::LdQr => { let v = ld(cpu, bus, ar!(As).wrapping_add(o.get(Imm) as u32), 16)?; setq!(Qu, v); }
        Kind::StQr => { st(cpu, bus, ar!(As).wrapping_add(o.get(Imm) as u32), 16, q!(Qs))?; }
        Kind::MvQr => { let s = if o.has(Qs) { q!(Qs) } else { q!(Qx) }; let dst = if o.has(Qu) { Qu } else { Qa }; setq!(dst, s); }
        Kind::Unimpl => return Err(Trap::Unimplemented(cpu.pc, w)),
    }
    Ok(())
}
