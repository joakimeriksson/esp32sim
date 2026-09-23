//! Regression coverage for the opt-in core pricing model.
use emu_core::{Bus, Core};
use xtensa_lx7::{decode, disasm, state::sr, Cpu, FlatRam};

const BASE: u32 = 0x4037_0000;

fn cpu_at(prog: &[u8]) -> (Cpu, FlatRam) {
    let mut ram = FlatRam::new(BASE, 64 * 1024);
    ram.mem[..prog.len()].copy_from_slice(prog);
    let mut cpu = Cpu::new(0);
    cpu.pc = BASE;
    cpu.ps = 0;
    for i in 0..3 { cpu.write_sr(sr::CCOMPARE0 + i, u32::MAX); }
    (cpu, ram)
}

fn check_decode(pc: u32, bytes: &[u8], expect: &str) {
    let mut b = [0u8; 4];
    b[..bytes.len()].copy_from_slice(bytes);
    let i = decode(pc, b);
    assert_eq!(i.len as usize, bytes.len(), "len for {expect}");
    assert!(
        disasm::format(&i).starts_with(expect),
        "decode {:02x?} = {}",
        bytes,
        disasm::format(&i)
    );
}

/// R1: native JIT must agree with the interpreter on priced control flow.
#[test]
fn r1_native_jit_drops_control_price() {
    // movi a2,1 ; j BASE+9 ; movi a3,2 ; movi a4,3
    // J at BASE+3 targets BASE+9 (0x...009, &3==1: no straddle).
    let prog = vec![0x22, 0xa0, 0x01, 0x86, 0x00, 0x00, 0x32, 0xa0, 0x02, 0x24, 0xa0, 0x03];
    check_decode(BASE, &prog[0..3], "movi");
    check_decode(BASE + 3, &prog[3..6], "j ");
    let mut extras = Vec::new();
    for jit in [false, true] {
        let (mut cpu, mut ram) = cpu_at(&prog);
        cpu.price_control = true;
        cpu.blocks.jit_enabled = jit;
        let (used, trap) = cpu.run(&mut ram, 4);
        let extra = cpu.take_timing_extra();
        eprintln!("jit={jit} used={used} trap={trap:?} pc={:#x} extra={extra} jit_insns={}", cpu.pc, cpu.blocks.jit_instructions);
        extras.push(extra);
        assert_eq!(trap, None);
        assert_eq!(cpu.pc, BASE + 9);
    }
    assert_eq!(extras[0], 2, "interpreter prices taken J with +2");
    assert_eq!(extras[1], extras[0], "native JIT must price the same J");
}

/// R2: L32ai carries the same load-use hazard as L32i but is not priced.
#[test]
fn r2_load_use_missing_opcodes() {
    // load a5,[a4] ; add a6,a5,a3 ; j self
    // a4 -> BASE+0x100 holding 0x11111111, a3 = 0x100.
    fn prog(load: [u8; 3]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend(load);
        p.extend([0x30, 0x65, 0x80]); // add a6,a5,a3
        p.extend([0x86, 0x00, 0x00]); // j .+2 (self)
        p
    }
    // L32i a5,a4,0 = 52 24 00 ; L32ai a5,a4,0 = 52 b4 00
    check_decode(BASE, &[0x52, 0x24, 0x00], "l32i");
    check_decode(BASE, &[0x52, 0xb4, 0x00], "l32ai");
    let mut got = Vec::new();
    for (name, load) in [("l32i", [0x52, 0x24, 0x00]), ("l32ai", [0x52, 0xb4, 0x00]), ("s32c1i", [0x52, 0xe4, 0x00])] {
        let (mut cpu, mut ram) = cpu_at(&prog(load));
        ram.write32(BASE + 0x100, 0x1111_1111).unwrap();
        cpu.price_control = true;
        cpu.blocks.jit_enabled = false;
        cpu.set_ar(4, BASE + 0x100);
        cpu.set_ar(3, 0x100);
        let (used, trap) = cpu.run(&mut ram, 3);
        let extra = cpu.take_timing_extra();
        eprintln!("{name}: used={used} trap={trap:?} a6={:#x} extra={extra}", cpu.get_ar(6));
        assert_eq!(trap, None);
        assert_eq!(cpu.get_ar(6), 0x1111_1211);
        got.push(extra);
    }
    assert_eq!(got[0], 3, "l32i -> add pays load-use 1 plus J 2");
    assert_eq!(got[1], got[0], "l32ai -> add must pay the same load-use cycle");
    assert_eq!(got[2], got[0], "s32c1i -> add must pay the same load-use cycle");
}

/// R3: block-path trap entry costs +6 while the single-step path costs 0.
#[test]
fn r3_trap_extra_step_vs_block() {
    let prog = vec![0x00, 0x00, 0x00]; // ill
    check_decode(BASE, &prog, "ill");
    let (mut a, mut ra) = cpu_at(&prog);
    a.price_control = true;
    a.blocks.jit_enabled = false;
    let (used, trap) = a.run(&mut ra, 4);
    let extra_block = a.take_timing_extra();
    eprintln!("block: used={used} trap={trap:?} extra={extra_block}");

    let (mut b, mut rb) = cpu_at(&prog);
    b.price_control = true;
    let r = xtensa_lx7::step(&mut b, &mut rb);
    let extra_step = b.take_timing_extra();
    eprintln!("step: result={r:?} extra={extra_step}");
    assert!(r.is_err());
    assert_eq!(extra_block, 6, "block path charges trap entry");
    assert_eq!(extra_step, extra_block, "step path must charge the same trap entry");
}

/// R4: fetch pricing is charged for the whole decoded block span, and never on step.
#[test]
fn r4_fetch_step_vs_block_and_early_exit() {
    const FBASE: u32 = 0x4200_0000;
    fn fcpu_at(prog: &[u8]) -> (Cpu, FlatRam) {
        let mut ram = FlatRam::new(FBASE, 64 * 1024);
        ram.mem[..prog.len()].copy_from_slice(prog);
        let mut cpu = Cpu::new(0);
        cpu.pc = FBASE;
        cpu.ps = 0;
        for i in 0..3 { cpu.write_sr(sr::CCOMPARE0 + i, u32::MAX); }
        (cpu, ram)
    }
    // 12 movis = 36 bytes spanning two 32-byte fetch lines; budget 1 executes
    // one insn on the first line while the block charges the whole block span.
    let mut prog = Vec::new();
    for k in 0..12 {
        prog.extend([0x32, 0xa0, (k + 1) as u8]); // movi a3,k+1
    }
    check_decode(FBASE, &prog[0..3], "movi");
    let (mut a, mut ra) = fcpu_at(&prog);
    a.price_control = true;
    a.icache_fill = 5;
    a.blocks.jit_enabled = false;
    let (used, trap) = a.run(&mut ra, 1);
    let extra_block = a.take_timing_extra();
    let misses_block = a.icache_misses;
    eprintln!("block: used={used} trap={trap:?} pc={:#x} extra={extra_block} misses={misses_block}", a.pc);

    let (mut b, mut rb) = fcpu_at(&prog);
    b.price_control = true;
    b.icache_fill = 5;
    let r = xtensa_lx7::step(&mut b, &mut rb);
    let extra_step = b.take_timing_extra();
    eprintln!("step: result={r:?} pc={:#x} extra={extra_step} misses={}", b.pc, b.icache_misses);
    assert_eq!(used, 1);
    assert!(r.is_ok());
    assert_eq!((extra_block, misses_block), (5, 1), "only the executed instruction's line is fetched");
    assert_eq!((extra_step, b.icache_misses), (extra_block, misses_block), "step must charge the same fetch");
}
