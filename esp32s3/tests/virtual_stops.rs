use emu_core::{Bus, Core};
use esp_soc::Stop;

const IRAM: u32 = 0x4037_0000;

#[test]
fn architectural_stop_preserves_unfinished_round() {
    // Native virtual quanta require ESP32SIM_VQ_NATIVE=1 and the interpreter.
    // Cover both round boundaries and partial rounds, with either core busy.
    for busy in 0..2 {
        for instructions in [1, 63, 64, 65, 127, 128, 129] {
            let mut results = Vec::new();
            for vq in [1, 1024] {
                let mut m = esp32s3::machine([0; 6]);
                m.console.capture = true;
                m.vq_max = 1;
                for c in &mut m.cores { c.set_jit(false); }
                m.bus.load_bytes(IRAM, &[0x06, 0xff, 0xff]).unwrap();
                m.bus.load_bytes(0x4000_0400, &[0x06, 0xff, 0xff]).unwrap();
                m.cores[0].pc = IRAM;
                m.cores[0].ps = 0;
                if busy == 1 { m.bus.write32(0x600c_0000, 2).unwrap(); }
                assert!(matches!(m.run(64), Stop::MaxInsns));
                if busy == 1 { m.cores[0].waiting = true; }
                let mut code = [0x3d, 0xf0].repeat(instructions - 1); // nop.n
                code.extend([0, 0, 0]); // ill
                m.bus.load_bytes(IRAM + 0x100, &code).unwrap();
                m.cores[busy].pc = IRAM + 0x100;
                m.cores[busy].ps = 0;
                m.dbg.stop_after_exceptions = 1;
                m.vq_max = vq;
                m.max_cycles = 64 + (instructions as u64).div_ceil(64).max(2) * 64;
                let stop = m.run(1024);
                assert!(matches!(stop, Stop::Exceptions(1)), "busy={busy} instructions={instructions} vq={vq}: {stop:?}");
                if vq > 1 && std::env::var_os("ESP32SIM_VQ_NATIVE").is_some() {
                    assert!(m.vq_stats[0] > 0);
                }
                results.push((m.bus.cycles, m.cores.iter().map(|c| (c.ccount, c.insn_count, c.pc, c.ps)).collect::<Vec<_>>()));
            }
            assert_eq!(results[0], results[1], "busy={busy} instructions={instructions}");
        }
    }
}
