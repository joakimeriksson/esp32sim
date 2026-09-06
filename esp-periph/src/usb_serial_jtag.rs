//! USB Serial/JTAG controller: the console on every module with a USB port.
use crate::device::{Device, WriteEffect};
use crate::regram::RegRam;
use emu_core::ClockDomain;

// ------------------------------------------------------------------ USB Serial/JTAG
/// the USB full-speed bulk packet size the Serial/JTAG device presents to the host
pub const USB_PACKET: usize = 64;

pub struct UsbSerialJtag {
    pub connected: bool,          // emulate a host: SOF every 1 ms
    sof_period: u64,
    pub dbg: bool,
    pub sof_count: u64,
    sof_acc: u64,
    pub tx_fifo: Vec<u8>,         // bytes written since last WR_DONE
    pub tx_out: Vec<u8>,          // flushed bytes for the host
    pub rx: std::collections::VecDeque<u8>,
    pub int_raw: u32, pub int_ena: u32, pub conf0: u32,
    /// host bytes not yet in the FIFO, one USB packet each
    pub rx_packets: std::collections::VecDeque<Vec<u8>>,
    ram: RegRam,
}
impl UsbSerialJtag {
    pub fn new(cpu_hz: u64) -> Self { UsbSerialJtag { sof_period: cpu_hz / 4000, connected: true, dbg: false, sof_count: 0, sof_acc: 0, tx_fifo: Vec::new(), tx_out: Vec::new(), rx_packets: Default::default(), rx: Default::default(), int_raw: 0, int_ena: 0, conf0: 0, ram: RegRam::new() } }
    /// advance by CPU cycles; raise SOF interrupt every 1 ms of emulated time
    pub fn tick(&mut self, cycles: u64) { if !self.connected { return; } self.sof_acc += cycles; if self.sof_acc >= self.sof_period { self.sof_acc -= self.sof_period; self.int_raw |= 1 << 1; if self.dbg { self.sof_count += 1; } } /* 4x per tick: HWCDC's tick hook clears it each tick */ }
    pub fn read(&mut self, off: u32) -> u32 {
        match off {
            0x0 => { let b = self.rx.pop_front().map(|b| b as u32).unwrap_or(0); self.present_packet(); b }
            0x4 => (1 << 1) | if self.rx.is_empty() { 0 } else { 1 << 2 },
            0x8 => { if self.dbg { eprintln!("[usb] int_raw read -> {:#x}", self.raw()); } self.raw() }
            0xc => { if self.dbg { eprintln!("[usb] int_st read -> {:#x} (ena {:#x})", self.raw() & self.int_ena, self.int_ena); } self.raw() & self.int_ena }
            0x10 => self.int_ena,
            0x18 => self.conf0,
            _ => self.ram.read(off),
        }
    }
    pub fn write(&mut self, off: u32, v: u32) {
        match off {
            0x0 => { self.tx_fifo.push(v as u8); if self.tx_fifo.len() >= 64 { self.flush(); } }
            0x4 => if v & 1 != 0 { self.flush(); },
            0x10 => { if self.dbg && v != self.int_ena { eprintln!("[usb] int_ena {:#x} -> {:#x} (raw {:#x}, fifo {} bytes)", self.int_ena, v, self.raw(), self.tx_fifo.len()); } self.int_ena = v }
            0x14 => { if self.dbg && v & !2 != 0 { eprintln!("[usb] int_clr {:#x} (raw before {:#x})", v, self.raw()); } self.int_raw &= !v; }
            0x18 => self.conf0 = v,
            _ => self.ram.write(off, v),
        }
    }
    fn flush(&mut self) { if self.dbg { eprintln!("[usb] flush {} bytes: {:?}", self.tx_fifo.len(), String::from_utf8_lossy(&self.tx_fifo)); } self.tx_out.extend_from_slice(&self.tx_fifo); self.tx_fifo.clear(); self.int_raw |= 1 << 3; }
    fn raw(&self) -> u32 { self.int_raw }
    /// Bytes from the host. The interface is USB: a transfer arrives as packets of at most 64
    /// bytes, and `SERIAL_OUT_RECV_PKT` is raised once per packet. Drivers rely on that —
    /// Arduino's HWCDC drains up to 64 bytes per interrupt and never polls the FIFO afterwards,
    /// so a single interrupt for a longer line left its tail stranded and the line's newline
    /// never arrived (found with a 66-byte JSON action; every committed script action is
    /// shorter). The bytes queue as packets and the next packet's interrupt follows the
    /// previous one being read out.
    pub fn host_input(&mut self, data: &[u8]) {
        for chunk in data.chunks(USB_PACKET) { self.rx_packets.push_back(chunk.to_vec()); }
        self.present_packet();
    }
    /// Move the next packet into the FIFO and raise RECV_PKT for it, once the FIFO is empty.
    fn present_packet(&mut self) {
        if self.rx.is_empty() { if let Some(p) = self.rx_packets.pop_front() { self.rx.extend(p); self.int_raw |= 1 << 2; } }
    }
    pub fn irq(&self) -> bool { self.raw() & self.int_ena != 0 }
}

impl Device for UsbSerialJtag {
    fn read(&mut self, off: u32) -> u32 { UsbSerialJtag::read(self, off) }
    fn write(&mut self, off: u32, v: u32) -> WriteEffect { UsbSerialJtag::write(self, off, v); WriteEffect::NONE }
    fn irq_sources(&self) -> u64 { self.irq() as u64 }
    fn clock(&self) -> Option<ClockDomain> { Some(ClockDomain::Cpu) }
    fn tick(&mut self, cycles: u64) { UsbSerialJtag::tick(self, cycles) }
    fn debug(&mut self, on: bool) { self.dbg = on; }
}
