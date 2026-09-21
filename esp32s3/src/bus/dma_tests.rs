use super::*;

const DESC: u32 = DRAM_LOW;
const INPUT: u32 = DRAM_LOW + 0x100;
const RX_DESC: u32 = DRAM_LOW + 0x200;
const OUTPUT: u32 = DRAM_LOW + 0x300;

fn bus_with_out(peripheral: u32, length: u32, next: u32) -> SocBus {
    let mut bus = SocBus::new(1024, 1024, [0; 6]);
    bus.write32(DESC, (1 << 31) | (length << 12) | length).unwrap();
    bus.write32(DESC + 4, INPUT).unwrap();
    bus.write32(DESC + 8, next).unwrap();
    let ch = &mut bus.periph.gdma.out[0];
    ch.running = true; ch.peri_sel = peripheral; ch.desc = DESC;
    bus
}

fn arm_aes_input(bus: &mut SocBus, capacity: u32, next: u32) {
    bus.write32(RX_DESC, (1 << 31) | capacity).unwrap();
    bus.write32(RX_DESC + 4, OUTPUT).unwrap();
    bus.write32(RX_DESC + 8, next).unwrap();
    let ch = &mut bus.periph.gdma.inp[0];
    ch.running = true; ch.peri_sel = 6; ch.desc = RX_DESC;
}

#[test]
fn empty_streaming_descriptor_cycles_raise_error_and_stop() {
    for peripheral in [3, 4, 5] {
        let mut bus = bus_with_out(peripheral, 0, DESC);
        match peripheral {
            3 | 4 => {
                let i2s = if peripheral == 3 { &mut bus.periph.i2s0 } else { &mut bus.periph.i2s1 };
                i2s.write(0x2c, 15 << 13); i2s.write(0x54, (1 << 16) | 3); i2s.write(0x24, 4); i2s.sample_rate = crate::periph::CPU_HZ as u32;
                bus.dma_i2s_step(1);
            }
            _ => {
                bus.periph.lcd_cam.lcd_user = 1 << 27;
                bus.periph.lcd_cam.lcd_ctrl = 1 << 31;
                bus.periph.lcd_cam.lcd_ctrl1 = 511 << 8;
                bus.dma_lcd_step(1);
            }
        }
        assert!(!bus.periph.gdma.out[0].running, "peripheral {peripheral}");
        assert_ne!(bus.periph.gdma.out[0].int_raw & (1 << 2), 0);
        assert!(bus.irq_dirty);
    }
}

#[test]
fn finite_i2s_work_can_read_a_nonempty_ring() {
    let mut bus = bus_with_out(3, 4, DESC);
    bus.write32(INPUT, 0x1234).unwrap();
    bus.periph.i2s0.write(0x2c, 15 << 13);
    bus.periph.i2s0.write(0x54, (1 << 16) | 3);
    bus.periph.i2s0.write(0x24, 4);
    bus.periph.i2s0.sample_rate = crate::periph::CPU_HZ as u32;
    bus.dma_i2s_step(2);
    assert!(bus.periph.gdma.out[0].running);
    assert_eq!(bus.periph.gdma.out[0].int_raw & (1 << 2), 0);
    assert_eq!(bus.periph.i2s0.pcm, [0x1234; 2]);
}

#[test]
fn crypto_cycles_do_not_hash_or_encrypt_partial_input() {
    for peripheral in [6, 7] {
        for length in [0, 16] {
            let mut bus = bus_with_out(peripheral, length, DESC);
            if peripheral == 6 {
                arm_aes_input(&mut bus, 64, 0);
                bus.aes_dma_step();
                assert_eq!(bus.periph.aes.blocks, 0);
                assert_eq!(bus.periph.aes.int_raw, 0);
            } else {
                bus.periph.sha.block_num = 1;
                bus.sha_dma_step();
                assert_eq!(bus.periph.sha.blocks, 0);
                assert!(!bus.periph.sha.busy);
            }
            assert!(!bus.periph.gdma.out[0].running);
            assert_ne!(bus.periph.gdma.out[0].int_raw & (1 << 2), 0);
        }
    }
}

#[test]
fn aes_empty_destination_cycle_is_not_reported_as_success() {
    let mut bus = bus_with_out(6, 16, 0);
    arm_aes_input(&mut bus, 0, RX_DESC);
    bus.aes_dma_step();
    assert!(!bus.periph.gdma.inp[0].running);
    assert_eq!(bus.periph.gdma.inp[0].int_raw, 1 << 3);
    assert_eq!(bus.periph.aes.int_raw, 0);
}

#[test]
fn sha_rejects_oversized_and_short_input_without_allocating_a_padded_message() {
    for blocks in [u32::MAX, 1] {
        let mut bus = bus_with_out(7, 0, 0);
        bus.periph.sha.block_num = blocks;
        bus.sha_dma_step();
        assert_eq!(bus.periph.sha.blocks, 0);
        assert!(!bus.periph.gdma.out[0].running);
        assert_eq!(bus.periph.gdma.out[0].int_raw & (1 << 2), 1 << 2);
    }
}

#[test]
fn crypto_descriptor_read_fault_stops_channel() {
    for peripheral in [6, 7] {
        let mut bus = bus_with_out(peripheral, 0, 0);
        bus.periph.gdma.out[0].desc = DRAM_HIGH;
        if peripheral == 6 { arm_aes_input(&mut bus, 16, 0); bus.aes_dma_step(); }
        else { bus.periph.sha.block_num = 1; bus.sha_dma_step(); }
        assert!(!bus.periph.gdma.out[0].running);
        assert_eq!(bus.periph.gdma.out[0].int_raw, 1 << 2);
    }
}

#[test]
fn aes_dma_scatters_across_descriptors_and_marks_only_final_eof() {
    let mut bus = bus_with_out(6, 16, 0);
    arm_aes_input(&mut bus, 8, RX_DESC + 16);
    bus.write32(RX_DESC + 16, (1 << 31) | 8).unwrap();
    bus.write32(RX_DESC + 20, OUTPUT + 8).unwrap();
    bus.write32(RX_DESC + 24, 0).unwrap();
    bus.aes_dma_step();
    let actual: Vec<_> = (0..16).map(|i| bus.read8(OUTPUT + i).unwrap()).collect();
    assert_eq!(actual, [0x66, 0xe9, 0x4b, 0xd4, 0xef, 0x8a, 0x2c, 0x3b, 0x88, 0x4c, 0xfa, 0x59, 0xca, 0x34, 0x2b, 0x2e]);
    assert_eq!(bus.read32(RX_DESC).unwrap() >> 30, 0);
    assert_eq!(bus.read32(RX_DESC + 16).unwrap() >> 30, 1);
    assert_eq!(bus.periph.gdma.inp[0].eof_desc, RX_DESC + 16);
    assert!(!bus.periph.gdma.inp[0].running);
    assert_eq!(bus.periph.aes.state, 2);
    assert_eq!(bus.periph.aes.int_raw, 1);
}

#[test]
fn sha_dma_hashes_exact_requested_message() {
    let mut bus = bus_with_out(7, 64, 0);
    let mut padded = [0; 64];
    padded[..4].copy_from_slice(b"abc\x80");
    padded[63] = 24;
    bus.load_bytes(INPUT, &padded).unwrap();
    bus.periph.sha.block_num = 1;
    bus.periph.sha.dma_first = true;
    bus.sha_dma_step();
    assert_eq!(&bus.periph.sha.h[..8], &[0xba7816bf, 0x8f01cfea, 0x414140de, 0x5dae2223, 0xb00361a3, 0x96177a9c, 0xb410ff61, 0xf20015ad]);
    assert_eq!(bus.periph.sha.blocks, 1);
    assert_eq!(bus.periph.gdma.out[0].int_raw & (1 << 2), 0);
}

#[test]
fn i2s_32_bit_stereo_consumes_eight_bytes_and_keeps_the_high_sample_bits() {
    let mut bus = bus_with_out(3, 16, 0);
    bus.write32(INPUT, 0x1234_5678).unwrap();
    bus.write32(INPUT + 4, 0xaaaa_bbbb).unwrap();
    bus.write32(INPUT + 8, 0xfedc_ba98).unwrap();
    bus.write32(INPUT + 12, 0xcccc_dddd).unwrap();
    bus.periph.i2s0.write(0x2c, (31 << 13) | (31 << 18) | (31 << 24));
    bus.periph.i2s0.write(0x54, (1 << 16) | 3);
    bus.periph.i2s0.write(0x24, 4);
    bus.periph.i2s0.sample_rate = crate::periph::CPU_HZ as u32;
    bus.dma_i2s_step(1);
    assert_eq!(bus.periph.gdma.out[0].buf_pos, 8);
    assert_eq!(bus.periph.i2s0.pcm, [0x1234]);
    bus.dma_i2s_step(1);
    assert_eq!(bus.periph.gdma.out[0].buf_pos, 16);
    assert_eq!(bus.periph.i2s0.pcm, [0x1234, 0xfedcu16 as i16]);
}

#[test]
fn aes_dma_rejects_mmio_destination_before_writing_any_register() {
    let mut bus = bus_with_out(6, 16, 0);
    arm_aes_input(&mut bus, 16, 0);
    bus.write32(RX_DESC + 4, 0x6000_4004).unwrap(); // GPIO_OUT
    bus.aes_dma_step();
    assert_eq!(bus.periph.gpio.out, 0);
    assert_eq!(bus.periph.gpio.enable, 0);
    assert!(!bus.periph.gdma.inp[0].running);
    assert_eq!(bus.periph.gdma.inp[0].int_raw, 1 << 3);
    assert_eq!(bus.periph.aes.int_raw, 0);
}

#[test]
fn streaming_dma_buffer_faults_raise_error_instead_of_emitting_zeros() {
    for peripheral in [3, 5] {
        let mut bus = bus_with_out(peripheral, 16, 0);
        bus.write32(DESC + 4, DRAM_HIGH).unwrap();
        if peripheral == 3 {
            bus.periph.i2s0.write(0x2c, 15 << 13);
            bus.periph.i2s0.write(0x54, (1 << 16) | 3);
            bus.periph.i2s0.write(0x24, 4);
            bus.periph.i2s0.sample_rate = crate::periph::CPU_HZ as u32;
            bus.dma_i2s_step(1);
            assert!(bus.periph.i2s0.pcm.is_empty());
        } else {
            bus.periph.lcd_cam.lcd_user = 1 << 27;
            bus.periph.lcd_cam.lcd_ctrl = 1 << 31;
            bus.periph.lcd_cam.lcd_ctrl1 = 511 << 8;
            bus.dma_lcd_step(1);
            assert_eq!(bus.periph.lcd_cam.lcd_frames, 0);
        }
        assert!(!bus.periph.gdma.out[0].running);
        assert_eq!(bus.periph.gdma.out[0].int_raw, 1 << 2);
    }
}

#[test]
fn crypto_owner_check_is_controlled_by_conf1() {
    for check_owner in [false, true] {
        for input_side in [false, true] {
            let mut bus = bus_with_out(6, 16, 0);
            arm_aes_input(&mut bus, 16, 0);
            let (desc, conf1) = if input_side {
                (RX_DESC, &mut bus.periph.gdma.inp[0].conf1)
            } else { (DESC, &mut bus.periph.gdma.out[0].conf1) };
            *conf1 = if check_owner { 1 << 12 } else { 0 };
            let control = bus.read32(desc).unwrap();
            bus.write32(desc, control & !(1 << 31)).unwrap();
            bus.aes_dma_step();
            assert_eq!(bus.periph.aes.state == 2, !check_owner);
        }
    }
}
