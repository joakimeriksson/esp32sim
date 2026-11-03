//! The cable: a USB 2.0 host controller on the other end of a device controller's port, at the
//! transaction level. Host software (`esp_soc::usb_host`) hands it one request at a time (power,
//! a bus reset, suspend and resume, a control or bulk transfer); the link carries it out as the
//! packets and bus states a host controller would put on the wire, each at its own emulated
//! time, against the device side (`UsbFunction`), and keeps the result until the software
//! collects it.
//!
//! Timing (in the device clock's ticks; `UsbTiming`):
//! - a transaction takes its bytes plus a fixed protocol overhead at the line rate: 55 bytes at
//!   480 Mbit/s for high speed, 13 at 12 Mbit/s for full speed (USB 2.0 §5.8.4, the bulk
//!   protocol overhead), and does not cross a (micro)frame boundary; transactions of one transfer
//!   follow back to back, so a high-speed bulk transfer moves at most 13 packets of 512 bytes per
//!   125 µs microframe. `UsbTiming::frame_budget` caps the payload per (micro)frame below that.
//! - a NAKed transaction is tried again at the start of the next (micro)frame, as an EHCI host
//!   controller does once a queue head's NAK counter (NakCnt) runs out; a high-speed bulk OUT is
//!   retried the same way (§8.5.1, PING flow control).
//! - SOFs go out every 125 µs at high speed and every 1 ms at full speed while the port is
//!   enabled and not suspended (§8.4.3.1); the device side counts them from when they start.
//! - a bus reset holds SE0 for `reset_ns` (TDRSTR, 50 ms from a root port, §7.1.7.5) and ends in
//!   the speed the device's chirp allows; resume drives K for `resume_ns` (TDRSMDN, 20 ms,
//!   §7.1.7.7). With SOFs stopped the device sees early suspend 3 ms later and suspend 125 µs
//!   after that, the time a high-speed device takes to revert to full speed and tell suspend
//!   from reset (§7.1.7.6).
//!
//! One device hangs on the cable, so tokens reach it whatever its address.
use super::DmaMemory;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Speed { Full, High }

/// Bus conditions the device side sees, other than transactions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BusEvent { SofStart, SofStop, EarlySuspend, Suspend, Resume }

/// A device's answer to an IN token: a data packet with its PID (true for DATA1), NAK or STALL.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum InReply { Data(Vec<u8>, bool), Nak, Stall }

/// A device's handshake to an OUT data packet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handshake { Ack, Nak, Stall }

/// The device end of the cable, at the transaction level.
pub trait UsbFunction {
    /// VBUS from the host.
    fn vbus(&mut self, now: u64, on: bool);
    /// D+ pulled up: the device presents itself.
    fn pullup(&self) -> bool;
    /// The speed a bus reset ends in with a high-speed host (the chirp handshake, §7.1.7.5).
    fn speed_capability(&self) -> Speed;
    /// SE0 began.
    fn bus_reset(&mut self, now: u64);
    /// The reset ended at `speed`.
    fn enumerated(&mut self, now: u64, speed: Speed);
    fn bus_event(&mut self, now: u64, e: BusEvent);
    /// A SETUP and its 8 bytes, which a device always acknowledges (§8.5.3).
    fn setup(&mut self, mem: &mut dyn DmaMemory, pkt: [u8; 8]);
    /// An IN token for endpoint `ep`, the host taking up to `max` bytes.
    fn token_in(&mut self, mem: &mut dyn DmaMemory, ep: u8, max: usize) -> InReply;
    /// An OUT token and its data packet.
    fn token_out(&mut self, mem: &mut dyn DmaMemory, ep: u8, data1: bool, data: &[u8]) -> Handshake;
    /// The device signals remote wakeup (K on the bus while suspended).
    fn remote_wakeup(&self) -> bool { false }
}

/// What host software asks of the link.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Request {
    /// VBUS on (`Attach`) or off (`Detach`)
    Attach,
    Detach,
    /// a bus reset, ending in the device's speed
    Reset,
    /// stop SOFs
    Suspend,
    /// drive resume, then SOFs again
    Resume,
    /// a control transfer on endpoint 0: SETUP, the data stage (`data` going out, or wLength
    /// bytes coming in), the status stage; `mps` is endpoint 0's packet size
    Control { setup: [u8; 8], data: Vec<u8>, mps: u16 },
    /// bulk OUT, with a zero-length packet after a last full one if `zlp`
    BulkOut { ep: u8, data: Vec<u8>, mps: u16, zlp: bool },
    /// bulk IN of up to `len` bytes, ending early at a short packet
    BulkIn { ep: u8, len: usize, mps: u16 },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// what came in (IN transfers, control reads) or nothing
    Done(Vec<u8>),
    /// the endpoint answered STALL, after what had come in
    Stall(Vec<u8>),
    /// the device more than the host asked for, or a packet over the packet size
    Babble(Vec<u8>),
    /// no device on the port (no VBUS, no pull-up, or it went away)
    Disconnected,
    /// a transfer on a port that is not enabled (before a reset, while suspended)
    NotRunning,
    /// host software gave up on it
    Cancelled,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Completion {
    pub outcome: Outcome,
    /// when the last packet or bus state ended
    pub at_ns: u64,
    pub naks: u32,
    pub packets: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct UsbTiming {
    /// line rate at high speed (480 Mbit/s, §7.1.11)
    pub hs_bps: u64,
    /// protocol overhead per high-speed transaction (55 bytes, §5.8.4)
    pub hs_overhead: u32,
    /// payload the host moves in one (micro)frame at most, below what the line allows
    pub frame_budget: Option<u32>,
    pub reset_ns: u64,
    pub resume_ns: u64,
}
impl Default for UsbTiming {
    fn default() -> Self { UsbTiming { hs_bps: 480_000_000, hs_overhead: 55, frame_budget: None, reset_ns: 50_000_000, resume_ns: 20_000_000 } }
}
impl UsbTiming {
    /// A host that moves at most `bytes_per_s` of payload, spread over the microframes.
    pub fn with_rate(mut self, bytes_per_s: u64) -> Self { self.frame_budget = Some((bytes_per_s / 8000).max(1) as u32); self }
}

/// Something that happens to the cable at a set time, whatever the host is doing then.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinkFault {
    /// VBUS goes (the host powers the port off): a transfer in flight ends Disconnected
    VbusOff,
}

/// What a root hub reports about its port.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PortStatus {
    pub power: bool,
    /// a device pulls D+ up
    pub connected: bool,
    /// when `connected` last changed
    pub change_ns: u64,
    /// reset done, SOFs or suspend
    pub enabled: bool,
    pub suspended: bool,
    pub speed: Option<Speed>,
    /// the device signalled remote wakeup while suspended
    pub remote_wakeup: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LinkStats {
    pub transfers: u64, pub transactions: u64, pub naks: u64, pub stalls: u64, pub bytes_out: u64, pub bytes_in: u64,
    pub toggle_errors: u64, pub resets: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Port { Off, Idle, Reset { until: u64 }, Running { speed: Speed }, Suspended { speed: Speed, since: u64, stage: u8 }, Resuming { speed: Speed, until: u64 } }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage { Setup, In, Out, StatusIn, StatusOut }

#[derive(Debug)]
struct Xfer { req: Request, stage: Stage, ep: u8, mps: usize, want: usize, data: Vec<u8>, pos: usize, toggle: bool, zlp: bool, naks: u32, packets: u32 }

pub struct UsbLink {
    pub timing: UsbTiming,
    hz: u64,
    now: u64,
    power: bool,
    pullup: bool,
    change_at: u64,
    port: Port,
    /// a power, reset, suspend or resume request not started yet
    port_op: Option<Request>,
    /// a request in progress: the port state change it waits for, or a transfer
    waiting: Option<Request>,
    xfer: Option<Xfer>,
    done: Option<Completion>,
    next_xact: Option<u64>,
    timer: Option<u64>,
    poke: bool,
    /// (frame index, payload bytes moved in it)
    budget: (u64, u32),
    /// the data PID each bulk endpoint expects next: [OUT, IN]
    toggles: [[bool; 16]; 2],
    remote_wake: bool,
    /// a fault to inject, and when
    fault: Option<(u64, LinkFault)>,
    pub stats: LinkStats,
    pub log: bool,
}

impl UsbLink {
    /// A host on a device clock of `hz` ticks per second, the port unpowered, at time `now`.
    pub fn new(timing: UsbTiming, hz: u64, now: u64) -> Self {
        UsbLink { timing, hz, now, power: false, pullup: false, change_at: now, port: Port::Off, port_op: None, waiting: None, xfer: None, done: None,
                  next_xact: None, timer: None, poke: false, budget: (u64::MAX, 0), toggles: [[false; 16]; 2], remote_wake: false, fault: None, stats: LinkStats::default(), log: false }
    }

    pub fn ns_to_ticks(&self, ns: u64) -> u64 { (ns as u128 * self.hz as u128).div_ceil(1_000_000_000) as u64 }
    pub fn ticks_to_ns(&self, t: u64) -> u64 { (t as u128 * 1_000_000_000 / self.hz as u128) as u64 }
    pub fn now_ns(&self) -> u64 { self.ticks_to_ns(self.now) }

    /// When the link next needs `service`, in ticks.
    pub fn wake_at(&self) -> Option<u64> {
        let xact = self.next_xact.filter(|_| self.xfer.is_some());
        let soon = (self.poke || self.port_op.is_some()).then_some(self.now);
        [xact, self.timer, soon, self.fault.map(|f| f.0)].into_iter().flatten().min()
    }

    /// Advance to `now`; true when work is due.
    pub fn tick_to(&mut self, now: u64) -> bool { self.now = now; self.wake_at().is_some_and(|t| t <= now) }

    pub fn busy(&self) -> bool { self.port_op.is_some() || self.waiting.is_some() || self.xfer.is_some() }

    /// Start `req`; one at a time.
    pub fn submit(&mut self, req: Request) -> Result<(), &'static str> {
        if self.busy() { return Err("the link is busy"); }
        if self.done.is_some() { return Err("a completion has not been collected"); }
        if self.log { eprintln!("[usb-link] t={} submit {}", self.now_ns(), describe(&req)); }
        match req {
            Request::Control { .. } | Request::BulkOut { .. } | Request::BulkIn { .. } => { self.xfer = Some(Xfer::new(req)); self.next_xact = Some(self.now); }
            r => self.port_op = Some(r),
        }
        Ok(())
    }

    /// The finished request's result, once.
    pub fn take_completion(&mut self) -> Option<Completion> { self.done.take() }

    /// Abandon the request in progress.
    pub fn cancel(&mut self) {
        if self.xfer.is_some() || self.waiting.is_some() || self.port_op.is_some() {
            let x = self.xfer.take();
            self.waiting = None; self.port_op = None; self.next_xact = None;
            self.finish_with(self.now, Outcome::Cancelled, x.as_ref());
        }
    }

    pub fn port(&self) -> PortStatus {
        let (enabled, suspended, speed) = match self.port {
            Port::Running { speed } => (true, false, Some(speed)),
            Port::Suspended { speed, .. } | Port::Resuming { speed, .. } => (true, true, Some(speed)),
            _ => (false, false, None),
        };
        PortStatus { power: self.power, connected: self.power && self.pullup, change_ns: self.ticks_to_ns(self.change_at), enabled, suspended, speed, remote_wakeup: self.remote_wake }
    }

    /// The host's data PID for bulk endpoint `ep_addr` back to DATA0: after configuring
    /// (USB 2.0 §9.1.1.5) or CLEAR_FEATURE(ENDPOINT_HALT) (§9.4.5).
    pub fn reset_toggle(&mut self, ep_addr: u8) { self.toggles[(ep_addr >> 7) as usize][(ep_addr & 15) as usize] = false; }
    pub fn reset_toggles(&mut self) { self.toggles = [[false; 16]; 2]; }

    /// Make `fault` happen at `at_ns`, in place of one set before.
    pub fn inject(&mut self, at_ns: u64, fault: LinkFault) { self.fault = Some((self.ns_to_ticks(at_ns), fault)); }

    fn inject_now(&mut self, t: u64, fault: LinkFault, dev: &mut dyn UsbFunction) {
        if self.log { eprintln!("[usb-link] t={} fault: {:?}", self.ticks_to_ns(t), fault); }
        match fault {
            LinkFault::VbusOff => {
                if matches!(self.port, Port::Running { .. }) { dev.bus_event(t, BusEvent::SofStop); }
                self.power = false;
                self.port = Port::Off;
                self.timer = None;
                dev.vbus(t, false);
                if self.xfer.is_some() || self.waiting.is_some() { let x = self.xfer.take(); self.waiting = None; self.next_xact = None; self.finish_with(t, Outcome::Disconnected, x.as_ref()); }
            }
        }
    }

    /// The device's pull-up changed.
    pub fn device_pullup(&mut self, now: u64, on: bool) {
        if on != self.pullup { self.pullup = on; self.change_at = now; self.poke = true; }
    }

    /// The device began remote-wakeup signalling.
    pub fn device_remote_wakeup(&mut self, _now: u64) { if matches!(self.port, Port::Suspended { .. }) { self.remote_wake = true; } }

    /// Carry out everything due by `now`.
    pub fn service(&mut self, now: u64, dev: &mut dyn UsbFunction, mem: &mut dyn DmaMemory) {
        self.now = now;
        while let Some(t) = self.wake_at().filter(|&t| t <= now) {
            if self.fault.is_some_and(|f| f.0 <= t) { let (_, f) = self.fault.take().unwrap(); self.inject_now(t, f, dev); continue; }
            if self.poke { self.poke = false; self.connection(t, dev); continue; }
            if self.timer.is_some_and(|x| x <= t) { self.timer = None; self.port_timer(t, dev); continue; }
            if let Some(r) = self.port_op.take() { self.port_request(t, r, dev); continue; }
            self.next_xact = None;
            self.transact(t, dev, mem);
        }
    }

    fn frame_ticks(&self, speed: Speed) -> u64 { self.ns_to_ticks(if speed == Speed::High { 125_000 } else { 1_000_000 }) }

    /// Ticks the line is busy with a transaction carrying `payload` bytes.
    fn wire_ticks(&self, speed: Speed, payload: usize) -> u64 {
        let (bps, overhead) = if speed == Speed::High { (self.timing.hs_bps, self.timing.hs_overhead as u64) } else { (12_000_000, 13) };
        ((payload as u128 + overhead as u128) * 8 * self.hz as u128).div_ceil(bps as u128) as u64
    }

    fn connection(&mut self, t: u64, dev: &mut dyn UsbFunction) {
        if self.pullup && self.power { return; }
        // The device went away: the port disables, SOFs stop, the transfer fails.
        if matches!(self.port, Port::Running { .. }) { dev.bus_event(t, BusEvent::SofStop); }
        if !matches!(self.port, Port::Off | Port::Idle) {
            self.port = if self.power { Port::Idle } else { Port::Off };
            self.timer = None;
            if self.log { eprintln!("[usb-link] t={} device disconnected", self.ticks_to_ns(t)); }
        }
        if self.xfer.is_some() || self.waiting.is_some() { let x = self.xfer.take(); self.waiting = None; self.finish_with(t, Outcome::Disconnected, x.as_ref()); }
    }

    fn port_request(&mut self, t: u64, r: Request, dev: &mut dyn UsbFunction) {
        match r {
            Request::Attach => {
                self.power = true;
                if self.port == Port::Off { self.port = Port::Idle; }
                dev.vbus(t, true);
                self.finish_with(t, Outcome::Done(Vec::new()), None);
            }
            Request::Detach => {
                if matches!(self.port, Port::Running { .. }) { dev.bus_event(t, BusEvent::SofStop); }
                self.power = false;
                self.port = Port::Off;
                self.timer = None;
                dev.vbus(t, false);
                self.finish_with(t, Outcome::Done(Vec::new()), None);
            }
            Request::Reset => {
                if !(self.power && self.pullup) { self.finish_with(t, Outcome::Disconnected, None); return; }
                if matches!(self.port, Port::Running { .. }) { dev.bus_event(t, BusEvent::SofStop); }
                let until = t + self.ns_to_ticks(self.timing.reset_ns);
                self.port = Port::Reset { until };
                self.timer = Some(until);
                self.remote_wake = false;
                self.stats.resets += 1;
                dev.bus_reset(t);
                self.waiting = Some(Request::Reset);
            }
            Request::Suspend => {
                let Port::Running { speed } = self.port else { self.finish_with(t, Outcome::NotRunning, None); return; };
                dev.bus_event(t, BusEvent::SofStop);
                self.port = Port::Suspended { speed, since: t, stage: 0 };
                self.timer = Some(t + self.ns_to_ticks(3_000_000));
                self.finish_with(t, Outcome::Done(Vec::new()), None);
            }
            Request::Resume => {
                let Port::Suspended { speed, .. } = self.port else { self.finish_with(t, Outcome::NotRunning, None); return; };
                let until = t + self.ns_to_ticks(self.timing.resume_ns);
                self.port = Port::Resuming { speed, until };
                self.timer = Some(until);
                self.remote_wake = false;
                dev.bus_event(t, BusEvent::Resume);
                self.waiting = Some(Request::Resume);
            }
            _ => {}
        }
    }

    fn port_timer(&mut self, t: u64, dev: &mut dyn UsbFunction) {
        match self.port {
            Port::Reset { until } if t >= until => {
                let speed = dev.speed_capability();
                dev.enumerated(t, speed);
                self.port = Port::Running { speed };
                self.reset_toggles();
                dev.bus_event(t, BusEvent::SofStart);
                self.waiting = None;
                self.finish_with(t, Outcome::Done(vec![speed as u8]), None);
            }
            Port::Resuming { speed, until } if t >= until => {
                self.port = Port::Running { speed };
                dev.bus_event(t, BusEvent::SofStart);
                self.waiting = None;
                self.finish_with(t, Outcome::Done(Vec::new()), None);
            }
            Port::Suspended { speed, since, stage } => {
                match stage {
                    0 => { dev.bus_event(t, BusEvent::EarlySuspend); self.timer = Some(since + self.ns_to_ticks(3_125_000)); }
                    1 => dev.bus_event(t, BusEvent::Suspend),
                    _ => {}
                }
                self.port = Port::Suspended { speed, since, stage: stage + 1 };
            }
            _ => {}
        }
    }

    fn finish_with(&mut self, t: u64, outcome: Outcome, x: Option<&Xfer>) {
        let (naks, packets) = x.map_or((0, 0), |x| (x.naks, x.packets));
        if x.is_some() { self.stats.transfers += 1; }
        if self.log { eprintln!("[usb-link] t={} done {:?} ({} packets, {} NAKs)", self.ticks_to_ns(t), short(&outcome), packets, naks); }
        self.done = Some(Completion { outcome, at_ns: self.ticks_to_ns(t), naks, packets });
    }

    fn transact(&mut self, t: u64, dev: &mut dyn UsbFunction, mem: &mut dyn DmaMemory) {
        let Some(mut x) = self.xfer.take() else { return };
        let speed = match self.port {
            Port::Running { speed } => speed,
            _ => { let o = if self.power && self.pullup { Outcome::NotRunning } else { Outcome::Disconnected }; self.finish_with(t, o, Some(&x)); return; }
        };
        let frame = self.frame_ticks(speed);
        let k = t / frame;
        let next_frame = (k + 1) * frame;
        if self.budget.0 != k { self.budget = (k, 0); }
        let payload = match x.stage { Stage::Setup => 8, Stage::Out => (x.data.len() - x.pos).min(x.mps), Stage::StatusOut => 0, _ => x.mps };
        let dur = self.wire_ticks(speed, payload);
        let over_budget = self.timing.frame_budget.is_some_and(|b| self.budget.1 > 0 && self.budget.1 + payload as u32 > b);
        if t + dur > next_frame || over_budget { self.next_xact = Some(next_frame); self.xfer = Some(x); return; }
        self.stats.transactions += 1;
        let end = t + dur;
        let outcome = match x.stage {
            Stage::Setup => {
                let Request::Control { setup, .. } = x.req else { unreachable!() };
                dev.setup(mem, setup);
                x.packets += 1;
                self.budget.1 += 8;
                x.toggle = true;
                x.stage = if x.want > 0 { Stage::In } else if !x.data.is_empty() { Stage::Out } else { Stage::StatusIn };
                None
            }
            Stage::In | Stage::StatusIn => {
                let control = matches!(x.req, Request::Control { .. });
                // a status stage is DATA1 whatever the data stage left (USB 2.0 §8.5.3)
                let expect = if x.stage == Stage::StatusIn { true } else if control { x.toggle } else { self.toggles[1][x.ep as usize & 15] };
                match dev.token_in(mem, x.ep, x.mps) {
                    InReply::Nak => { x.naks += 1; self.stats.naks += 1; self.next_xact = Some(next_frame); self.xfer = Some(x); return; }
                    InReply::Stall => { self.stats.stalls += 1; Some(Outcome::Stall(std::mem::take(&mut x.data))) }
                    InReply::Data(d, pid) => {
                        x.packets += 1;
                        self.budget.1 += d.len() as u32;
                        if d.len() > x.mps { Some(Outcome::Babble(std::mem::take(&mut x.data))) }
                        else if pid != expect {
                            // a packet the host already has: dropped, acknowledged (§8.6.4)
                            self.stats.toggle_errors += 1;
                            if self.log { eprintln!("[usb-link] IN{} DATA{} dropped, expected DATA{}", x.ep, pid as u8, expect as u8); }
                            None
                        } else if x.stage == Stage::StatusIn {
                            // the status of a control write or a no-data request: nothing came in
                            if d.is_empty() { Some(Outcome::Done(Vec::new())) } else { Some(Outcome::Babble(Vec::new())) }
                        } else {
                            if control { x.toggle = !x.toggle } else { self.toggles[1][x.ep as usize & 15] ^= true; }
                            self.stats.bytes_in += d.len() as u64;
                            let short = d.len() < x.mps;
                            x.data.extend_from_slice(&d);
                            if x.data.len() > x.want { Some(Outcome::Babble(std::mem::take(&mut x.data))) }
                            else if short || x.data.len() == x.want {
                                if control { x.stage = Stage::StatusOut; None } else { Some(Outcome::Done(std::mem::take(&mut x.data))) }
                            } else { None }
                        }
                    }
                }
            }
            Stage::Out | Stage::StatusOut => {
                let control = matches!(x.req, Request::Control { .. });
                let (pid, chunk) = if x.stage == Stage::StatusOut { (true, 0..0) } else {
                    (if control { x.toggle } else { self.toggles[0][x.ep as usize & 15] }, x.pos..x.pos + payload)
                };
                match dev.token_out(mem, x.ep, pid, &x.data[chunk]) {
                    Handshake::Nak => { x.naks += 1; self.stats.naks += 1; self.next_xact = Some(next_frame); self.xfer = Some(x); return; }
                    Handshake::Stall => { self.stats.stalls += 1; Some(Outcome::Stall(Vec::new())) }
                    Handshake::Ack => {
                        x.packets += 1;
                        self.budget.1 += payload as u32;
                        if x.stage == Stage::StatusOut { Some(Outcome::Done(std::mem::take(&mut x.data))) } else {
                            if control { x.toggle = !x.toggle } else { self.toggles[0][x.ep as usize & 15] ^= true; }
                            self.stats.bytes_out += payload as u64;
                            x.pos += payload;
                            let zlp_due = x.zlp && payload == x.mps;
                            if x.pos < x.data.len() || (zlp_due && payload > 0) { None }
                            else if control { x.stage = Stage::StatusIn; None }
                            else { Some(Outcome::Done(Vec::new())) }
                        }
                    }
                }
            }
        };
        match outcome {
            Some(o) => self.finish_with(end, o, Some(&x)),
            None => { self.next_xact = Some(end); self.xfer = Some(x); }
        }
    }
}

impl Xfer {
    fn new(req: Request) -> Self {
        let (stage, ep, mps, want, data, zlp) = match &req {
            Request::Control { setup, data, mps } => {
                let wlen = u16::from_le_bytes([setup[6], setup[7]]) as usize;
                let want = if setup[0] & 0x80 != 0 { wlen } else { 0 };
                (Stage::Setup, 0, *mps as usize, want, if want > 0 { Vec::new() } else { data.clone() }, false)
            }
            Request::BulkOut { ep, data, mps, zlp } => (Stage::Out, *ep & 15, *mps as usize, 0, data.clone(), *zlp),
            Request::BulkIn { ep, len, mps } => (Stage::In, *ep & 15, *mps as usize, *len, Vec::new(), false),
            _ => unreachable!(),
        };
        Xfer { req, stage, ep, mps: mps.max(1), want, data, pos: 0, toggle: false, zlp, naks: 0, packets: 0 }
    }
}

fn describe(r: &Request) -> String {
    match r {
        Request::Control { setup, data, .. } => format!("control {:02x?}{}", setup, if data.is_empty() { String::new() } else { format!(" + {} bytes", data.len()) }),
        Request::BulkOut { ep, data, .. } => format!("bulk OUT{} {} bytes", ep, data.len()),
        Request::BulkIn { ep, len, .. } => format!("bulk IN{} up to {} bytes", ep, len),
        r => format!("{:?}", r),
    }
}

fn short(o: &Outcome) -> String {
    match o {
        Outcome::Done(d) => format!("done, {} bytes", d.len()),
        Outcome::Stall(d) => format!("STALL after {} bytes", d.len()),
        Outcome::Babble(d) => format!("babble after {} bytes", d.len()),
        o => format!("{:?}", o),
    }
}
