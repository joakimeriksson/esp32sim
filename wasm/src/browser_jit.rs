//! Receipt-priced SRAM sidecar handoff, separate from the production block JIT.
use super::Emu;
use esp_soc::Stop;
use esp32sim_wasm_jit::{compile_shared_sram_block, supported_max_ar, REGISTER_COUNT};
use xtensa_lx7::{decode, Bus as _};

const JIT_STATE_LEN: usize = 80;
const JIT_MODULE_LIMIT: usize = 1024;

pub(super) struct BrowserJit {
    state: Box<[u8; JIT_STATE_LEN]>,
    modules: Vec<CachedJitModule>,
    ticket: Option<JitTicket>,
}

struct CachedJitModule {
    pc: u32,
    code: Vec<u8>,
    module: Vec<u8>,
    receipt_cycles: u64,
}

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

impl Default for BrowserJit {
    fn default() -> Self { Self { state: Box::new([0; JIT_STATE_LEN]), modules: Vec::new(), ticket: None } }
}

/// Offer the browser one complete receipt-priced, side-effect-free S3 SRAM scheduling quantum.
/// A nonzero return value is a stable module id; zero means the normal interpreter must run. The
/// generated module shares this module's exported memory and writes only an internal handoff
/// record; architectural state changes only after `esp32sim_jit_commit` validates it.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_jit_prepare(e: *mut Emu, requested: u32, unix_ms: f64) -> u32 {
    let e = unsafe { &mut *e };
    if !e.booted {
        return 0;
    }
    esp_soc::host::set_unix_time_ms(unix_ms as u64);
    let Emu { m, jit, .. } = e;
    let Some(machine) = m.s3_mut() else {
        return 0;
    };
    prepare_browser_jit(machine, jit, requested).unwrap_or(0)
}

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
        let Some(max_ar) = supported_max_ar(&instruction) else { break };
        if instruction.len == 0 || window_overflow_possible(cpu, max_ar) { break; }
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

fn window_overflow_possible(cpu: &xtensa_lx7::Cpu, max_ar: u8) -> bool {
    use xtensa_lx7::state::ps;
    if max_ar < 4 || cpu.ps & ps::WOE == 0 || cpu.ps & ps::EXCM != 0 {
        return false;
    }
    (1..=u32::from(max_ar / 4)).any(|frame| {
        cpu.windowstart & (1 << ((cpu.windowbase + frame) & 15)) != 0
    })
}

fn write_jit_state(jit: &mut BrowserJit, cpu: &xtensa_lx7::Cpu, pc: u32) {
    for register in 0..REGISTER_COUNT {
        store_jit_u32(&mut jit.state[..], register * 4, cpu.get_ar(register as u8));
    }
    store_jit_u32(&mut jit.state[..], esp32sim_wasm_jit::PC_OFFSET, pc);
    jit.state[esp32sim_wasm_jit::CYCLE_OFFSET..esp32sim_wasm_jit::CYCLE_OFFSET + 8]
        .copy_from_slice(&0u64.to_le_bytes());
}

fn store_jit_u32(state: &mut [u8], offset: usize, value: u32) {
    state[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn load_jit_u32(state: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(state[offset..offset + 4].try_into().unwrap())
}

fn load_jit_u64(state: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(state[offset..offset + 8].try_into().unwrap())
}

/// Commit the prepared sidecar result. Returns 1 when committed and 0 when validation failed.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_jit_commit(e: *mut Emu) -> u32 {
    let e = unsafe { &mut *e };
    let Some(ticket) = e.jit.ticket.take() else {
        return 0;
    };
    let Some(machine) = e.m.s3_mut() else {
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
        crate::machine::reboot(machine);
    }
    1
}

/// Discard a prepared sidecar result after the generated module trapped.
///
/// # Safety
/// `e` must point to a live emulator to which the caller has exclusive access.
#[no_mangle]
pub unsafe extern "C" fn esp32sim_jit_abort(e: *mut Emu) {
    unsafe { &mut *e }.jit.ticket = None;
}

/// Pointer to the currently prepared sidecar module, or null without a ticket.
///
/// # Safety
/// `e` must point to a live emulator and no mutable access may overlap this call.
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
#[no_mangle]
pub unsafe extern "C" fn esp32sim_jit_module_len(e: *mut Emu) -> usize {
    let e = unsafe { &*e };
    let Some(ticket) = &e.jit.ticket else {
        return 0;
    };
    e.jit.modules[(ticket.module_id - 1) as usize].module.len()
}
