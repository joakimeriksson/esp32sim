//! The emulator as a WebAssembly module. A C ABI, hand-driven from `web/wasm/worker.js` — no
//! bindgen, no dependencies. The machine produces exactly the messages the WebSocket UI speaks
//! (`docs/web-ui.md`); here they are queued in a `WebServer::queued()` sink and handed to JS.
//!
//! Lifecycle: `esp32sim_new` → `esp32sim_load` (ROM, bootloader, partition table, app, ELF
//! symbols, script) → optional `esp32sim_wifi` → `esp32sim_boot` → repeated `esp32sim_run(cycles,
//! unix_ms)` with `esp32sim_out_*` draining the outbox after each slice and `esp32sim_in_*`
//! feeding the page's inputs.
use esp_soc::observers::{BlockProfile, Coverage, IrqLatency};
use esp_soc::web::{json_escape, WebServer};
use esp_soc::{Machine, Soc, SocBus, Stop};
use std::any::Any;

#[cfg(target_arch = "wasm32")]
use esp32sim_wasm_jit::{compile_shared_sram_block, REGISTER_COUNT};
#[cfg(target_arch = "wasm32")]
use xtensa_lx7::{decode, Bus as _, Op};

/// The dozen calls the ABI makes, over whichever chip this instance is.
trait MachineApi {
    fn load(&mut self, kind: u32, d: &[u8], txt: &str) -> Result<(), String>;
    fn write_flash(&mut self, off: usize, d: &[u8]) -> Result<(), String>;
    fn boot(&mut self, app_direct: bool) -> Result<(), String>;
    fn board_name(&self) -> String;
    fn web(&self) -> Option<&WebServer>;
    fn run_slice(&mut self, cycles: u32) -> u32;
    fn cpu_hz(&self) -> f64;
    fn cycles(&self) -> f64;
    fn insns(&self) -> f64;
    fn stub(&mut self, name: &str, value: u32) -> u32;
    fn observer(&mut self, name: &str, arg: &str) -> u32;
    fn reports(&mut self) -> String;
    fn set_jit(&mut self, enabled: bool);
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<S: Soc> MachineApi for Machine<S> {
    fn load(&mut self, kind: u32, d: &[u8], txt: &str) -> Result<(), String> {
        match kind {
            0 => self.load_rom(d),
            1 => self.write_flash(0, d), 2 => self.write_flash(0x8000, d), 3 => self.write_flash(0x10000, d),
            4 => self.add_symbols(d),
            5 => self.write_flash(0, d),
            6 => self.load_script(txt),
            7 => esp_soc::picture::parse(d).map(|p| self.bus.board().set_camera_picture(p)),
            _ => Err(format!("unknown load kind {}", kind)),
        }
    }
    fn write_flash(&mut self, off: usize, d: &[u8]) -> Result<(), String> { Machine::write_flash(self, off, d) }
    fn boot(&mut self, app_direct: bool) -> Result<(), String> { if app_direct { self.boot_app(0x10000).map(|_| ()) } else { self.boot_rom(); Ok(()) } }
    fn board_name(&self) -> String { self.bus.board_ref().name().to_string() }
    fn web(&self) -> Option<&WebServer> { self.web.as_ref() }
    fn run_slice(&mut self, cycles: u32) -> u32 {
        self.max_cycles = self.bus.cycles() + cycles as u64;
        loop {
            match self.run(u64::MAX) {
                Stop::Halted | Stop::MaxInsns => return 0,
                Stop::SwReset => {
                    let cause = self.bus.reset_cause();
                    let note = format!("[emu] chip reset at t={:.3}s: cause {:#x} ({})", self.seconds(), cause, esp_periph::reset_cause_name(cause));
                    log(&note);
                    if let Some(w) = &self.web { w.send_text(&format!("{{\"t\":\"emu\",\"msg\":\"{}\"}}", json_escape(&note))); }
                    self.reboot();
                }
                Stop::Unimplemented(pc, raw) => { log(&format!("[emu] unimplemented instruction at {:08x} {} (raw {:#x})", pc, self.sym(pc), raw)); return 2; }
                Stop::Ebreak(pc) => { log(&format!("[emu] ebreak at {:08x} {}", pc, self.sym(pc))); return 3; }
                Stop::Breakpoint(_) => return 3,
                Stop::Exceptions(_) => return 4,
                Stop::Simcall(_) => return 5,
                Stop::Watch(..) => return 6,
                Stop::CostModel { reason, .. } | Stop::CostModelLifecycle { reason, .. } => { log(&format!("[emu] cost model: {}", reason)); return 7; }
            }
        }
    }
    fn cpu_hz(&self) -> f64 { S::CPU_HZ as f64 }
    fn cycles(&self) -> f64 { self.bus.cycles() as f64 }
    fn insns(&self) -> f64 { Machine::insns(self) as f64 }
    fn stub(&mut self, name: &str, value: u32) -> u32 {
        let by_addr = name.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok());
        match by_addr.or_else(|| self.sym_addr(name)) {
            Some(addr) => { self.stubs.insert(addr, value); log(&format!("[emu] stub {} @ {:#x} -> returns {:#x}", name, addr, value)); 0 }
            None => { log(&format!("[emu] stub: no symbol '{}' (load the app ELF first)", name)); 1 }
        }
    }
    fn observer(&mut self, name: &str, arg: &str) -> u32 {
        match name {
            "profile-blocks" => { self.add_observer(Box::new(BlockProfile::new(20))); 0 }
            "coverage" => { self.add_observer(Box::new(Coverage::new(None))); 0 }
            "irq-latency" => { self.add_observer(Box::new(IrqLatency::new(S::CORES))); 0 }
            "trace-fn" => { let n: Vec<(u32, String)> = self.symbols.iter().filter(|(_, s)| s.starts_with(arg)).map(|(a, s)| (*a, s.clone())).collect(); for (a, s) in n { self.fn_probes.insert(a, s); } 0 }
            _ => { log(&format!("[emu] unknown observer '{}'", name)); 1 }
        }
    }
    fn reports(&mut self) -> String { Machine::reports(self) }
    fn set_jit(&mut self, enabled: bool) { for core in &mut self.cores { xtensa_lx7::Core::set_jit(core, enabled); } }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" { fn host_log(ptr: *const u8, len: usize); }
#[cfg(not(target_arch = "wasm32"))]
unsafe fn host_log(ptr: *const u8, len: usize) {
    // SAFETY: The caller provides a readable string pointer for exactly `len` bytes.
    let message = unsafe { std::slice::from_raw_parts(ptr, len) };
    eprintln!("{}", String::from_utf8_lossy(message));
}

fn log(s: &str) {
    // SAFETY: `s` is readable for its length and the host does not retain the pointer.
    unsafe { host_log(s.as_ptr(), s.len()); }
}

pub struct Emu {
    m: Box<dyn MachineApi>,
    /// the last drained outbox: (1 text | 2 binary, payload), addressed by index from JS
    out: Vec<(u8, Vec<u8>)>,
    booted: bool,
    #[cfg(target_arch = "wasm32")]
    jit: BrowserJit,
}

#[cfg(target_arch = "wasm32")]
const JIT_STATE_LEN: usize = 80;
#[cfg(target_arch = "wasm32")]
const JIT_MODULE_LIMIT: usize = 1024;

#[cfg(target_arch = "wasm32")]
struct BrowserJit {
    state: Box<[u8; JIT_STATE_LEN]>,
    modules: Vec<CachedJitModule>,
    ticket: Option<JitTicket>,
}

#[cfg(target_arch = "wasm32")]
struct CachedJitModule {
    pc: u32,
    code: Vec<u8>,
    module: Vec<u8>,
    receipt_cycles: u64,
}

#[cfg(target_arch = "wasm32")]
struct JitTicket {
    module_id: u32,
    pc: u32,
    next_pc: u32,
    last_pc: u32,
    ccount: u32,
    insns: u64,
    bus_cycles: u64,
    instruction_count: u32,
    receipt_cycles: u64,
    code_pages: Vec<(u32, u32)>,
}

/// Borrow an ABI buffer.
///
/// # Safety
/// For nonzero `len`, `ptr` must be non-null and readable for `len` bytes. The memory must remain
/// unchanged and valid for the returned lifetime. A null pointer is accepted only when `len` is 0.
unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 { &[] } else {
        // SAFETY: The caller supplies the validity, immutability, and lifetime guarantees above.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

/// Borrow a UTF-8 ABI buffer, treating invalid UTF-8 as an empty string.
///
/// # Safety
/// The pointer and length must satisfy `bytes`'s contract.
unsafe fn text<'a>(ptr: *const u8, len: usize) -> &'a str {
    // SAFETY: The caller satisfies `bytes`'s pointer and lifetime contract.
    std::str::from_utf8(unsafe { bytes(ptr, len) }).unwrap_or("")
}

/// Buffers the page fills before handing them to `esp32sim_load` / `esp32sim_in_*`.
#[no_mangle] pub extern "C" fn esp32sim_alloc(len: usize) -> *mut u8 { let mut v = vec![0u8; len.max(1)]; let p = v.as_mut_ptr(); std::mem::forget(v); p }
/// Release a buffer returned by `esp32sim_alloc`.
///
/// # Safety
/// `ptr` must be the live pointer returned by `esp32sim_alloc(len)`. It must not be used again.
#[no_mangle] pub unsafe extern "C" fn esp32sim_free(ptr: *mut u8, len: usize) {
    // SAFETY: The caller returns the allocation with the same length and unique ownership.
    drop(unsafe { Vec::from_raw_parts(ptr, len.max(1), len.max(1)) });
}

/// `board` is a CLI board name (atech14, waveshare-cam, waveshare-lcd4b,
/// waveshare-amoled18-v2, none) for the ESP32-S3,
/// or `esp32c3` for the RISC-V chip, which is console-only and takes no board. Null on failure.
///
/// # Safety
/// For nonzero `board_len`, `board` must be non-null and readable for `board_len` bytes throughout
/// this call. A null pointer is accepted only when `board_len` is 0.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_new(board: *const u8, board_len: usize, flash_mb: u32, psram_mb: u32) -> *mut Emu {
    std::panic::set_hook(Box::new(|info| log(&format!("[emu] panic: {}", info))));
    // SAFETY: The caller provides a readable board-name buffer for this call.
    let board = unsafe { text(board, board_len) }.to_string();
    let (flash_mb, psram_mb) = (flash_mb.max(1) as usize, psram_mb as usize);
    let m: Box<dyn MachineApi> = if board == "esp32c3" || board == "c3" {
        let mut m = esp32c3::machine([0x3c, 0x84, 0x27, 0xb6, 0xa7, 0x1c], flash_mb << 20);
        m.bus.set_flash_size(flash_mb << 20);
        m.console.mask = 2;                                  // the ROM mirrors its console to UART0 and USB-Serial/JTAG
        prepare(&mut m);
        Box::new(m)
    } else if board == "esp32c6" || board == "c6" || board.starts_with("waveshare-c6") || board.ends_with("lcd147") {
        let mut m = esp32c6::machine([0xdc, 0x1e, 0xd5, 0x6e, 0x8c, 0xdc], flash_mb << 20);
        let name = if board == "esp32c6" || board == "c6" { "none" } else { board.as_str() };
        let Some(b) = esp32c6::board::make_board(name) else { log(&format!("[emu] unknown board '{}'", board)); return std::ptr::null_mut() };
        m.bus.board = b;
        m.bus.set_flash_size(flash_mb << 20);
        m.console.mask = 2;
        prepare(&mut m);
        Box::new(m)
    } else {
        let mut m = esp32s3::machine([0x44, 0x1b, 0xf6, 0x75, 0xdc, 0xe0]);
        let Some(b) = esp32s3::board::make_board(&board) else { log(&format!("[emu] unknown board '{}'", board)); return std::ptr::null_mut() };
        m.bus.board = b;
        m.bus.attach_board_devices();
        m.bus.set_flash_size(flash_mb << 20);
        let _ = m.bus.set_psram_size(psram_mb << 20);
        m.bus.periph.lcd_cam.frame_cycles = esp32s3::periph::CPU_HZ / 10;
        prepare(&mut m);
        Box::new(m)
    };
    Box::into_raw(Box::new(Emu {
        m,
        out: Vec::new(),
        booted: false,
        #[cfg(target_arch = "wasm32")]
        jit: BrowserJit {
            state: Box::new([0; JIT_STATE_LEN]),
            modules: Vec::new(),
            ticket: None,
        },
    }))
}

/// The page is the one client: messages queue in a `WebServer` sink; the worker paces the run.
fn prepare<S: Soc>(m: &mut Machine<S>) {
    m.web = Some(WebServer::queued());
    m.rt.enabled = false;                                    // std::time does not exist here
    m.console.capture = true;
}

/// Destroy an emulator returned by `esp32sim_new`. A null pointer is ignored.
///
/// # Safety
/// A non-null `e` must be the live pointer returned by `esp32sim_new`. The caller must have
/// exclusive access, and the pointer must not be used again.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_delete(e: *mut Emu) {
    if !e.is_null() {
        // SAFETY: The caller returns the live allocation with unique ownership.
        drop(unsafe { Box::from_raw(e) });
    }
}

/// kind: 0 mask-ROM ELF, 1 bootloader (flash 0x0), 2 partition table (0x8000), 3 app (0x10000),
/// 4 ELF for symbols, 5 whole flash image (0x0), 6 script text, 7 camera picture (BMP/PPM).
/// Returns 0, or 1 with the reason logged.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access. For nonzero `len`,
/// `ptr` must be non-null and readable for `len` bytes throughout this call.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_load(e: *mut Emu, kind: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    // SAFETY: The caller provides a readable input buffer for this call.
    let data = unsafe { bytes(ptr, len) };
    let input_text = std::str::from_utf8(data).unwrap_or("");
    match e.m.load(kind, data, input_text) { Ok(()) => 0, Err(msg) => { log(&format!("[emu] load kind {}: {}", kind, msg)); 1 } }
}

/// Write bytes into flash at an arbitrary offset (a data partition's contents).
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access. For nonzero `len`,
/// `ptr` must be non-null and readable for `len` bytes throughout this call.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_load_at(e: *mut Emu, offset: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    // SAFETY: The caller provides a readable input buffer for this call.
    let data = unsafe { bytes(ptr, len) };
    match e.m.write_flash(offset as usize, data) { Ok(()) => 0, Err(msg) => { log(&format!("[emu] flash {:#x}: {}", offset, msg)); 1 } }
}

/// Attach the virtual access point and subnet: `ssid=NAME,psk=PASS,chan=N`. No NAT — the browser
/// has no sockets — so DHCP, DNS, SNTP and ICMP answer, and connections past the gateway are refused.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access. For nonzero `len`,
/// `spec` must be non-null and readable for `len` bytes throughout this call.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_wifi(e: *mut Emu, spec: *const u8, len: usize) {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    let Some(m) = e.m.as_any_mut().downcast_mut::<esp32s3::Machine>() else { log("[emu] wifi: the C3 radio is not modelled"); return };
    let mut cfg = esp32s3::wifi::ApConfig { ssid: "esp32sim".into(), bssid: [0x02, 0x53, 0x49, 0x4d, 0x00, 0x01], channel: 6, psk: None };
    // SAFETY: The caller provides a readable setup string for this call.
    for kv in unsafe { text(spec, len) }.split(',') {
        match kv.split_once('=') {
            Some(("ssid", v)) => cfg.ssid = v.to_string(),
            Some(("chan", v)) | Some(("channel", v)) => cfg.channel = v.parse().unwrap_or(6),
            Some(("psk", v)) | Some(("password", v)) => cfg.psk = Some(v.to_string()),
            _ => {}
        }
    }
    log(&format!("[emu] virtual AP '{}' ({}), subnet 10.0.2.0/24, no NAT in the browser", cfg.ssid, if cfg.psk.is_some() { "WPA2-PSK" } else { "open" }));
    m.bus.periph.wifi.ap = Some(esp32s3::wifi::VirtualAp::new(cfg, m.bus.debug.has("wifi-frames")));
    m.bus.periph.wifi.net = Some(esp32s3::net::VirtualNet::new(m.bus.debug.has("net")));
}

/// `--stub NAME[=value]`: return `value` immediately when execution reaches the function's entry.
/// NAME is a symbol (needs the ELF loaded) or a hex address. Returns 1 if it cannot be resolved.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access. For nonzero `len`,
/// `name` must be non-null and readable for `len` bytes throughout this call.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_stub(e: *mut Emu, name: *const u8, len: usize, value: u32) -> u32 {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    // SAFETY: The caller provides a readable symbol name for this call.
    e.m.stub(unsafe { text(name, len) }, value)
}

/// Attach an analysis: `profile-blocks`, `coverage`, `irq-latency` (no argument), `trace-fn`
/// (arg = symbol prefix as text). Returns 1 for an unknown name.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access. For each nonzero
/// length, its corresponding `name` or `arg` pointer must be non-null and readable throughout this
/// call.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_observer(e: *mut Emu, name: *const u8, len: usize, arg: *const u8, arg_len: usize) -> u32 {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    // SAFETY: The caller provides readable name and argument buffers for this call.
    e.m.observer(unsafe { text(name, len) }, unsafe { text(arg, arg_len) })
}

/// Every observer's report so far, as `emu` messages in the outbox.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_reports(e: *mut Emu) {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    let r = e.m.reports();
    if let Some(w) = e.m.web() { for line in r.lines() { w.send_text(&format!("{{\"t\":\"emu\",\"msg\":\"{}\"}}", json_escape(line))); } }
}

/// Start from the mask ROM (the normal path) or, with `app_direct` set, straight into the app image.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_boot(e: *mut Emu, app_direct: u32) -> u32 {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    if let Err(msg) = e.m.boot(app_direct != 0) { log(&format!("[emu] boot: {}", msg)); return 1; }
    // the WebSocket server announces the board in its per-client hello; here there is one client
    let name = e.m.board_name();
    if let Some(w) = e.m.web() { w.send_text(&format!("{{\"t\":\"board\",\"name\":\"{}\"}}", name)); }
    e.booted = true; 0
}

/// Run for `cycles` more emulated cycles. Returns 0 while the machine can go on; otherwise a stop
/// code: 2 unimplemented instruction, 3 breakpoint/ebreak, 4 exception limit, 5 semihosting call.
/// A chip reset (esp_restart, watchdog) reboots through the ROM and keeps going, like the CLI.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_run(e: *mut Emu, cycles: u32, unix_ms: f64) -> u32 {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    if !e.booted { return 9; }
    #[cfg(target_arch = "wasm32")] esp_soc::host::set_unix_time_ms(unix_ms as u64);
    let _ = unix_ms;
    e.m.run_slice(cycles)
}

/// Offer the browser one complete receipt-priced, side-effect-free S3 SRAM scheduling quantum.
/// A nonzero return value is a stable module id; zero means the normal interpreter must run. The
/// generated module shares this module's exported memory and writes only an internal handoff
/// record; architectural state changes only after `esp32sim_jit_commit` validates it.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn esp32sim_jit_prepare(e: *mut Emu, requested: u32, unix_ms: f64) -> u32 {
    let e = unsafe { &mut *e };
    if !e.booted {
        return 0;
    }
    esp_soc::host::set_unix_time_ms(unix_ms as u64);
    let Emu { m, jit, .. } = e;
    let Some(machine) = m.as_any_mut().downcast_mut::<esp32s3::Machine>() else {
        return 0;
    };
    prepare_browser_jit(machine, jit, requested).unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
fn prepare_browser_jit(
    machine: &mut esp32s3::Machine,
    jit: &mut BrowserJit,
    requested: u32,
) -> Option<u32> {
    jit.ticket = None;
    let cpu = &machine.cores[0];
    let limit = machine.browser_external_block_budget(requested)?;
    for compare in cpu.ccompare {
        let distance = compare.wrapping_sub(cpu.ccount);
        if distance != 0 && distance < limit {
            return None;
        }
    }

    let (start_pc, mut pc) = (cpu.pc, cpu.pc);
    let mut code = Vec::with_capacity(limit as usize * 3);
    let mut last_pc = start_pc;
    let mut instruction_count = 0u32;
    while instruction_count < limit {
        let bytes = machine.bus.fetch(pc).ok()?;
        let instruction = decode(pc, bytes);
        if !matches!(
            instruction.op,
            Op::L32i | Op::L32iN | Op::MoviN | Op::Memw | Op::Sub | Op::Saltu
        ) || instruction.len == 0
            || window_overflow_possible(cpu, supported_max_ar(&instruction))
        {
            break;
        }
        let next_pc = pc.wrapping_add(u32::from(instruction.len));
        if cpu.lcount != 0 && next_pc == cpu.lend {
            break;
        }
        code.extend_from_slice(&bytes[..instruction.len as usize]);
        last_pc = pc;
        pc = next_pc;
        instruction_count += 1;
    }
    if instruction_count != limit {
        return None;
    }

    let mut page_indices = Vec::new();
    let last_byte = pc.wrapping_sub(1);
    let page_size = 1u32 << xtensa_lx7::bus::VPAGE_SHIFT;
    let mut page_address = start_pc;
    loop {
        let index = machine.bus.code_page(page_address);
        if page_indices.last() != Some(&index) {
            page_indices.push(index);
        }
        if page_address / page_size == last_byte / page_size {
            break;
        }
        page_address = (page_address / page_size + 1) * page_size;
    }
    let versions = machine.bus.page_versions();
    let code_pages = page_indices
        .into_iter()
        .map(|index| (index, versions.get(index as usize).copied().unwrap_or(0)))
        .collect();

    let state_offset = u32::try_from(jit.state.as_ptr() as usize).ok()?;
    let dram_len = (esp32s3::bus::DRAM_HIGH - esp32s3::bus::DRAM_LOW) as usize;
    let dram_storage_offset = machine.bus.sram.len().checked_sub(dram_len)?;
    // SAFETY: `dram_storage_offset` was derived by subtracting `dram_len` from this allocation.
    let dram_ptr = unsafe { machine.bus.sram.as_ptr().add(dram_storage_offset) };
    let dram_offset = u32::try_from(dram_ptr as usize).ok()?;
    let module_index = if let Some(index) = jit
        .modules
        .iter()
        .position(|cached| cached.pc == start_pc && cached.code == code)
    {
        index
    } else {
        if jit.modules.len() >= JIT_MODULE_LIMIT {
            return None;
        }
        let compiled = compile_shared_sram_block(
            start_pc,
            &code,
            state_offset,
            dram_offset,
            esp32s3::bus::DRAM_LOW,
            dram_len,
        )
        .ok()?;
        let receipt_cycles = compiled.cycle_cost;
        jit.modules.push(CachedJitModule {
            pc: start_pc,
            code: code.clone(),
            module: compiled.bytes,
            receipt_cycles,
        });
        jit.modules.len() - 1
    };

    let receipt_cycles = jit.modules[module_index].receipt_cycles;
    write_jit_state(jit, cpu, start_pc);
    let module_id = module_index as u32 + 1;
    jit.ticket = Some(JitTicket {
        module_id,
        pc: start_pc,
        next_pc: pc,
        last_pc,
        ccount: cpu.ccount,
        insns: cpu.insn_count,
        bus_cycles: machine.bus.cycles,
        instruction_count,
        receipt_cycles,
        code_pages,
    });
    Some(module_id)
}

#[cfg(target_arch = "wasm32")]
fn window_overflow_possible(cpu: &xtensa_lx7::Cpu, max_ar: u8) -> bool {
    use xtensa_lx7::state::ps;
    if max_ar < 4 || cpu.ps & ps::WOE == 0 || cpu.ps & ps::EXCM != 0 {
        return false;
    }
    (1..=u32::from(max_ar / 4)).any(|frame| {
        cpu.windowstart & (1 << ((cpu.windowbase + frame) & 15)) != 0
    })
}

#[cfg(target_arch = "wasm32")]
fn supported_max_ar(instruction: &xtensa_lx7::Insn) -> u8 {
    match instruction.op {
        Op::L32i | Op::L32iN => instruction.s.max(instruction.t),
        Op::MoviN => instruction.s,
        Op::Sub | Op::Saltu => instruction.r.max(instruction.s).max(instruction.t),
        Op::Memw => 0,
        _ => unreachable!("called only after the supported-opcode check"),
    }
}

#[cfg(target_arch = "wasm32")]
fn write_jit_state(jit: &mut BrowserJit, cpu: &xtensa_lx7::Cpu, pc: u32) {
    for register in 0..REGISTER_COUNT {
        store_jit_u32(&mut jit.state[..], register * 4, cpu.get_ar(register as u8));
    }
    store_jit_u32(&mut jit.state[..], esp32sim_wasm_jit::PC_OFFSET, pc);
    jit.state[esp32sim_wasm_jit::CYCLE_OFFSET..esp32sim_wasm_jit::CYCLE_OFFSET + 8]
        .copy_from_slice(&0u64.to_le_bytes());
}

#[cfg(target_arch = "wasm32")]
fn store_jit_u32(state: &mut [u8], offset: usize, value: u32) {
    state[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(target_arch = "wasm32")]
fn load_jit_u32(state: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(state[offset..offset + 4].try_into().unwrap())
}

#[cfg(target_arch = "wasm32")]
fn load_jit_u64(state: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(state[offset..offset + 8].try_into().unwrap())
}

/// Commit the prepared sidecar result. Returns 1 when committed and 0 when validation failed.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn esp32sim_jit_commit(e: *mut Emu) -> u32 {
    let e = unsafe { &mut *e };
    let Some(ticket) = e.jit.ticket.take() else {
        return 0;
    };
    let Some(machine) = e.m.as_any_mut().downcast_mut::<esp32s3::Machine>() else {
        return 0;
    };
    let cpu = &machine.cores[0];
    let versions = machine.bus.page_versions();
    let unchanged = cpu.pc == ticket.pc
        && cpu.ccount == ticket.ccount
        && cpu.insn_count == ticket.insns
        && machine.bus.cycles == ticket.bus_cycles
        && ticket.code_pages.iter().all(|&(index, version)| {
            versions.get(index as usize).copied().unwrap_or(0) == version
        })
        && load_jit_u32(&e.jit.state[..], esp32sim_wasm_jit::PC_OFFSET) == ticket.next_pc
        && load_jit_u64(&e.jit.state[..], esp32sim_wasm_jit::CYCLE_OFFSET) == ticket.receipt_cycles
        && machine
            .browser_external_block_budget(ticket.instruction_count)
            .is_some_and(|budget| budget >= ticket.instruction_count);
    if !unchanged {
        return 0;
    }

    let cpu = &mut machine.cores[0];
    for register in 0..REGISTER_COUNT {
        cpu.set_ar(
            register as u8,
            load_jit_u32(&e.jit.state[..], register * 4),
        );
    }
    cpu.pc = ticket.next_pc;
    cpu.insn_count += u64::from(ticket.instruction_count);
    cpu.advance_ccount(ticket.instruction_count);
    machine.bus.note_pc(ticket.last_pc);
    if matches!(
        machine.finish_browser_external_quantum(),
        Some(Stop::SwReset)
    ) {
        let cause = machine.bus.reset_cause();
        let note = format!(
            "[emu] chip reset at t={:.3}s: cause {:#x} ({})",
            machine.seconds(),
            cause,
            esp_periph::reset_cause_name(cause)
        );
        log(&note);
        if let Some(web) = &machine.web {
            web.send_text(&format!(
                "{{\"t\":\"emu\",\"msg\":\"{}\"}}",
                json_escape(&note)
            ));
        }
        machine.reboot();
    }
    1
}

/// Discard a prepared sidecar result after the generated module trapped.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn esp32sim_jit_abort(e: *mut Emu) {
    unsafe { &mut *e }.jit.ticket = None;
}

/// Pointer to the currently prepared sidecar module, or null without a ticket.
///
/// # Safety
/// `e` must point to a live emulator and no mutable access may overlap this call.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn esp32sim_jit_module_ptr(e: *mut Emu) -> *const u8 {
    let e = unsafe { &*e };
    let Some(ticket) = &e.jit.ticket else {
        return std::ptr::null();
    };
    e.jit.modules[(ticket.module_id - 1) as usize].module.as_ptr()
}

/// Length of the currently prepared sidecar module.
///
/// # Safety
/// `e` must point to a live emulator and no mutable access may overlap this call.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn esp32sim_jit_module_len(e: *mut Emu) -> usize {
    let e = unsafe { &*e };
    let Some(ticket) = &e.jit.ticket else {
        return 0;
    };
    e.jit.modules[(ticket.module_id - 1) as usize].module.len()
}

/// The emulated CPU clock, so the driver paces the right chip: 240 MHz on the S3, 160 on the C3.
///
/// # Safety
/// `e` must point to a live emulator, and no mutable access may overlap this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_cpu_hz(e: *mut Emu) -> f64 {
    // SAFETY: The caller provides shared access to a live emulator without overlapping mutation.
    unsafe { &*e }.m.cpu_hz()
}
/// Return the current emulated cycle count.
///
/// # Safety
/// `e` must point to a live emulator, and no mutable access may overlap this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_cycles(e: *mut Emu) -> f64 {
    // SAFETY: The caller provides shared access to a live emulator without overlapping mutation.
    unsafe { &*e }.m.cycles()
}
/// Return the current emulated instruction count.
///
/// # Safety
/// `e` must point to a live emulator, and no mutable access may overlap this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_insns(e: *mut Emu) -> f64 {
    // SAFETY: The caller provides shared access to a live emulator without overlapping mutation.
    unsafe { &*e }.m.insns()
}

/// Drain what the machine sent since the last call; then index it with the accessors below.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_out_take(e: *mut Emu) -> u32 {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    e.out = e.m.web().map(|w| w.take_outbox()).unwrap_or_default();
    e.out.len() as u32
}
/// Return the kind of one message from the last output drain, or 0 for an invalid index.
///
/// # Safety
/// `e` must point to a live emulator, and no mutable access may overlap this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_out_kind(e: *mut Emu, i: u32) -> u32 {
    // SAFETY: The caller provides shared access to a live emulator without overlapping mutation.
    unsafe { &*e }.out.get(i as usize).map(|m| m.0 as u32).unwrap_or(0)
}
/// Return a message's data pointer, or null for an invalid index.
///
/// # Safety
/// `e` must point to a live emulator, and no mutable access may overlap this call. A non-null
/// result remains valid until the next `esp32sim_out_take` or `esp32sim_delete` call for `e`.
#[no_mangle] pub unsafe extern "C" fn esp32sim_out_ptr(e: *mut Emu, i: u32) -> *const u8 {
    // SAFETY: The caller provides shared access to a live emulator without overlapping mutation.
    unsafe { &*e }.out.get(i as usize).map(|m| m.1.as_ptr()).unwrap_or(std::ptr::null())
}
/// Return a message's data length, or 0 for an invalid index.
///
/// # Safety
/// `e` must point to a live emulator, and no mutable access may overlap this call.
#[no_mangle] pub unsafe extern "C" fn esp32sim_out_len(e: *mut Emu, i: u32) -> usize {
    // SAFETY: The caller provides shared access to a live emulator without overlapping mutation.
    unsafe { &*e }.out.get(i as usize).map(|m| m.1.len()).unwrap_or(0)
}

/// Page input in the WebSocket JSON protocol.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access. For nonzero `len`,
/// `ptr` must be non-null and readable for `len` bytes throughout this call.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_in_text(e: *mut Emu, ptr: *const u8, len: usize) {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    // SAFETY: The caller provides a readable input buffer for this call.
    let input = unsafe { text(ptr, len) };
    if let Some(w) = e.m.web() { w.push_incoming(input.to_string()); }
}
/// Page input in the WebSocket binary protocol.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access. For nonzero `len`,
/// `ptr` must be non-null and readable for `len` bytes throughout this call.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_in_bin(e: *mut Emu, ptr: *const u8, len: usize) {
    // SAFETY: The caller provides exclusive access to a live emulator.
    let e = unsafe { &mut *e };
    // SAFETY: The caller provides a readable input buffer for this call.
    let input = unsafe { bytes(ptr, len) };
    if let Some(w) = e.m.web() { w.push_incoming_bin(input.to_vec()); }
}

/// Enable or disable the scheduler-integrated block JIT. The interpreter remains available.
///
/// # Safety
/// `e` must be a live exclusively borrowed emulator.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_set_jit(e: *mut Emu, enabled: u32) {
    // SAFETY: The ABI caller guarantees a live exclusive handle.
    unsafe { &mut *e }.m.set_jit(enabled != 0);
}

/// Guest instructions retired by compiled blocks, including interpreter helpers.
///
/// # Safety
/// `e` must be a live exclusively borrowed emulator.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_block_jit_insns(e: *mut Emu) -> f64 {
    // SAFETY: The ABI caller guarantees a live exclusive handle.
    let e = unsafe { &mut *e };
    e.m.as_any_mut().downcast_mut::<esp32s3::Machine>()
        .map(|m| m.cores.iter().map(|c| c.blocks.jit_instructions).sum::<u64>() as f64)
        .unwrap_or(0.0)
}

#[cfg(all(target_arch = "wasm32", feature = "jit-tests"))]
mod jit_tests;

/// Emit the optional statistical block profile through host_log.
///
/// # Safety
/// `e` must be a live exclusively borrowed emulator.
#[cfg(all(target_arch = "wasm32", feature = "jit-profile"))]
#[no_mangle]
pub unsafe extern "C" fn esp32sim_profile_report(e: *mut Emu) {
    // SAFETY: the ABI caller guarantees a live exclusive handle.
    if let Some(m) = unsafe { &mut *e }.m.as_any_mut().downcast_mut::<esp32s3::Machine>() {
        for (i, core) in m.cores.iter().enumerate() {
            log(&format!("core={i}\n{}", core.blocks.profile.report()));
        }
    }
}

/// Runs the generated-code differential suite in a real WASM runtime.
#[cfg(all(target_arch = "wasm32", feature = "jit-tests"))]
#[no_mangle]
pub extern "C" fn esp32sim_test_block_jit() -> u32 {
    std::panic::set_hook(Box::new(|info| log(&format!("[jit test] {info}"))));
    xtensa_lx7::jit::tests::run_tests() + jit_tests::run()
}

// ---------------------------------------------------------------- a network of C6 motes
//
// The single-machine ABI above runs one emulator; this one runs several on a shared medium
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

/// Add a node and return its index. `mac` is six bytes — two nodes must not share one, since
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
    let flash = (flash_mb.max(1) as usize) << 20;
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
    let m = &mut node.m;
    let r = match kind {
        0 => m.load_rom(data),
        1 => m.write_flash(0x0, data), 2 => m.write_flash(0x8000, data), 3 => m.write_flash(0x10000, data),
        4 => m.add_symbols(data),
        5 => m.write_flash(0x0, data),
        _ => Err(format!("unknown load kind {}", kind)),
    };
    match r { Ok(()) => 0, Err(msg) => { log(&format!("[emu] net load kind {}: {}", kind, msg)); 1 } }
}

/// Stub a function on one node, by symbol name or `0x`-prefixed address.
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
    let addr = node.m.sym_addr(name).or_else(|| u32::from_str_radix(name.trim_start_matches("0x"), 16).ok());
    match addr { Some(a) => { node.m.stubs.insert(a, value); 0 } None => { log(&format!("[emu] net stub: unknown symbol {}", name)); 1 } }
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
