//! Two emulated ESP32-C6 motes on one medium, running the Contiki-NG nullnet image that the
//! `--cooja` integration is tested against. Needs the mask ROM ELF and that build, so it is
//! ignored by default like the other firmware tests.
#[path = "../../tests/common.rs"]
mod common;
use esp32c6::net::Network;

fn contiki_dir() -> String {
    std::env::var("CONTIKI_C6_DIR").unwrap_or_else(|_| format!("{}/work/esp32/esp32-contiki/build-nullnet", std::env::var("HOME").unwrap_or_default()))
}

fn load(net: &mut Network, i: usize, rom: &[u8], dir: &str) {
    let n = &mut net.nodes[i].m;
    n.load_rom(rom).expect("rom");
    n.write_flash(0x0, &std::fs::read(format!("{dir}/bootloader/bootloader.bin")).expect("bootloader")).unwrap();
    n.write_flash(0x8000, &std::fs::read(format!("{dir}/partition_table/partition-table.bin")).expect("ptable")).unwrap();
    n.write_flash(0x10000, &std::fs::read(format!("{dir}/esp32-blink.bin")).expect("app")).unwrap();
    let elf = std::fs::read(format!("{dir}/esp32-blink.elf")).expect("elf");
    n.add_symbols(&elf).unwrap();
    // the PHY blob's baseband calibration spins on undocumented registers; the MAC does not need it
    let bb = n.sym_addr("bb_init").expect("bb_init in the app ELF");
    n.stubs.insert(bb, 0);
}

/// A broadcast from one node reaches the other, and Contiki's nullnet layer accepts it — which
/// means the whole path works: TX_START cuts the slice, the frame goes on the air at its first
/// preamble byte, the receiver's address filter passes it and the application sees the payload.
#[test] #[ignore = "needs the ESP32-C6 mask ROM ELF and CONTIKI_C6_DIR (a Contiki-NG nullnet build)"]
fn two_motes_exchange_nullnet_broadcasts() {
    let rom = std::fs::read(common::rom("esp32c6_rev0")).expect("rom elf");
    let dir = contiki_dir();
    let mut net = Network::new();
    // different MACs: Contiki takes its link-layer address from the efuses and drops a frame
    // that looks like its own. Staggered starts: two identical images booted together stay in
    // lockstep and collide forever (see the module docs).
    net.add([0x02, 0, 0, 0, 0, 1], 2 << 20, 0, 0.0, 0.0, "none");
    net.add([0x02, 0, 0, 0, 0, 2], 2 << 20, 1_300_000_000, 2.0, 0.0, "none");
    for i in 0..2 { load(&mut net, i, &rom, &dir); }
    net.boot();
    net.run_until(30_000_000_000);

    let out: Vec<String> = (0..2).map(|i| String::from_utf8_lossy(&net.take_console(i)).into_owned()).collect();
    for (i, text) in out.iter().enumerate() {
        assert!(text.contains("Starting Contiki-NG-ESP32C6"), "node {} never booted Contiki:\n{}", i, &text[text.len().saturating_sub(600)..]);
    }
    assert!(net.nodes[0].tx > 0 && net.nodes[1].tx > 0, "both nodes transmit: {} and {}", net.nodes[0].tx, net.nodes[1].tx);
    // the payload, not just the PHY: nullnet prints what it received and from whom
    let heard: Vec<bool> = out.iter().map(|t| t.contains("rx len")).collect();
    assert!(heard[0] || heard[1], "neither node's nullnet layer received a frame:\nnode0 tail:\n{}\nnode1 tail:\n{}",
            &out[0][out[0].len().saturating_sub(400)..], &out[1][out[1].len().saturating_sub(400)..]);
    assert!(net.nodes[0].rx + net.nodes[1].rx > 0, "no frame was taken by a radio");
}
