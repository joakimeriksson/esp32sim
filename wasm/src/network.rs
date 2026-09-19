//! A network of C6 motes on the same simulated 802.15.4 medium.
use super::{bytes, text, log, mib_bytes};
use crate::machine::MachineApi;

// The single-machine ABI runs one emulator; this one runs several on a shared medium
// (`esp32c6::net`), so the page can boot a whole 802.15.4 network with no simulator behind it.
// The stepping and the medium stay in Rust — the caller only says how far to run and reads what
// came out — because that is where `run_until_cycle` and `radio_receive` already make the timing
// exact, and where a native test can hold it to that (`esp32c6/tests/net.rs`).

/// A network plus the buffers its accessors hand out.
pub struct Net { net: esp32c6::net::Network, console: Vec<u8> }

/// A new network. `slice_ns` is how far every node runs before the medium looks again (0: the
/// default 100 µs); shorter is more exact and slower.
#[no_mangle] pub extern "C" fn esp32sim_net_new(slice_ns: f64) -> *mut Net {
    std::panic::set_hook(Box::new(|info| log(&format!("[emu] panic: {}", info))));
    let mut net = esp32c6::net::Network::new();
    if slice_ns > 0.0 { net.slice_ns = slice_ns as u64; }
    Box::into_raw(Box::new(Net { net, console: Vec::new() }))
}

/// Destroy a network returned by `esp32sim_net_new`. A null pointer is ignored.
///
/// # Safety
/// A non-null `n` must be the live pointer returned by `esp32sim_net_new`, with exclusive access,
/// and must not be used again.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_delete(n: *mut Net) {
    // SAFETY: The caller returns the live allocation with unique ownership.
    if !n.is_null() { drop(unsafe { Box::from_raw(n) }); }
}

/// Add a node and return its index, or `u32::MAX` if flash exceeds 32 MiB. `mac` is six bytes — two nodes must not share one, since
/// Contiki takes its link-layer address from the efuses and drops a frame that looks like its
/// own. `start_ns` staggers the power-on: identical images booted together stay in lockstep and
/// collide forever. `board` may be empty for a bare module.
///
/// # Safety
/// `n` must point to a live network with exclusive access; `mac` must be readable for 6 bytes and
/// `board` for `board_len` bytes throughout this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_add(n: *mut Net, mac: *const u8, flash_mb: u32, start_ns: f64, x: f64, y: f64, board: *const u8, board_len: usize) -> u32 {
    // SAFETY: The caller provides exclusive access to a live network.
    let n = unsafe { &mut *n };
    // SAFETY: The caller provides a readable six-byte MAC for this call.
    let mac_bytes = unsafe { bytes(mac, 6) };
    let mut m = [0u8; 6];
    m.copy_from_slice(&mac_bytes[..6.min(mac_bytes.len())]);
    // SAFETY: The caller provides a readable board name for this call.
    let board = unsafe { text(board, board_len) };
    let Some(flash) = mib_bytes(flash_mb.max(1)) else { log("[emu] net flash size exceeds 32 MiB"); return u32::MAX };
    n.net.add(m, flash, start_ns.max(0.0) as u64, x, y, board) as u32
}

/// Load an image into one node: the same `kind` numbering as `esp32sim_load`.
///
/// # Safety
/// `n` must point to a live network with exclusive access; for nonzero `len`, `ptr` must be
/// readable for `len` bytes throughout this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_load(n: *mut Net, node: u32, kind: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: The caller provides exclusive access to a live network.
    let n = unsafe { &mut *n };
    // SAFETY: The caller provides a readable input buffer for this call.
    let data = unsafe { bytes(ptr, len) };
    let Some(node) = n.net.nodes.get_mut(node as usize) else { return 1 };
    let r = node.m.load(kind, data);
    match r { Ok(()) => 0, Err(msg) => { log(&format!("[emu] net load kind {}: {}", kind, msg)); 1 } }
}

/// Stub a function on one node, by symbol name or hexadecimal address.
///
/// # Safety
/// `n` must point to a live network with exclusive access; `name` must be readable for `len`
/// bytes throughout this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_stub(n: *mut Net, node: u32, name: *const u8, len: usize, value: u32) -> u32 {
    // SAFETY: The caller provides exclusive access to a live network.
    let n = unsafe { &mut *n };
    // SAFETY: The caller provides a readable symbol name for this call.
    let name = unsafe { text(name, len) };
    let Some(node) = n.net.nodes.get_mut(node as usize) else { return 1 };
    node.m.stub(name, value)
}

/// Boot every node from its reset vector.
///
/// # Safety
/// `n` must point to a live network to which the caller has exclusive access.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_boot(n: *mut Net) -> u32 {
    // SAFETY: The caller provides exclusive access to a live network.
    let n = unsafe { &mut *n };
    if n.net.nodes.is_empty() { log("[emu] net: no nodes"); return 1; }
    n.net.boot(); 0
}

/// Advance the whole network to `until_ns` of network time.
///
/// # Safety
/// `n` must point to a live network to which the caller has exclusive access.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_run(n: *mut Net, until_ns: f64) {
    // SAFETY: The caller provides exclusive access to a live network.
    let n = unsafe { &mut *n };
    n.net.run_until(until_ns.max(0.0) as u64);
}

/// Network time in nanoseconds.
///
/// # Safety
/// `n` must point to a live network, and no mutable access may overlap this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_now_ns(n: *mut Net) -> f64 {
    // SAFETY: The caller provides shared access without overlapping mutation.
    unsafe { &*n }.net.now_ns as f64
}

/// Collect one node's console bytes into the network's buffer and return how many there are;
/// `esp32sim_net_console_ptr` then reads them, until the next call.
///
/// # Safety
/// `n` must point to a live network to which the caller has exclusive access.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_console_take(n: *mut Net, node: u32) -> usize {
    // SAFETY: The caller provides exclusive access to a live network.
    let n = unsafe { &mut *n };
    n.console = n.net.take_console(node as usize);
    n.console.len()
}

/// The bytes from the last `esp32sim_net_console_take`.
///
/// # Safety
/// `n` must point to a live network, and no mutable access may overlap this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_console_ptr(n: *mut Net) -> *const u8 {
    // SAFETY: The caller provides shared access without overlapping mutation.
    unsafe { &*n }.console.as_ptr()
}

/// One counter of one node: 0 frames sent, 1 frames taken, 2 frames refused (a collision or a
/// radio not listening), 3 the node's own clock in ns, 4 its WS2812 as 0xRRGGBB, 5 that LED's
/// change count, 6 nonzero once the node has halted.
///
/// # Safety
/// `n` must point to a live network, and no mutable access may overlap this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_net_stat(n: *mut Net, node: u32, which: u32) -> f64 {
    // SAFETY: The caller provides shared access without overlapping mutation.
    let n = unsafe { &*n };
    let i = node as usize;
    let Some(nd) = n.net.nodes.get(i) else { return 0.0 };
    match which {
        0 => nd.tx as f64, 1 => nd.rx as f64, 2 => nd.rx_dropped as f64,
        3 => nd.now_ns() as f64,
        4 => n.net.led(i).0 as f64, 5 => n.net.led(i).1 as f64,
        6 => nd.halted as u32 as f64,
        _ => 0.0,
    }
}
