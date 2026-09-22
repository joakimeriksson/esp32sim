//! Real-machine regressions for scheduler exits; invoked only by the WASM test build.
use esp_soc::{ScriptAction, SocBus, Stop};
use xtensa_lx7::{bus::Bus, Core};
const BASE: u32 = 0x4037_0000;
const CONTROL: u32 = 0x600c_0000;
// addi.n a3,a3,1; s32i a5,a4,0; j back
const LOOP: [u8; 8] = [0x1b, 0x33, 0x52, 0x64, 0x00, 0xc6, 0xfd, 0xff];
fn machine(jit: bool) -> esp32s3::Machine {
    let mut m = esp32s3::machine([1, 2, 3, 4, 5, 6]);
    m.console.capture = true;
    SocBus::load_bytes(&mut m.bus, BASE, &LOOP).unwrap();
    SocBus::load_bytes(
        &mut m.bus,
        0x4000_0400,
        &[0x00, 0x70, 0x00, 0x06, 0xff, 0xff],
    )
    .unwrap();
    for c in &mut m.cores {
        c.set_jit(jit);
    }
    let c = &mut m.cores[0];
    c.pc = BASE;
    c.ps = 0;
    c.set_ar(4, BASE + 0x400);
    c.set_ar(5, 0);
    m.max_cycles = 4096;
    assert!(matches!(m.run(u64::MAX), Stop::Halted));
    if jit {
        assert!(
            m.cores[0].blocks.jit_instructions > 100,
            "machine never entered compiled code"
        );
    }
    m
}
fn same(a: &esp32s3::Machine, b: &esp32s3::Machine) {
    assert_eq!(a.bus.cycles, b.bus.cycles);
    assert_eq!(a.script.pos, b.script.pos);
    assert_eq!(a.console.all, b.console.all);
    assert_eq!(a.console.uart0, b.console.uart0);
    for (a, b) in a.cores.iter().zip(&b.cores) {
        assert_eq!(a.pc, b.pc);
        assert_eq!(a.ar, b.ar);
        assert_eq!(a.ps, b.ps);
        assert_eq!(a.ccount, b.ccount);
        assert_eq!(a.insn_count, b.insn_count);
        assert_eq!(a.interrupt, b.interrupt);
        assert_eq!(a.waiting(), b.waiting());
        assert_eq!(a.windowbase, b.windowbase);
        assert_eq!(a.epc, b.epc);
    }
}
fn solo_core_one() -> u32 {
    for jit in [false, true] {
        for mmio in [false, true] {
            let (mut a, mut b) = (machine(jit), machine(jit));
            for m in [&mut a, &mut b] {
                m.cores[0].waiting = true;
                m.cores[0].intenable = 0;
                m.cores[0].interrupt = 0;
                m.bus.write32(CONTROL, 2).unwrap();
                // Let the scheduler release core1 through its normal reset path.
                m.max_cycles = m.bus.cycles + 64;
                assert!(matches!(m.run(u64::MAX), Stop::Halted));
                let c = &mut m.cores[1];
                c.pc = BASE; c.ps = 0; c.waiting = false;
                c.intenable = 0; c.interrupt = 0;
                c.set_ar(3, 0);
                c.set_ar(4, if mmio { CONTROL } else { BASE + 0x400 });
                c.set_ar(5, 2); // keep core1 released when the loop writes CONTROL
                m.max_cycles = m.bus.cycles + 4096;
            }
            a.vq_max = 1;
            b.vq_max = 1000;
            let before = b.vq_stats;
            assert!(matches!(a.run(u64::MAX), Stop::Halted));
            assert!(matches!(b.run(u64::MAX), Stop::Halted));
            same(&a, &b);
            assert!(b.vq_stats[0] > before[0], "core1 must exercise virtual quanta");
            if mmio { assert!(b.vq_stats[2] > before[2], "core1 must stop before device access"); }
        }
    }
    4
}

#[cfg(feature = "cache-inline")]
fn sequential_emulators_reset_timing_state() -> u32 {
    use std::sync::atomic::Ordering::Relaxed;
    use xtensa_lx7::jit::{CACHE_PROBES, CACHE_SET_MASK, FETCH_RING, PRICED};
    let name = b"none";
    let exercise = |m: &mut esp32s3::Machine| {
        SocBus::load_bytes(&mut m.bus, BASE, &LOOP).unwrap();
        let c = &mut m.cores[0];
        c.pc = BASE; c.ps = 0;
        c.set_ar(4, BASE + 0x400); c.set_ar(5, 0);
        m.max_cycles = 4096;
        assert!(matches!(m.run(u64::MAX), Stop::Halted));
        assert!(m.cores[0].blocks.jit_instructions > 100);
    };
    // Exercise the real C ABI in one WASM instance, as worker create/delete does.
    unsafe {
        let first = super::esp32sim_new(name.as_ptr(), name.len(), 1, 0);
        assert!(!first.is_null());
        assert_eq!(super::esp32sim_set_approximate_jit_timing(first, 1, 64), 0);
        assert_eq!(super::esp32sim_set_approximate_jit_frontiers(first, 1), 0);
        assert_eq!(super::esp32sim_set_approximate_jit_cache(first, 96, 160, 3), 0);
        assert_eq!(super::esp32sim_set_control_prices(first, 1), 0);
        assert_eq!(super::esp32sim_set_icache_fill(first, 404), 0);
        assert!(PRICED.load(Relaxed) && CACHE_PROBES.load(Relaxed) && FETCH_RING.load(Relaxed));
        assert_eq!(CACHE_SET_MASK.load(Relaxed), 127);
        let m = (*first).m.s3_mut().unwrap();
        m.cores[0].touch_fetch_lines(0x4200_0000, 0x4200_0000);
        assert_eq!(m.cores[0].icache_misses, 1);
        exercise(m);
        assert!(m.cores[0].blocks.code_bytes() > 0);
        assert!(m.bus.cycles > m.cores[0].insn_count, "the scheduler must charge the first emulator's priced instructions");
        super::esp32sim_delete(first);

        let second = super::esp32sim_new(name.as_ptr(), name.len(), 1, 0);
        assert!(!second.is_null());
        assert!(!PRICED.load(Relaxed) && !CACHE_PROBES.load(Relaxed) && !FETCH_RING.load(Relaxed));
        assert_eq!(CACHE_SET_MASK.load(Relaxed), 63);
        let m = (*second).m.s3_mut().unwrap();
        assert!(m.cores.iter().all(|c| !c.price_control && c.icache_fill == 0 && c.blocks.code_bytes() == 0));
        // Do not call the icache setter here: it itself clears the cache and would
        // hide a missing reset in esp32sim_new.
        m.cores[0].price_control = true;
        m.cores[0].icache_fill = 404;
        m.cores[0].touch_fetch_lines(0x4200_0000, 0x4200_0000);
        assert_eq!(m.cores[0].icache_misses, 1, "new emulator must start with a cold fetch cache");
        m.cores[1].price_control = true;
        m.cores[1].icache_fill = 404;
        m.cores[1].touch_fetch_lines(0x4200_0000, 0x4200_0000);
        assert_eq!(m.cores[1].icache_misses, 0, "the two new cores still share their fetch cache");
        for c in &mut m.cores { c.price_control = false; c.icache_fill = 0; c.timing_extra = 0; }
        exercise(m);
        assert_eq!(m.cores[0].timing_extra, 0, "old priced code must not survive recreation");
        super::esp32sim_delete(second);
    }
    1
}

/// EX177: a batch of whole rounds run while both cores are busy must leave exactly the state the
/// per-round schedule leaves. Cover an interior device access, WAITI, a core-local timer
/// interrupt, a script event, an architectural stop and a plain uncut batch, on either core and
/// at the first, last and following instruction of a quantum.
fn both_busy_rounds() -> u32 {
    const NOP: [u8; 2] = [0x3d, 0xf0];
    const SPIN: [u8; 3] = [0x06, 0xff, 0xff];        // j .
    const BACK: [u8; 3] = [0x06, 0xfd, 0xff];        // j -12: back over four nop.n
    const CODE: [u32; 2] = [BASE + 0x800, BASE + 0xc00];
    const VECTORS: u32 = BASE + 0x2000;
    let timer = xtensa_lx7::state::TIMER_INTERRUPT[0];
    let mut cases = 0;
    for jit in [false, true] {
        for vq in [1u64, 1024] {
            for kind in 0..6 {
                for at in [1usize, 63, 64, 65] {
                    for cut in 0..2usize {
                        let (mut a, mut b) = (machine(jit), machine(jit));
                        for m in [&mut a, &mut b] {
                            m.vq_max = 1;
                            // release core 1 through its ordinary reset path, then keep both busy
                            m.bus.write32(CONTROL, 2).unwrap();
                            m.max_cycles = m.bus.cycles + 64;
                            assert!(matches!(m.run(u64::MAX), Stop::Halted));
                            let peer: Vec<u8> = NOP.repeat(4).into_iter().chain(BACK).collect();
                            let mut code: Vec<u8> = NOP.repeat(at);
                            match kind {
                                1 => code.extend([0x22, 0x23, 0x00]),      // l32i a2,a3,0: SYSTIMER
                                2 => code.extend([0x00, 0x70, 0x00]),      // waiti 0
                                5 => code.extend([0x00, 0x00, 0x00]),      // ill
                                _ => {}
                            }
                            code.extend(SPIN);
                            SocBus::load_bytes(&mut m.bus, VECTORS + xtensa_lx7::state::vec::KERNEL, &SPIN).unwrap();
                            for (i, &entry) in CODE.iter().enumerate() {
                                SocBus::load_bytes(&mut m.bus, entry, if i == cut { &code } else { &peer }).unwrap();
                                let c = &mut m.cores[i];
                                c.pc = entry; c.ps = 0; c.waiting = false;
                                c.intenable = 0; c.interrupt = 0; c.vecbase = VECTORS;
                                c.set_ar(3, 0x6002_3000);
                            }
                            if kind == 3 {
                                let c = &mut m.cores[cut];
                                c.ccompare[0] = c.ccount.wrapping_add(at as u32);
                                c.intenable = 1 << timer;
                            }
                            if kind == 4 {
                                m.script.log = false;
                                m.script.events = vec![
                                    (m.bus.cycles + at as u64, ScriptAction::Serial("event".into())),
                                    (m.bus.cycles + 300, ScriptAction::Stop),
                                ];
                            }
                            if kind == 5 { m.dbg.stop_after_exceptions = 1; }
                            m.vq_max = vq;
                            m.max_cycles = m.bus.cycles + if kind == 0 || kind == 3 { 32768 } else { 512 };
                        }
                        b.bb_max = 128;
                        let label = format!("jit={jit} vq={vq} kind={kind} at={at} cut={cut}");
                        for m in [&mut a, &mut b] {
                            let stop = m.run(u64::MAX);
                            if kind == 5 { assert!(matches!(stop, Stop::Exceptions(1)), "{label}: {stop:?}"); }
                            else { assert!(matches!(stop, Stop::Halted), "{label}: {stop:?}"); }
                            assert_eq!(m.bus.vq_violations, 0, "{label}: undeferred device access");
                        }
                        assert!(b.bb_stats[0] > 0, "{label}: no batch ran");
                        if kind == 0 { assert!(b.bb_stats[1] / b.bb_stats[0] > 8, "{label}: shallow batches"); }
                        if kind == 1 { assert!(b.bb_stats[2] > 0, "{label}: no device-register cut"); }
                        if kind == 2 { assert!(b.bb_stats[3] > 0, "{label}: no waiti cut"); }
                        if kind == 3 { assert!(b.interrupts > 0, "{label}: timer never fired"); }
                        if kind == 4 { assert_eq!(b.script.pos, 2, "{label}: script events never applied"); }
                        same(&a, &b);
                        assert_eq!(a.run_steps(), b.run_steps(), "{label}: scheduling budget");
                        assert_eq!((a.insns(), a.exceptions, a.interrupts, &a.irq_hist),
                                   (b.insns(), b.exceptions, b.interrupts, &b.irq_hist), "{label}");
                        cases += 1;
                    }
                }
            }
        }
    }
    cases
}

/// shell-s2: cached region entry facts leave flash pages to the bus epoch. Rewriting or remapping
/// the second chunk of a hot compiled flash loop, whose head page stays current, must still run
/// the new code, exactly like the interpreter.
fn flash_code_rewrites() -> u32 {
    const CODE: u32 = 0x4201_0000; // IBUS page 1
    const HEAD: [u8; 5] = [0x1b, 0x33, 0x86, 0x3e, 0x00]; // addi.n a3,a3,1; j CODE+0x100
    let tail = |step: u8| [0x0b | (step << 4), 0x33, 0x86, 0xbe, 0xff]; // addi.n a3,a3,STEP; j CODE
    for writer in 0..3 {
        let (mut a, mut b) = (machine(false), machine(true));
        for m in [&mut a, &mut b] {
            for (page, step) in [(0x1_0000, 1), (0x2_0000, 2)] {
                m.write_flash(page, &HEAD).unwrap();
                m.write_flash(page + 0x100, &tail(step)).unwrap();
            }
            m.bus.write32(esp32s3::bus::MMU_TABLE + 4, 1).unwrap();
            let c = &mut m.cores[0];
            c.pc = CODE;
            c.set_ar(3, 0);
            for _ in 0..2 {
                m.max_cycles = m.bus.cycles + 20_000;
                assert!(matches!(m.run(u64::MAX), Stop::Halted));
                match writer {
                    0 => m.write_flash(0x1_0100, &tail(2)).unwrap(),
                    1 => m.bus.write32(esp32s3::bus::MMU_TABLE + 4, 2).unwrap(),
                    _ => SocBus::load_bytes(&mut m.bus, CODE + 0x100, &tail(2)).unwrap(),
                }
            }
            m.max_cycles = m.bus.cycles + 20_000;
            assert!(matches!(m.run(u64::MAX), Stop::Halted));
        }
        same(&a, &b);
        assert!(b.cores[0].get_ar(3) > 20_000, "writer {writer}: the flash loop never ran");
    }
    3
}

fn architectural_stops() -> u32 {
    for jit in [false, true] {
        for busy in 0..2 {
            for instructions in [1usize, 63, 64, 65, 127, 128, 129] {
                let (mut a, mut b) = (machine(jit), machine(jit));
                for m in [&mut a, &mut b] {
                    m.vq_max = 1;
                    if busy == 1 {
                        m.bus.write32(CONTROL, 2).unwrap();
                        m.max_cycles = m.bus.cycles + 64;
                        assert!(matches!(m.run(u64::MAX), Stop::Halted));
                        m.cores[0].waiting = true;
                    }
                    let mut code = [0x3d, 0xf0].repeat(instructions - 1); // nop.n
                    code.extend([0, 0, 0]); // ill
                    m.bus.load_bytes(BASE + 0x800, &code).unwrap();
                    m.cores[busy].pc = BASE + 0x800;
                    m.cores[busy].ps = 0;
                    m.cores[busy].waiting = false;
                    m.dbg.stop_after_exceptions = 1;
                    m.max_cycles = m.bus.cycles + (instructions as u64).div_ceil(64).max(2) * 64;
                }
                b.vq_max = 1024;
                let before = b.vq_stats[0];
                assert!(matches!(a.run(u64::MAX), Stop::Exceptions(1)));
                assert!(matches!(b.run(u64::MAX), Stop::Exceptions(1)));
                assert!(b.vq_stats[0] > before);
                same(&a, &b);
            }
        }
    }
    28
}

pub fn run() -> u32 {
    crate::browser_jit::code_page_watch_test();
    let (mut a, mut b) = (machine(false), machine(true));
    for m in [&mut a, &mut b] {
        m.bus.periph.uart[0].tx_out.extend(b"compiled stop\n");
        m.script
            .events
            .push((m.bus.cycles + 64, ScriptAction::Stop));
        m.max_cycles = u64::MAX;
        assert!(matches!(m.run(u64::MAX), Stop::Halted));
        assert!(
            m.console.all.ends_with(b"compiled stop\n"),
            "script stop did not drain console"
        );
    }
    same(&a, &b);
    let (mut a, mut b) = (machine(false), machine(true));
    // The compiled store first releases the peer, then holds it while it is asleep
    // with a CCOMPARE deadline in the same round. Compare that round and the next.
    for value in [2, 0] {
        let before = b.cores[0].blocks.jit_instructions;
        for m in [&mut a, &mut b] {
            m.cores[0].pc = BASE;
            m.cores[0].set_ar(4, CONTROL);
            m.cores[0].set_ar(5, value);
            if value == 0 {
                let c = &mut m.cores[1];
                c.ps = 0;
                c.ccompare[0] = c.ccount + 1;
                c.intenable = 1 << 6;
            }
            m.max_cycles = m.bus.cycles + 64;
            assert!(matches!(m.run(u64::MAX), Stop::Halted));
        }
        assert!(
            b.cores[0].blocks.jit_instructions > before,
            "MMIO store missed compiled block"
        );
        same(&a, &b);
        for m in [&mut a, &mut b] {
            m.max_cycles = m.bus.cycles + 64;
            assert!(matches!(m.run(u64::MAX), Stop::Halted));
        }
        same(&a, &b);
    }
    let cases = 3 + solo_core_one() + architectural_stops() + both_busy_rounds() + flash_code_rewrites();
    #[cfg(feature = "cache-inline")]
    let cases = cases + sequential_emulators_reset_timing_state();
    cases
}
