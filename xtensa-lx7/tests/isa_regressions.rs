//! Independent encoding and result checks for the September 2026 review.
//! FFT layouts and operation are ESP32-S3 TRM v1.8 sections 1.8.11–1.8.12:
//! https://documentation.espressif.com/esp32-s3_technical_reference_manual_en.pdf
use xtensa_lx7::{decode, pie, pie_timing, step, Cpu, FlatRam, Op, Trap};

const BASE: u32 = 0x4037_0000;

#[test]
fn custom_slots_decode_their_actual_pie_table_entries() {
    for op1 in [6, 7] {
        let name = if op1 == 6 { "ee.ldf.64.xp" } else { "ee.stf.64.xp" };
        for op2 in 0..16 {
            for operands in [0, 0x123, 0xfff] {
                let raw: u32 = op2 << 20 | op1 << 16 | operands << 4;
                let insn = decode(BASE, raw.to_le_bytes());
                assert_eq!((insn.op, insn.len), (Op::Pie, 3), "{raw:06x}");
                assert_eq!(pie::OPS[insn.imm as usize].name, name);
                assert_eq!(insn.imm2, ((operands >> 4) & 15).max(operands & 15) as i32);
            }
        }
    }
}

#[test]
fn custom_pie_slots_load_and_store_float_registers() {
    // TRM 1.8.28 / 1.8.68 and the checked-in objdump corpus:
    // ee.ldf.64.xp f13,f0,a4,a8; ee.stf.64.xp f0,f2,a6,a8.
    let (mut cpu, mut ram) = fft_state();
    cpu.ps = 0; cpu.cpenable |= 1;
    cpu.set_ar(4, BASE + 0x43); cpu.set_ar(6, BASE + 0x53); cpu.set_ar(8, 24);
    cpu.fr[2] = 0x1234_5678;
    ram.mem[0x40..0x48].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let before = cpu.qr;
    for (raw, pc) in [(0x06d840_u32, BASE), (0x270860, BASE + 3)] {
        cpu.pc = pc;
        ram.mem[(pc - BASE) as usize..(pc - BASE) as usize + 4].copy_from_slice(&raw.to_le_bytes());
        step(&mut cpu, &mut ram).unwrap();
    }
    assert_eq!((cpu.fr[0], cpu.fr[13]), (0x0403_0201, 0x0807_0605));
    assert_eq!(&ram.mem[0x50..0x58], &[0x78, 0x56, 0x34, 0x12, 1, 2, 3, 4]);
    assert_eq!((cpu.get_ar(4), cpu.get_ar(6)), (BASE + 0x5b, BASE + 0x6b));
    assert_eq!(cpu.qr, before);
}

fn lanes(values: [i16; 8]) -> u128 {
    let mut bytes = [0; 16];
    for (lane, value) in bytes.as_chunks_mut::<2>().0.iter_mut().zip(values) { lane.copy_from_slice(&value.to_le_bytes()); }
    u128::from_le_bytes(bytes)
}

fn fft_state() -> (Cpu, FlatRam) {
    let mut cpu = Cpu::new(0);
    cpu.pc = BASE; cpu.cpenable = 1 << 3; cpu.sar = 1;
    cpu.qr[0] = u128::MAX;
    cpu.qr[1] = lanes([10, -20, 30, -40, 50, -60, 70, -80]);
    cpu.qr[2] = lanes([3, 4, -5, 6, 7, -8, 9, 10]);
    cpu.qr[3] = lanes([100, 200, 300, 400, 500, 600, 700, 800]);
    cpu.set_ar(2, BASE + 0x43); cpu.set_ar(3, 32);
    let mut ram = FlatRam::new(BASE, 0x100);
    ram.mem[0x40..0x50].copy_from_slice(&[0xa5; 16]);
    (cpu, ram)
}

fn fft_ld(sel: u32) -> u32 {
    // qu=q4, as=a2, ad=a3, qz=q3, qx=q1, qy=q2; direct TRM bit layout.
    0xdc00_000e | 2 << 4 | 3 << 8 | 2 << 12 | 1 << 14 | 3 << 16 | 2 << 20
        | (sel & 1) << 19 | (sel >> 1) << 24
}

fn fft_st(sel: u32, upd: u32, sar4: u32) -> u32 {
    // qx=q1, qy=q2, qv=q3, as=a2, ad=a3.
    0xa800_000e | 2 << 4 | 3 << 8 | 1 << 14 | 3 << 16 | 2 << 20
        | (sel >> 1) << 12 | (sel & 1) << 23 | (upd & 1) << 19 | (upd >> 1) << 24 | sar4 << 25
}

#[test]
fn fft_load_updates_only_the_three_pairs_defined_by_the_trm() {
    let results = [(-25, -50), (55, -10), (-195, 10), (45, 190), (415, -10), (-65, -410)];
    for sel in 0..8 {
        let (mut cpu, mut ram) = fft_state();
        let mut expected = [100, 200, 300, 400, 500, 600, 700, 800];
        if let Some(&(re, im)) = results.get(sel as usize) {
            expected[(sel / 2 * 2) as usize] = re;
            expected[(sel / 2 * 2 + 1) as usize] = im;
        }
        let insn = decode(BASE, fft_ld(sel).to_le_bytes());
        assert_eq!(pie::format(insn.raw, insn.imm as usize), format!("ee.fft.cmul.s16.ld.xp\tq4, a2, a3, q3, q1, q2, {sel}"));
        pie::exec(&mut cpu, &mut ram, &insn).unwrap();
        assert_eq!(cpu.qr[3], lanes(expected), "sel8={sel}");
        assert_eq!(cpu.qr[4], u128::from_le_bytes([0xa5; 16]));
        assert_eq!(cpu.get_ar(2), BASE + 0x63);
    }
}

#[test]
fn fft_store_computes_the_final_pair_and_preserves_all_q_registers() {
    for sel in [6, 7] {
        for upd in 0..3 {
            for sar4 in 0..4 {
                let (mut cpu, mut ram) = fft_state();
                let before = cpu.qr;
                let shifted = [10 >> sar4, -20 >> sar4, 30 >> sar4, -40 >> sar4];
                let (re, im) = if sel == 6 { (-85, -710) } else { (715, -10) };
                let expected = match upd {
                    0 => [100, 200, 300, 400, 500, 600, re, im],
                    1 => [shifted[0], shifted[1], shifted[2], shifted[3], 500, 600, re, im],
                    _ => [shifted[0], shifted[1], 500, 600, shifted[2], shifted[3], re, im],
                };
                let insn = decode(BASE, fft_st(sel, upd, sar4).to_le_bytes());
                assert_eq!(pie::format(insn.raw, insn.imm as usize), format!("ee.fft.cmul.s16.st.xp\tq1, q2, q3, a2, a3, {sel}, {upd}, {sar4}"));
                assert_eq!(pie_timing::insn_effects(&insn).writes, 0);
                pie::exec(&mut cpu, &mut ram, &insn).unwrap();
                assert_eq!(&ram.mem[0x40..0x50], &lanes(expected).to_le_bytes(), "sel8={sel} upd4={upd} sar4={sar4}");
                assert_eq!(cpu.qr, before);
                assert_eq!(cpu.get_ar(2), BASE + 0x63);
            }
        }
    }
}

#[test]
fn undocumented_fft_store_operands_fail_before_changing_state() {
    for (sel, upd) in [(0, 0), (5, 2), (6, 3), (7, 3)] {
        let (mut cpu, mut ram) = fft_state();
        let before = (cpu.qr, cpu.get_ar(2), ram.mem.clone());
        let insn = decode(BASE, fft_st(sel, upd, 0).to_le_bytes());
        assert_eq!(pie::exec(&mut cpu, &mut ram, &insn), Err(Trap::Unimplemented(BASE, insn.raw)));
        assert_eq!((cpu.qr, cpu.get_ar(2), ram.mem), before);
    }
}

#[test]
fn fft_load_fault_preserves_aliased_inputs_for_restart() {
    let (mut cpu, mut ram) = fft_state();
    // Alias Qz with Qx so committing the product before a fault would corrupt a retry.
    let raw = (fft_ld(0) & !(7 << 16)) | (1 << 16);
    let insn = decode(BASE, raw.to_le_bytes());
    cpu.set_ar(2, BASE + 0x1000);
    let before = cpu.qr;
    assert!(pie::exec(&mut cpu, &mut ram, &insn).is_err());
    assert_eq!(cpu.qr, before);
    assert_eq!(cpu.get_ar(2), BASE + 0x1000);
    cpu.set_ar(2, BASE + 0x43);
    pie::exec(&mut cpu, &mut ram, &insn).unwrap();
    let expected = lanes([-25, -50, 30, -40, 50, -60, 70, -80]);
    assert_eq!(cpu.qr[1], expected);
    assert_eq!(cpu.qr[4], u128::from_le_bytes([0xa5; 16]));
    assert_eq!(cpu.get_ar(2), BASE + 0x63);
}
