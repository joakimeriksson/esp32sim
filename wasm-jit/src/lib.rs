//! WebAssembly code generation for receipt-priced ESP32-S3 LX7 SRAM blocks.

use emu_core::{
    CostModel, ExecutionFacts, LifecycleFacts, LifecycleKind, MemoryAccess, MemoryAccessKind,
    StepKind, StepOutcome,
};
use esp32s3::bus::{DRAM_HIGH, DRAM_LOW};
use esp32s3::Esp32S3SramCostModel;
use std::fmt;
use xtensa_lx7::{decode, Insn, Op};

pub const REGISTER_COUNT: usize = 16;
pub const PC_OFFSET: usize = 64;
pub const CYCLE_OFFSET: usize = 72;
pub const SRAM_IMAGE_OFFSET: usize = 4096;

#[derive(Clone, Copy)]
struct MemoryLayout {
    registers: usize,
    pc: usize,
    cycles: usize,
    sram_image: usize,
}

const PRIVATE_LAYOUT: MemoryLayout = MemoryLayout {
    registers: 0,
    pc: PC_OFFSET,
    cycles: CYCLE_OFFSET,
    sram_image: SRAM_IMAGE_OFFSET,
};

struct CompileTarget<'a> {
    initial_registers: [u32; REGISTER_COUNT],
    initial_sram: Option<&'a [u8]>,
    sram_base: u32,
    sram_len: usize,
    layout: MemoryLayout,
    shared_memory: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    EmptyBlock,
    TruncatedInstruction { offset: usize, decoded_len: usize },
    UnsupportedInstruction { pc: u32, op: Op },
    InvalidSramRange { base: u32, len: usize },
    InvalidSharedMemoryRange { offset: u32, len: usize },
    TimingRefusal { pc: u32, reason: String },
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBlock => write!(f, "cannot compile an empty block"),
            Self::TruncatedInstruction {
                offset,
                decoded_len,
            } => write!(
                f,
                "instruction at block offset {offset} needs {decoded_len} bytes"
            ),
            Self::UnsupportedInstruction { pc, op } => {
                write!(f, "unsupported instruction {op:?} at {pc:#010x}")
            }
            Self::InvalidSramRange { base, len } => {
                write!(
                    f,
                    "SRAM image {base:#010x}+{len:#x} is outside internal DRAM"
                )
            }
            Self::InvalidSharedMemoryRange { offset, len } => write!(
                f,
                "shared WebAssembly memory range {offset:#010x}+{len:#x} exceeds 32-bit memory"
            ),
            Self::TimingRefusal { pc, reason } => {
                write!(f, "timing model refused {pc:#010x}: {reason}")
            }
        }
    }
}

impl std::error::Error for CompileError {}

#[derive(Clone, Debug)]
pub struct CompiledBlock {
    pub bytes: Vec<u8>,
    pub instruction_count: usize,
    pub cycle_cost: u64,
}

struct DecodedInstruction {
    pc: u32,
    bytes: [u8; 4],
    instruction: Insn,
}

enum SramInstruction {
    Load32 {
        destination: u8,
        base: u8,
        offset: i32,
    },
    MoveImmediate {
        destination: u8,
        value: i32,
    },
    Binary {
        destination: u8,
        left: u8,
        right: u8,
        opcode: u8,
    },
    Barrier,
}

impl SramInstruction {
    fn from_insn(instruction: &Insn) -> Option<Self> {
        let Insn {
            op, r, s, t, imm, ..
        } = *instruction;
        Some(match op {
            Op::L32i | Op::L32iN => Self::Load32 {
                destination: t,
                base: s,
                offset: imm,
            },
            Op::MoviN => Self::MoveImmediate {
                destination: s,
                value: imm,
            },
            Op::Sub => Self::Binary {
                destination: r,
                left: s,
                right: t,
                opcode: 0x6b,
            },
            Op::Saltu => Self::Binary {
                destination: r,
                left: s,
                right: t,
                opcode: 0x49,
            },
            Op::Memw => Self::Barrier,
            _ => return None,
        })
    }

    fn max_ar(&self) -> u8 {
        match *self {
            Self::Load32 {
                destination, base, ..
            } => destination.max(base),
            Self::MoveImmediate { destination, .. } => destination,
            Self::Binary {
                destination,
                left,
                right,
                ..
            } => destination.max(left).max(right),
            Self::Barrier => 0,
        }
    }
}

/// Return the highest AR used by an instruction supported by this SRAM emitter.
/// `memw` returns `Some(0)` because it uses no ARs; unsupported instructions return `None`.
pub fn supported_max_ar(instruction: &Insn) -> Option<u8> {
    SramInstruction::from_insn(instruction).map(|instruction| instruction.max_ar())
}

/// Compile one straight-line LX7 block whose data accesses are confined to `sram`.
pub fn compile_sram_block(
    base_pc: u32,
    block: &[u8],
    initial_registers: [u32; REGISTER_COUNT],
    sram_base: u32,
    sram: &[u8],
) -> Result<CompiledBlock, CompileError> {
    compile_sram_block_with_layout(
        base_pc,
        block,
        CompileTarget {
            initial_registers,
            initial_sram: Some(sram),
            sram_base,
            sram_len: sram.len(),
            layout: PRIVATE_LAYOUT,
            shared_memory: false,
        },
    )
}

/// Compile a block that operates directly on the browser emulator's exported WebAssembly
/// memory. `state_offset` addresses an 80-byte handoff record (16 ARs, PC at byte 64, and a
/// u64 receipt-cycle counter at byte 72); `sram_offset` is the host-memory address corresponding
/// to `sram_base`. The emitted module imports `env.memory`, so neither state nor SRAM is copied.
pub fn compile_shared_sram_block(
    base_pc: u32,
    block: &[u8],
    state_offset: u32,
    sram_offset: u32,
    sram_base: u32,
    sram_len: usize,
) -> Result<CompiledBlock, CompileError> {
    validate_sram_range(sram_base, sram_len)?;
    validate_shared_range(state_offset, CYCLE_OFFSET + 8)?;
    validate_shared_range(sram_offset, sram_len)?;
    let layout = MemoryLayout {
        registers: state_offset as usize,
        pc: state_offset as usize + PC_OFFSET,
        cycles: state_offset as usize + CYCLE_OFFSET,
        sram_image: sram_offset as usize,
    };
    compile_sram_block_with_layout(
        base_pc,
        block,
        CompileTarget {
            initial_registers: [0; REGISTER_COUNT],
            initial_sram: None,
            sram_base,
            sram_len,
            layout,
            shared_memory: true,
        },
    )
}

fn validate_shared_range(offset: u32, len: usize) -> Result<(), CompileError> {
    let valid = u32::try_from(len)
        .ok()
        .and_then(|len| offset.checked_add(len))
        .is_some();
    if valid {
        Ok(())
    } else {
        Err(CompileError::InvalidSharedMemoryRange { offset, len })
    }
}

fn compile_sram_block_with_layout(
    base_pc: u32,
    block: &[u8],
    target: CompileTarget<'_>,
) -> Result<CompiledBlock, CompileError> {
    let CompileTarget {
        initial_registers,
        initial_sram,
        sram_base,
        sram_len,
        layout,
        shared_memory,
    } = target;
    validate_sram_range(sram_base, sram_len)?;
    let instructions = decode_block(base_pc, block)?;
    let costs = price_sram_block(&instructions)?;
    let mut body = if shared_memory {
        Vec::new()
    } else {
        function_prefix()
    };
    if shared_memory {
        emit_entry_pc_guard(&mut body, base_pc, layout);
    }
    for (decoded, cost) in instructions.iter().zip(&costs) {
        if !shared_memory {
            emit_current_pc_guard(&mut body, decoded.pc, layout);
        }
        emit_sram_instruction(
            &mut body,
            decoded.pc,
            &decoded.instruction,
            sram_base,
            sram_len,
            layout,
        )?;
        emit_state_store_const(
            &mut body,
            layout.pc,
            decoded.pc.wrapping_add(u32::from(decoded.instruction.len)) as i32,
        );
        emit_cycle_charge(&mut body, u64::from(*cost), layout);
        if !shared_memory {
            emit_continue(&mut body);
        }
    }
    if shared_memory {
        body.push(0x0b);
    } else {
        finish_function(&mut body);
    }

    Ok(CompiledBlock {
        bytes: if shared_memory {
            wasm_shared_module(body)
        } else {
            let mut state = vec![0; SRAM_IMAGE_OFFSET + sram_len];
            for (index, value) in initial_registers.into_iter().enumerate() {
                store_u32(&mut state, index * 4, value);
            }
            store_u32(&mut state, PC_OFFSET, base_pc);
            if let Some(sram) = initial_sram {
                state[SRAM_IMAGE_OFFSET..].copy_from_slice(sram);
            }
            wasm_module(body, &state)
        },
        instruction_count: instructions.len(),
        cycle_cost: costs.iter().map(|cost| u64::from(*cost)).sum(),
    })
}

fn validate_sram_range(base: u32, len: usize) -> Result<(), CompileError> {
    let end = u32::try_from(len)
        .ok()
        .and_then(|len| base.checked_add(len));
    if len < 4 || base < DRAM_LOW || end.is_none_or(|end| end > DRAM_HIGH) {
        return Err(CompileError::InvalidSramRange { base, len });
    }
    Ok(())
}

fn decode_block(base_pc: u32, block: &[u8]) -> Result<Vec<DecodedInstruction>, CompileError> {
    if block.is_empty() {
        return Err(CompileError::EmptyBlock);
    }
    let mut offset = 0;
    let mut instructions = Vec::new();
    while offset < block.len() {
        let mut bytes = [0; 4];
        let available = (block.len() - offset).min(bytes.len());
        bytes[..available].copy_from_slice(&block[offset..offset + available]);
        let pc = base_pc.wrapping_add(offset as u32);
        let instruction = decode(pc, bytes);
        let len = instruction.len as usize;
        if len == 0 || len > block.len() - offset {
            return Err(CompileError::TruncatedInstruction {
                offset,
                decoded_len: len,
            });
        }
        instructions.push(DecodedInstruction {
            pc,
            bytes,
            instruction,
        });
        offset += len;
    }
    Ok(instructions)
}

fn price_sram_block(instructions: &[DecodedInstruction]) -> Result<Vec<u32>, CompileError> {
    let mut model = Esp32S3SramCostModel::new();
    model
        .lifecycle(&LifecycleFacts {
            kind: LifecycleKind::Attach,
            chip: "esp32s3",
            cores: 2,
            cpu_hz: 240_000_000,
        })
        .map_err(|reason| CompileError::TimingRefusal {
            pc: instructions[0].pc,
            reason,
        })?;

    instructions
        .iter()
        .map(|decoded| {
            let mut accesses = vec![MemoryAccess {
                kind: MemoryAccessKind::Fetch,
                address: decoded.pc,
                width: 4,
                value: u32::from_le_bytes(decoded.bytes),
                fault: None,
            }];
            if matches!(
                SramInstruction::from_insn(&decoded.instruction),
                Some(SramInstruction::Load32 { .. })
            ) {
                accesses.push(MemoryAccess {
                    kind: MemoryAccessKind::Read,
                    address: DRAM_LOW,
                    width: 4,
                    value: 0,
                    fault: None,
                });
            }
            model
                .cycles(&ExecutionFacts {
                    core: 0,
                    outcome: StepOutcome {
                        pc: decoded.pc,
                        next_pc: decoded.pc.wrapping_add(u32::from(decoded.instruction.len)),
                        bytes: Some(decoded.bytes),
                        length: decoded.instruction.len,
                        kind: StepKind::Retired,
                        control: None,
                    },
                    accesses: &accesses,
                })
                .map_err(|reason| CompileError::TimingRefusal {
                    pc: decoded.pc,
                    reason,
                })
        })
        .collect()
}

fn emit_sram_instruction(
    body: &mut Vec<u8>,
    pc: u32,
    instruction: &Insn,
    sram_base: u32,
    sram_len: usize,
    layout: MemoryLayout,
) -> Result<(), CompileError> {
    let instruction =
        SramInstruction::from_insn(instruction).ok_or(CompileError::UnsupportedInstruction {
            pc,
            op: instruction.op,
        })?;
    match instruction {
        SramInstruction::Load32 {
            destination,
            base,
            offset,
        } => {
            emit_i32_const(
                body,
                (layout.registers + usize::from(destination) * 4) as i32,
            );
            emit_sram_alignment_guard(body, base, offset, layout);
            emit_sram_bounds_guard(body, base, offset, sram_base, sram_len, layout);
            emit_sram_address(body, base, offset, sram_base, layout);
            body.extend_from_slice(&[0x28, 0x02, 0x00]);
            emit_i32_store(body);
        }
        SramInstruction::MoveImmediate { destination, value } => {
            emit_i32_const(
                body,
                (layout.registers + usize::from(destination) * 4) as i32,
            );
            emit_i32_const(body, value);
            emit_i32_store(body);
        }
        SramInstruction::Binary {
            destination,
            left,
            right,
            opcode,
        } => {
            emit_i32_const(
                body,
                (layout.registers + usize::from(destination) * 4) as i32,
            );
            emit_register_load(body, left, layout);
            emit_register_load(body, right, layout);
            body.push(opcode);
            emit_i32_store(body);
        }
        SramInstruction::Barrier => {}
    }
    Ok(())
}

fn emit_sram_alignment_guard(body: &mut Vec<u8>, base: u8, offset: i32, layout: MemoryLayout) {
    emit_register_load(body, base, layout);
    emit_i32_const(body, offset);
    body.push(0x6a);
    emit_i32_const(body, 3);
    body.push(0x71);
    emit_trap_if_true(body);
}

fn emit_sram_bounds_guard(
    body: &mut Vec<u8>,
    base: u8,
    offset: i32,
    sram_base: u32,
    sram_len: usize,
    layout: MemoryLayout,
) {
    emit_sram_address(body, base, offset, sram_base, layout);
    emit_i32_const(body, layout.sram_image as i32);
    body.push(0x49);
    emit_trap_if_true(body);

    emit_sram_address(body, base, offset, sram_base, layout);
    emit_i32_const(body, (layout.sram_image + sram_len - 4) as i32);
    body.push(0x4b);
    emit_trap_if_true(body);
}

fn emit_trap_if_true(body: &mut Vec<u8>) {
    body.extend_from_slice(&[0x04, 0x40, 0x00, 0x0b]);
}

fn emit_sram_address(
    body: &mut Vec<u8>,
    base: u8,
    offset: i32,
    sram_base: u32,
    layout: MemoryLayout,
) {
    emit_register_load(body, base, layout);
    emit_i32_const(body, offset);
    body.push(0x6a);
    emit_i32_const(body, sram_base as i32);
    body.push(0x6b);
    emit_i32_const(body, layout.sram_image as i32);
    body.push(0x6a);
}

fn function_prefix() -> Vec<u8> {
    vec![0x02, 0x40, 0x03, 0x40]
}

fn emit_current_pc_guard(body: &mut Vec<u8>, pc: u32, layout: MemoryLayout) {
    emit_state_load(body, layout.pc);
    emit_i32_const(body, pc as i32);
    body.push(0x46);
    body.extend_from_slice(&[0x04, 0x40]);
}

fn emit_entry_pc_guard(body: &mut Vec<u8>, pc: u32, layout: MemoryLayout) {
    emit_state_load(body, layout.pc);
    emit_i32_const(body, pc as i32);
    body.push(0x47);
    emit_trap_if_true(body);
}

fn emit_continue(body: &mut Vec<u8>) {
    body.extend_from_slice(&[0x0c, 0x01, 0x0b]);
}

fn finish_function(body: &mut Vec<u8>) {
    body.extend_from_slice(&[0x0c, 0x01, 0x0b, 0x0b, 0x0b]);
}

fn emit_register_load(body: &mut Vec<u8>, register: u8, layout: MemoryLayout) {
    emit_i32_const(body, (layout.registers + usize::from(register) * 4) as i32);
    body.extend_from_slice(&[0x28, 0x02, 0x00]);
}

fn emit_state_load(body: &mut Vec<u8>, offset: usize) {
    emit_i32_const(body, offset as i32);
    body.extend_from_slice(&[0x28, 0x02, 0x00]);
}

fn emit_state_store_const(body: &mut Vec<u8>, offset: usize, value: i32) {
    emit_i32_const(body, offset as i32);
    emit_i32_const(body, value);
    emit_i32_store(body);
}

fn emit_i32_store(body: &mut Vec<u8>) {
    body.extend_from_slice(&[0x36, 0x02, 0x00]);
}

fn emit_cycle_charge(body: &mut Vec<u8>, cycles: u64, layout: MemoryLayout) {
    emit_i32_const(body, layout.cycles as i32);
    emit_i32_const(body, layout.cycles as i32);
    body.extend_from_slice(&[0x29, 0x03, 0x00]);
    body.push(0x42);
    push_sleb(body, cycles as i64);
    body.push(0x7c);
    body.extend_from_slice(&[0x37, 0x03, 0x00]);
}

fn emit_i32_const(body: &mut Vec<u8>, value: i32) {
    body.push(0x41);
    push_sleb(body, i64::from(value));
}

fn store_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn wasm_module(body: Vec<u8>, state: &[u8]) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    append_section(&mut module, 1, &[1, 0x60, 0, 0]);
    append_section(&mut module, 3, &[1, 0]);
    let pages = state.len().div_ceil(65_536).max(1);
    let mut memory = vec![1, 0];
    push_uleb(&mut memory, pages);
    append_section(&mut module, 5, &memory);
    let mut exports = Vec::new();
    push_uleb(&mut exports, 2);
    append_name(&mut exports, "memory");
    exports.extend_from_slice(&[2, 0]);
    append_name(&mut exports, "run");
    exports.extend_from_slice(&[0, 0]);
    append_section(&mut module, 7, &exports);
    let mut code = Vec::new();
    push_uleb(&mut code, 1);
    push_uleb(&mut code, body.len() + 1);
    code.push(0);
    code.extend_from_slice(&body);
    append_section(&mut module, 10, &code);
    let mut data = vec![1, 0, 0x41, 0, 0x0b];
    push_uleb(&mut data, state.len());
    data.extend_from_slice(state);
    append_section(&mut module, 11, &data);
    module
}

fn wasm_shared_module(body: Vec<u8>) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    append_section(&mut module, 1, &[1, 0x60, 0, 0]);
    let mut imports = Vec::new();
    push_uleb(&mut imports, 1);
    append_name(&mut imports, "env");
    append_name(&mut imports, "memory");
    imports.extend_from_slice(&[2, 0, 0]);
    append_section(&mut module, 2, &imports);
    append_section(&mut module, 3, &[1, 0]);
    let mut exports = Vec::new();
    push_uleb(&mut exports, 1);
    append_name(&mut exports, "run");
    exports.extend_from_slice(&[0, 0]);
    append_section(&mut module, 7, &exports);
    let mut code = Vec::new();
    push_uleb(&mut code, 1);
    push_uleb(&mut code, body.len() + 1);
    code.push(0);
    code.extend_from_slice(&body);
    append_section(&mut module, 10, &code);
    module
}

fn append_section(module: &mut Vec<u8>, id: u8, contents: &[u8]) {
    module.push(id);
    push_uleb(module, contents.len());
    module.extend_from_slice(contents);
}

fn append_name(bytes: &mut Vec<u8>, name: &str) {
    push_uleb(bytes, name.len());
    bytes.extend_from_slice(name.as_bytes());
}

fn push_uleb(bytes: &mut Vec<u8>, mut value: usize) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_sleb(bytes: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}
