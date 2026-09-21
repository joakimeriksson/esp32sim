//! The C6's WiFi side, as far as an unmodified ESP-IDF station has asked for it so far.
//!
//! Nothing here is copied from the S3: the C6 has its own modem subsystem (baseband at
//! `0x600A0000`, the 802.15.4 MAC at `0x600A3000` in `radio.rs`, the WiFi MAC further up), and
//! every register below is here because the closed PHY/WiFi library was seen waiting on it, with
//! the waiting code named. The specimen is `examples/c6-wifi-station`; the plan and the order of
//! work are in `docs/wifi-c6-plan.md`.
use esp_periph::{Device, RegRam, WriteEffect};

/// The modem baseband block (`0x600A0000`). Register RAM, plus the handshakes the PHY library
/// polls:
///
/// - **Channel switch.** The ROM's `freq_chan_en_sw` puts the channel index into `+0xC0` bits
///   13:7 and pulses bit 14 (start); the library's `ram_set_chan_freq_sw_start` then spins on
///   `+0xCC` bit 8 (done). The synthesiser settling is not modelled: the switch is done at the
///   start pulse, and stays done until the next one.
/// - **IQ estimate.** `ram_iq_est_enable` (from `dc_iq_est_new`, which the scan runs on every
///   channel) sets `+0x474` bit 0 (enable) then bit 1 (start) and spins on `+0x4A0` bit 16
///   (done). There is no signal to estimate: done follows the start bit, and the result
///   registers read as written, zero.
pub struct ModemBb {
    ram: RegRam,
    chan_done: bool,
    /// the channel index of the last switch, and how many there were (for `--debug` and tests)
    pub chan_index: u32,
    pub chan_switches: u32,
}
impl Default for ModemBb { fn default() -> Self { Self::new() } }
impl ModemBb {
    pub fn new() -> Self { ModemBb { ram: RegRam::new(), chan_done: false, chan_index: 0, chan_switches: 0 } }
}

const FREQ_CHAN: u32 = 0xc0;
const FREQ_CHAN_START: u32 = 1 << 14;
const FREQ_STATUS: u32 = 0xcc;
const FREQ_STATUS_DONE: u32 = 1 << 8;
const IQ_EST: u32 = 0x474;
const IQ_EST_START: u32 = 1 << 1;
const IQ_EST_STATUS: u32 = 0x4a0;
const IQ_EST_DONE: u32 = 1 << 16;

impl Device for ModemBb {
    fn read(&mut self, off: u32) -> u32 {
        match off {
            FREQ_STATUS => self.ram.read(off) & !FREQ_STATUS_DONE | if self.chan_done { FREQ_STATUS_DONE } else { 0 },
            IQ_EST_STATUS => self.ram.read(off) & !IQ_EST_DONE | if self.ram.read(IQ_EST) & IQ_EST_START != 0 { IQ_EST_DONE } else { 0 },
            _ => self.ram.read(off),
        }
    }
    fn write(&mut self, off: u32, v: u32) -> WriteEffect {
        if off == FREQ_CHAN && v & FREQ_CHAN_START != 0 && self.ram.read(off) & FREQ_CHAN_START == 0 {
            self.chan_index = (v >> 7) & 0x7f;
            self.chan_switches += 1;
            self.chan_done = true;
        }
        self.ram.write(off, v);
        WriteEffect::NONE
    }
}

/// The 802.11 MAC (`0x600A4000`) that the closed `libpp`/`libnet80211` drive
/// (`mac_version:HAL_MAC_ESP32AX_761`). Undocumented. It is a relative of the S3's MAC
/// (`esp32s3::periph::WifiMac`): the same handshakes turn up at shifted offsets, so that model is
/// the map of what to look for, never a source of addresses. Register RAM, plus:
///
/// - **`hal_init`** sets `+0xDDC` bit 1 and spins until bit 0 reads set (the S3 has the same pair
///   at `+0xD14`): the MAC core's ready flag, held set.
pub struct WifiMac { ram: RegRam }
impl Default for WifiMac { fn default() -> Self { Self::new() } }
impl WifiMac {
    pub fn new() -> Self { WifiMac { ram: RegRam::new() } }
}

const MAC_INIT: u32 = 0xddc;
const MAC_INIT_READY: u32 = 1;

impl Device for WifiMac {
    fn read(&mut self, off: u32) -> u32 {
        match off {
            MAC_INIT => self.ram.read(off) | MAC_INIT_READY,
            _ => self.ram.read(off),
        }
    }
    fn write(&mut self, off: u32, v: u32) -> WriteEffect { self.ram.write(off, v); WriteEffect::NONE }
}
