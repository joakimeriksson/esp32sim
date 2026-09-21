//! Native fast-memory bounds must fault through the bus before any host pointer access.
#![cfg(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux")))]

use emu_core::bus::{tlb_index, FastMem, TlbEntry, TLB_ENTRIES};
use xtensa_lx7::block::run_block;
use xtensa_lx7::state::exc;
use xtensa_lx7::{decode, Bus, Cpu, Fault, FlatRam, Op, Trap};

const CODE: u32 = 0x4037_0000;
// This valid mapping collides with the final guest address in the TLB hash.
const DATA: u32 = 0x0101_0000;
const LEN: u32 = 256;
const VALUE: u32 = 0x1234_5678;
const ACCESSES: [(u8, Op, u32); 6] = [
    (0, Op::L8ui, 1), (1, Op::L16ui, 2), (2, Op::L32i, 4),
    (4, Op::S8i, 1), (5, Op::S16i, 2), (6, Op::S32i, 4),
];

struct MemoryBus {
    code: FlatRam,
    data: FlatRam,
    tlb: Box<[TlbEntry; TLB_ENTRIES]>,
    versions: [u32; 2],
    helper_calls: u32,
}

impl Bus for MemoryBus {
    fn read8(&mut self, a: u32) -> Result<u8, Fault> { self.helper_calls += 1; self.data.read8(a) }
    fn read16(&mut self, a: u32) -> Result<u16, Fault> { self.helper_calls += 1; self.data.read16(a) }
    fn read32(&mut self, a: u32) -> Result<u32, Fault> { self.helper_calls += 1; self.data.read32(a) }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), Fault> { self.helper_calls += 1; self.data.write8(a, v) }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), Fault> { self.helper_calls += 1; self.data.write16(a, v) }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), Fault> { self.helper_calls += 1; self.data.write32(a, v) }
    fn fetch(&mut self, pc: u32) -> Result<[u8; 4], Fault> { self.code.fetch(pc) }
    fn page_versions(&self) -> &[u32] { &self.versions }
    fn fast_mem(&mut self) -> Option<FastMem> {
        Some(FastMem { tlb: self.tlb.as_ptr(), page_ver: self.versions.as_mut_ptr() })
    }
}

fn compiled_access(opcode: u8, op: Op) -> (Cpu, MemoryBus) {
    let mut bus = MemoryBus {
        code: FlatRam::new(CODE, 16),
        data: FlatRam::new(DATA, LEN as usize),
        tlb: Box::new([TlbEntry::EMPTY; TLB_ENTRIES]),
        versions: [0; 2],
        helper_calls: 0,
    };
    // load/store a3, a4, 0; j self
    bus.code.mem[..6].copy_from_slice(&[0x32, opcode << 4 | 4, 0, 0x06, 0xff, 0xff]);
    let insn = decode(CODE, bus.code.fetch(CODE).unwrap());
    assert_eq!((insn.op, insn.s, insn.t, insn.imm), (op, 4, 3, 0));
    bus.tlb[tlb_index(DATA)] = TlbEntry {
        lo: DATA, hi: DATA + LEN, base: bus.data.mem.as_mut_ptr(),
        vbase: 1, writable: 1, off: 0, src: 0,
        // EX110: decoded code depends on this mapping, so generated stores bump its versions.
        code: 1,
        span: 0,
    }
    .with_span();
    let mut cpu = Cpu::new(0);
    cpu.ps = 0;
    cpu.set_ar(4, DATA + 32);
    cpu.set_ar(3, VALUE);
    cpu.pc = CODE;
    assert_eq!(run_block(&mut cpu, &mut bus, 1), (1, None));
    assert!(cpu.blocks.jit_instructions > 0, "must exercise compiled memory access");
    (cpu, bus)
}

fn assert_fault(cpu: &mut Cpu, bus: &mut MemoryBus, opcode: u8, op: Op, addr: u32) {
    let before = bus.data.mem.clone();
    let compiled = cpu.blocks.jit_instructions;
    let helpers = bus.helper_calls;
    cpu.pc = CODE;
    cpu.set_ar(4, addr);
    let cause = if opcode < 4 { exc::LOAD_PROHIBITED } else { exc::STORE_PROHIBITED };
    assert_eq!(run_block(cpu, bus, 1), (1, Some(Trap::Exception(cause))), "{op:?}");
    assert_eq!(cpu.excvaddr, addr, "{op:?}");
    assert_eq!(cpu.epc[1], CODE, "{op:?}");
    assert_eq!(cpu.blocks.jit_instructions, compiled + 1, "{op:?}");
    assert_eq!(bus.helper_calls, helpers + 1, "{op:?}: the bus must decide the fault");
    assert_eq!(bus.data.mem, before, "{op:?}: a fault must not modify memory");
}

fn wrapping_addresses_fault(empty: bool) {
    for (opcode, op, width) in ACCESSES {
        let (mut cpu, mut bus) = compiled_access(opcode, op);
        let addr = u32::MAX - width + 1; // aligned, but addr + width wraps to zero
        assert_eq!(tlb_index(addr), tlb_index(DATA));
        if empty { bus.tlb.fill(TlbEntry::EMPTY); }
        assert_fault(&mut cpu, &mut bus, opcode, op, addr);
    }
}

#[test]
fn empty_tlb_rejects_wrapping_accesses() { wrapping_addresses_fault(true); }

#[test]
fn aliased_tlb_rejects_wrapping_accesses() { wrapping_addresses_fault(false); }

#[test]
fn access_crossing_mapping_limit_faults() {
    for (opcode, op, width) in ACCESSES {
        let (mut cpu, mut bus) = compiled_access(opcode, op);
        // Truncation preserves the allocation while moving the exclusive limit back one byte.
        bus.data.mem.truncate((LEN - 1) as usize);
        bus.tlb[tlb_index(DATA)].hi -= 1;
        bus.tlb[tlb_index(DATA)] = bus.tlb[tlb_index(DATA)].with_span();
        assert_fault(&mut cpu, &mut bus, opcode, op, DATA + LEN - width);
    }
}

#[test]
fn access_ending_at_mapping_limit_stays_on_fast_path() {
    for (opcode, op, width) in ACCESSES {
        let (mut cpu, mut bus) = compiled_access(opcode, op);
        bus.data.mem.fill(0x5a);
        let helpers = bus.helper_calls;
        let compiled = cpu.blocks.jit_instructions;
        cpu.pc = CODE;
        cpu.set_ar(4, DATA + LEN - width);
        cpu.set_ar(3, VALUE);
        assert_eq!(run_block(&mut cpu, &mut bus, 1), (1, None), "{op:?}");
        assert_eq!(cpu.blocks.jit_instructions, compiled + 1, "{op:?}");
        assert_eq!(bus.helper_calls, helpers, "{op:?}: a valid access must stay fast");
        if opcode < 4 {
            let mask = u32::MAX >> (32 - width * 8);
            assert_eq!(cpu.get_ar(3), 0x5a5a_5a5a & mask, "{op:?}");
        } else {
            assert_eq!(&bus.data.mem[(LEN - width) as usize..], &VALUE.to_le_bytes()[..width as usize], "{op:?}");
        }
    }
}
