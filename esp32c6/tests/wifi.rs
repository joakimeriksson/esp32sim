//! The C6 WiFi model's handshakes, each as the closed library drives it.
use esp32c6::wifi::{ModemBb, WifiMac};
use esp_periph::Device;

/// The channel switch as the ROM's `freq_chan_en_sw` and the PHY library's
/// `ram_set_chan_freq_sw_start` perform it: index into +0xC0, pulse bit 14, poll +0xCC bit 8.
#[test]
fn channel_switch_is_done_at_the_start_pulse() {
    let mut bb = ModemBb::new();
    assert_eq!(Device::read(&mut bb, 0xcc) & (1 << 8), 0, "no switch was asked for yet");

    let index = 6u32;
    let v = Device::read(&mut bb, 0xc0) & 0xffff_c00f | (index << 7) & 0x3ff0;
    Device::write(&mut bb, 0xc0, v);
    assert_eq!(Device::read(&mut bb, 0xcc) & (1 << 8), 0, "the index alone starts nothing");
    Device::write(&mut bb, 0xc0, v | 1 << 14);
    assert_ne!(Device::read(&mut bb, 0xcc) & (1 << 8), 0, "done");
    Device::write(&mut bb, 0xc0, v);
    assert_ne!(Device::read(&mut bb, 0xcc) & (1 << 8), 0, "still done after the pulse ends");
    assert_eq!((bb.chan_index, bb.chan_switches), (6, 1));

    Device::write(&mut bb, 0xcc, 0xffff_feff);          // the done bit is the hardware's, not a latch of writes
    Device::write(&mut bb, 0xc0, v | 1 << 14);
    Device::write(&mut bb, 0xc0, v | 1 << 14);          // held high: one pulse, one switch
    assert_eq!(bb.chan_switches, 2);
    assert_eq!(Device::read(&mut bb, 0xc0), v | 1 << 14, "the rest of the block is plain register RAM");
}

/// `hal_init`: set +0xDDC bit 1, wait for bit 0.
#[test]
fn mac_core_reports_ready_to_hal_init() {
    let mut mac = WifiMac::new();
    let v = Device::read(&mut mac, 0xddc) | 2;
    Device::write(&mut mac, 0xddc, v);
    assert_eq!(Device::read(&mut mac, 0xddc) & 3, 3, "the request bit is kept, ready reads set");
}

/// `ram_iq_est_enable`: +0x474 bit 0 then bit 1, wait for +0x4A0 bit 16.
#[test]
fn iq_estimate_is_done_while_started() {
    let mut bb = ModemBb::new();
    Device::write(&mut bb, 0x474, 1);
    assert_eq!(Device::read(&mut bb, 0x4a0) & (1 << 16), 0, "enabled, not started");
    Device::write(&mut bb, 0x474, 3);
    assert_ne!(Device::read(&mut bb, 0x4a0) & (1 << 16), 0);
    Device::write(&mut bb, 0x474, 0);
    assert_eq!(Device::read(&mut bb, 0x4a0) & (1 << 16), 0, "the next estimate waits for its own start");
}
