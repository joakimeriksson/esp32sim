//! The station's NAT carried by the page: the guest's sockets past the gateway become records the
//! page takes after every run (`esp32sim_nat_take`), and the page answers with events
//! (`esp32sim_nat_event`) from whatever network it has (a WebSocket relay, say). The NAT and its
//! TCP stay here; the page sees connects, data, shutdowns and closes per flow
//! (`esp_soc::nat::queue`).
use super::{bytes, log, Emu};
use esp_soc::nat::{HostEvent, Nat, QueueTransport};

/// Carry the network behind the access point (`esp32sim_wifi`, call it first and again after
/// each new one) through the page. `flags` bit 0: on (TCP and UDP past the gateway become records);
/// without it, back to no NAT. Bit 1: the page resolves the guest's A / AAAA queries (`RESOLVE`
/// records); without it DNS datagrams go to the page as UDP to 10.0.2.3:53. Bit 2: SNTP is
/// answered here, from the network clock, instead of going to the page. A chip reset aborts the
/// open flows (`TCP_CLOSE` records with abort); a second call starts a new queue, forgetting the
/// old flows without records. Returns 0, or 1 without a network (the reason goes to `host_log`).
/// # Safety
/// `e` must be a live emulator with exclusive access for this call.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_nat_relay(e: *mut Emu, flags: u32) -> u32 {
    let e = unsafe { &mut *e };
    let (nat, handle) = if flags & 1 == 0 { (None, None) } else {
        let (transport, handle) = QueueTransport::with_dns(flags & 2 != 0);
        (Some(Nat::with_transport(false, Box::new(transport))), Some(handle))
    };
    if let Err(reason) = e.m.attach_nat(nat, flags & 1 != 0 && flags & 4 != 0) { log(&format!("[emu] nat relay: {reason}")); return 1 }
    e.nat = handle;
    0
}

/// Take the records the guest's sockets queued since the last take, returning their byte length
/// (zero when none, or without `esp32sim_nat_relay`). The bytes are at `esp32sim_nat_ptr`, valid
/// until the next take or emulator deletion: records of `u8 op, u8 flags, u16 port, u32 id, u32 len`
/// and `len` bytes, little-endian, each padded to a multiple of 4 bytes. op 1 TCP_CONNECT (port,
/// data = IPv4[4]), 2 TCP_DATA, 3 TCP_SHUTDOWN, 4 TCP_CLOSE (flags bit 0: abort), 5 UDP_OPEN (port,
/// IPv4[4]), 6 UDP_DATA, 7 UDP_CLOSE, 8 RESOLVE (id = query, port = qtype 1 A / 28 AAAA, data = the
/// name in UTF-8).
/// # Safety
/// `e` must be a live emulator with exclusive access for this call.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_nat_take(e: *mut Emu) -> usize {
    let e = unsafe { &mut *e };
    match &e.nat {
        Some(h) => { h.take_ops(&mut e.nat_out); }
        None => e.nat_out.clear(),
    }
    e.nat_out.len()
}

/// Bytes from `esp32sim_nat_take`, valid until the next take or emulator deletion.
/// # Safety
/// `e` must be live and readable. The returned bytes must not be modified.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_nat_ptr(e: *const Emu) -> *const u8 {
    unsafe { &*e }.nat_out.as_ptr()
}

/// Deliver one event from the page. `kind` 1 CONNECTED (`arg` = bytes the page takes before its
/// first TX_CREDIT), 2 FAILED (`arg` 1 refused, 2 unreachable or timed out, 3 policy, 4 offline:
/// the guest gets RST; an open flow is reset), 3 TCP_DATA (returns the bytes taken: the rest is
/// offered again later; all of them for a flow the guest has forgotten), 4 TCP_EOF, 5 TCP_RESET
/// (after the bytes already taken), 6 TX_CREDIT (`arg` bytes), 7 UDP_DATA (1 taken, 0 dropped:
/// 16 wait per flow), 8 RESOLVED (`id` = query, `arg` = TTL, data = n × IPv4[4], none for an empty
/// answer), 9 UNRESOLVED (`arg` = rcode 2 SERVFAIL or 3 NXDOMAIN). The others return 1 when
/// applied, 0 for a flow or query not waiting for it, an unknown kind or no relay. No TCP_CLOSE
/// record follows FAILED or TCP_RESET.
/// # Safety
/// `e` must be live and exclusive; `ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_nat_event(e: *mut Emu, kind: u32, id: u32, arg: u32, ptr: *const u8, len: usize) -> u32 {
    let e = unsafe { &mut *e };
    let data = unsafe { bytes(ptr, len) };
    let (Some(h), Some(ev)) = (&e.nat, HostEvent::from_abi(kind, id, arg, data)) else { return 0 };
    h.event(ev)
}
