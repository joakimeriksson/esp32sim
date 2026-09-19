use emu_core::Core;
use xtensa_lx7::{Cpu, FlatRam};
const BASE: u32 = 0x4037_0000;

fn setup(program: &[u8]) -> (Cpu, FlatRam) {
    let mut cpu = Cpu::new(0);
    cpu.pc = BASE;
    cpu.ps = 0;
    cpu.price_control = true;
    cpu.approximate_cpi = 3;
    cpu.cpenable = 9;
    cpu.scompare1 = 1; // Failed conditional stores still return the loaded value.
    cpu.ccompare = [u32::MAX; 3];
    cpu.set_ar(4, BASE + 0x100);
    let mut ram = FlatRam::new(BASE, 4096);
    ram.mem[..program.len()].copy_from_slice(program);
    (cpu, ram)
}

#[test]
fn step_block_and_mixed_prefixes_share_prices() {
    // Load-use, FP scoreboard, control transfer and faulting execution.
    for (program, n, extra) in [
        (vec![0x52, 0xb4, 0, 0x30, 0x65, 0x80, 0x86, 0, 0], 3, 3),
        (vec![0x52, 0xe4, 0, 0x30, 0x65, 0x80, 0x86, 0, 0], 3, 3),
        (vec![0x50, 0x04, 0x09, 0x30, 0x65, 0x80, 0x86, 0, 0], 3, 3),
        (vec![0x20, 0x01, 0x0a, 0x20, 0x30, 0x0a, 0x86, 0, 0], 3, 5),
        (vec![0x44, 0x00, 0x83, 0x04, 0x30, 0xcd, 0x86, 0, 0], 3, 3),
        (vec![0x22, 0xa0, 1, 0, 0, 0], 2, 6),
    ] {
        let mut reference = None;
        for mode in 0..5 {
            let (mut cpu, mut ram) = setup(&program);
            cpu.blocks.jit_enabled = mode != 0;
            if mode < 2 {
                assert_eq!(cpu.run(&mut ram, n).0, n);
            } else {
                for index in 0..n {
                    if mode == 2 || (index % 2 == 0) == (mode == 3) {
                        let _ = xtensa_lx7::step(&mut cpu, &mut ram);
                    } else {
                        assert_eq!(cpu.run(&mut ram, 1).0, 1);
                    }
                }
            }
            assert_eq!(cpu.timing_extra, extra, "mode={mode}, program={program:x?}");
            let result = (cpu.pc, cpu.ccount, cpu.insn_count, cpu.timing_extra, cpu.get_ar(6), cpu.fr);
            if let Some(expected) = reference { assert_eq!(result, expected); } else { reference = Some(result); }
            assert_eq!(cpu.ccount, n * 3);
        }
    }
}

#[test]
fn idle_and_fetch_fault_match_step() {
    for idle in [false, true] {
        let mut reference = None;
        for step in [false, true] {
            let (mut cpu, mut ram) = setup(&[]);
            cpu.waiting = idle;
            cpu.pc = BASE + 0x10000;
            let trap = if step { xtensa_lx7::step(&mut cpu, &mut ram).err() } else { cpu.run(&mut ram, 1).1 };
            let result = (cpu.pc, cpu.ccount, cpu.insn_count, cpu.timing_extra, trap);
            if let Some(expected) = reference { assert_eq!(result, expected); } else { reference = Some(result); }
            assert_eq!(cpu.timing_extra, if idle { 0 } else { 6 });
            assert_eq!(cpu.ccount, if idle { 3 } else { 0 });
        }
    }
}

#[test]
fn interrupt_entry_matches_step_without_retirement() {
    let mut reference = None;
    for step in [false, true] {
        let (mut cpu, mut ram) = setup(&[0x22, 0xa0, 1]);
        cpu.interrupt = 1;
        cpu.intenable = 1;
        let trap = if step { xtensa_lx7::step(&mut cpu, &mut ram).err() } else { cpu.run(&mut ram, 1).1 };
        assert!(matches!(trap, Some(xtensa_lx7::Trap::Interrupt(_))));
        assert_eq!((cpu.insn_count, cpu.ccount, cpu.timing_extra), (0, 0, 6));
        let result = (cpu.pc, trap);
        if let Some(expected) = reference { assert_eq!(result, expected); } else { reference = Some(result); }
    }
}

#[test]
fn fetch_requires_pricing_and_counts_instruction_end() {
    use xtensa_lx7::state::reset_shared_fetch_cache;
    for enabled in [false, true] { for step in [false, true] {
        reset_shared_fetch_cache();
        let base = 0x4200_0000;
        let mut ram = FlatRam::new(base, 4096);
        // A 3-byte instruction at offset 30 straddles two 32-byte lines.
        ram.mem[30..33].copy_from_slice(&[0x22, 0xa0, 1]);
        let (mut cpu, _) = setup(&[]);
        cpu.pc = base + 30;
        cpu.price_control = enabled;
        cpu.icache_fill = 5;
        if step { xtensa_lx7::step(&mut cpu, &mut ram).unwrap(); }
        else { assert_eq!(cpu.run(&mut ram, 1), (1, None)); }
        assert_eq!(cpu.timing_extra, if enabled { 10 } else { 0 });
        assert_eq!(cpu.icache_misses, if enabled { 2 } else { 0 });
    } }
}
