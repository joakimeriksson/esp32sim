//! UART: TX to the host console, RX from it through the 128-byte receive FIFO; the transmit
//! side reads as idle (its FIFO count is 0 and TXFIFO_EMPTY/TX_DONE stay raised).
use std::collections::VecDeque;
use crate::device::{Device, WriteEffect};
use crate::regram::RegRam;

const RX_FIFO_SIZE: usize = 128;
const INT_RXFIFO_FULL: u32 = 1 << 0;
const INT_TXFIFO_EMPTY: u32 = 1 << 1;
const INT_RXFIFO_OVF: u32 = 1 << 4;
const INT_TX_DONE: u32 = 1 << 14;
/// TXFIFO_EMPTY and TX_DONE: always true here, so INT_CLR cannot take them down.
const INT_ALWAYS: u32 = INT_TXFIFO_EMPTY | INT_TX_DONE;

// ------------------------------------------------------------------ UART
pub struct Uart { pub tx_out: Vec<u8>, pub int_raw: u32, pub int_ena: u32, rx: VecDeque<u8>, ram: RegRam }
impl Uart {
    pub fn new() -> Self { Uart { tx_out: Vec::new(), int_raw: INT_ALWAYS, int_ena: 0, rx: VecDeque::new(), ram: RegRam::new() } }
    /// Bytes from the host into the receive FIFO; what does not fit is dropped and flagged RXFIFO_OVF.
    pub fn host_input(&mut self, data: &[u8]) {
        for &b in data {
            if self.rx.len() >= RX_FIFO_SIZE { self.int_raw |= INT_RXFIFO_OVF; break; }
            self.rx.push_back(b);
        }
        self.refresh_rx_full();
    }
    /// CONF1 rxfifo_full_thrhd (bits 9:0); the silicon reset value is 0x60, a driver that wants
    /// every byte sets 1. RXFIFO_FULL is a level here: it stays raised while the count is at or
    /// over the threshold, so a driver that clears it before draining is woken again.
    fn rx_full_threshold(&self) -> usize { ((self.ram.read(0x24) & 0x3ff) as usize).max(1) }
    fn refresh_rx_full(&mut self) { if self.rx.len() >= self.rx_full_threshold() { self.int_raw |= INT_RXFIFO_FULL; } }
    pub fn rx_pending(&self) -> usize { self.rx.len() }
    pub fn read(&mut self, off: u32) -> u32 {
        match off {
            0x0 => self.rx.pop_front().map(|b| b as u32).unwrap_or(0),
            0x4 => self.int_raw,
            0x8 => self.int_raw & self.int_ena,
            0xc => self.int_ena,
            0x1c => 0xe000_c000 | self.rx.len() as u32,   // STATUS: rxfifo_cnt, tx count 0, TXD/RTSN/DSRN idle levels as on silicon
            0x98 => 0,                              // REG_UPDATE (C6 and later): the driver sets it and spins until hardware clears it
            _ => self.ram.read(off),
        }
    }
    pub fn write(&mut self, off: u32, v: u32) {
        match off {
            0x0 => self.tx_out.push(v as u8),
            0xc => self.int_ena = v,
            0x10 => { self.int_raw &= !v | INT_ALWAYS; self.refresh_rx_full(); }
            0x20 => { if v & (1 << 17) != 0 { self.rx.clear(); } self.ram.write(off, v); }   // CONF0 rxfifo_rst
            0x24 => { self.ram.write(off, v); self.refresh_rx_full(); }
            _ => self.ram.write(off, v),
        }
    }
    pub fn irq(&self) -> bool { self.int_raw & self.int_ena != 0 }
}
impl Default for Uart { fn default() -> Self { Self::new() } }

impl Device for Uart {
    fn read(&mut self, off: u32) -> u32 { Uart::read(self, off) }
    fn write(&mut self, off: u32, v: u32) -> WriteEffect { Uart::write(self, off, v); WriteEffect::NONE }
    fn irq_sources(&self) -> u64 { self.irq() as u64 }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The Linux esp32_uart driver's receive path: threshold 1, RXFIFO_FULL enabled, count from
    /// STATUS, pop the FIFO, then INT_CLR — and the line must drop only once the FIFO is empty.
    #[test]
    fn receive_fifo_drives_rxfifo_full_as_a_level() {
        let mut u = Uart::new();
        u.write(0x24, 1); u.write(0xc, INT_RXFIFO_FULL);
        assert!(!u.irq());
        u.host_input(b"ro");
        assert!(u.irq()); assert_eq!(u.read(0x1c) & 0x3ff, 2);
        u.write(0x10, INT_RXFIFO_FULL);          // cleared early: still two bytes waiting
        assert!(u.irq());
        assert_eq!((u.read(0x0), u.read(0x0)), (b'r' as u32, b'o' as u32));
        u.write(0x10, INT_RXFIFO_FULL);
        assert!(!u.irq()); assert_eq!(u.read(0x1c) & 0x3ff, 0); assert_eq!(u.read(0x0), 0);
        assert_eq!(u.read(0x4) & INT_ALWAYS, INT_ALWAYS);
    }
    #[test]
    fn receive_fifo_overflow_is_flagged_and_reset_by_conf0() {
        let mut u = Uart::new();
        u.host_input(&[b'x'; RX_FIFO_SIZE + 3]);
        assert_eq!(u.read(0x1c) & 0x3ff, RX_FIFO_SIZE as u32);
        assert_ne!(u.read(0x4) & INT_RXFIFO_OVF, 0);
        u.write(0x20, 1 << 17);
        assert_eq!(u.read(0x1c) & 0x3ff, 0);
    }
}
