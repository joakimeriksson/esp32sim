//! EX110/EX173/EX180: generated stores must observe real S3 code-watch transitions.
use xtensa_lx7::{block::run_block, bus::{tlb_index, Bus}, Core, Cpu};
use esp32s3::bus::{SocBus, DBUS_LOW, DRAM_LOW, IRAM_LOW, MMU_SPIRAM, MMU_TABLE};

const WRITER: u32 = IRAM_LOW;
const VALUE: u32 = 0x1234_5678;

fn store(cpu: &mut Cpu, bus: &mut SocBus, addr: u32) {
    cpu.pc = WRITER;
    cpu.set_ar(4, addr);
    cpu.set_ar(3, VALUE);
    let compiled = cpu.blocks.jit_instructions;
    assert_eq!(run_block(cpu, bus, 1), (1, None));
    assert!(cpu.blocks.jit_instructions > compiled, "store must execute generated WASM");
    assert_eq!(bus.read32(addr).unwrap(), VALUE);
}

pub fn run() -> u32 {
    // First/last 256-byte pages of a 64 KiB code-watch group must watch the
    // neighboring group. Use the DRAM alias for stores and IRAM for decode.
    for (code_offset, write_offset) in [(0x2_0000, 0x1_fffc), (0x2_ff00, 0x3_0000), (0x2_0010, 0x2_0000)] {
        let mut bus = SocBus::new(65536, 65536, [0; 6]);
        let mut cpu = Cpu::new(0);
        cpu.ps = 0;
        cpu.set_jit(true);
        // s32i a3,a4,0; j self. Warm the generated store on an unrelated group.
        bus.load_bytes(WRITER, &[0x32, 0x64, 0, 0x06, 0xff, 0xff]).unwrap();
        cpu.set_ar(4, DRAM_LOW + 0x4_8000);
        for _ in 0..64 {
            cpu.pc = WRITER;
            assert_eq!(run_block(&mut cpu, &mut bus, 1), (1, None));
        }
        assert!(cpu.blocks.jit_instructions > 0);
        let target = DRAM_LOW + write_offset - 0x8000;
        let unrelated = DRAM_LOW + 0x4_8000;
        let code = IRAM_LOW + code_offset;
        bus.load_bytes(code, &[0x3d, 0xf0, 0x06, 0xff, 0xff]).unwrap(); // nop.n; j self
        let before = bus.page_versions().to_vec();
        store(&mut cpu, &mut bus, target);
        assert_eq!(bus.page_versions(), before, "unwatched generated store must skip versions");
        // The store above published a code=0 entry. Decode must invalidate it
        // when watching the IRAM alias, even for the adjacent 64 KiB group.
        let fast = bus.fast_mem().unwrap();
        assert_eq!(unsafe { (*fast.tlb.add(tlb_index(target))).code }, 0);
        let mut reader = Cpu::new(1);
        reader.ps = 0;
        reader.set_jit(false);
        reader.pc = code;
        assert_ne!(tlb_index(target), tlb_index(code), "decode must not evict the stale entry by collision");
        assert_eq!(run_block(&mut reader, &mut bus, 1), (1, None));
        let before = bus.page_versions().to_vec();
        // Refill once after invalidation, then exercise the fast generated path.
        bus.read32(target).unwrap();
        assert_eq!(unsafe { (*fast.tlb.add(tlb_index(target))).code }, 1);
        let page = bus.code_page(target) as usize;
        store(&mut cpu, &mut bus, target);
        assert_eq!(bus.page_versions()[page], before[page].wrapping_add(1));
        if write_offset & 255 < 3 {
            assert_eq!(bus.page_versions()[page - 1], before[page - 1].wrapping_add(1), "EX180 previous-page bump");
        }
        let before = bus.page_versions().to_vec();
        store(&mut cpu, &mut bus, unrelated);
        assert_eq!(bus.page_versions(), before, "unrelated group must remain unwatched");
    }
    // shell-s2: a generated store to the first bytes of PSRAM bumps the last flash page without the
    // bus, so that page must stay outside the pages the flash epoch vouches for.
    let mut bus = SocBus::new(65536, 65536, [0; 6]);
    let mut cpu = Cpu::new(0);
    cpu.ps = 0;
    cpu.set_jit(true);
    bus.load_bytes(WRITER, &[0x32, 0x64, 0, 0x06, 0xff, 0xff]).unwrap();
    bus.write32(MMU_TABLE, MMU_SPIRAM).unwrap();
    let first = bus.code_page(DBUS_LOW);
    bus.note_code_page(first);
    cpu.set_ar(4, DBUS_LOW);
    for _ in 0..64 {
        cpu.pc = WRITER;
        assert_eq!(run_block(&mut cpu, &mut bus, 1), (1, None));
    }
    let (lo, hi, epoch) = bus.stable_pages();
    let before = bus.page_versions().to_vec();
    store(&mut cpu, &mut bus, DBUS_LOW);
    assert_eq!(bus.page_versions()[first as usize - 1], before[first as usize - 1].wrapping_add(1), "EX180 previous-page bump");
    assert!(bus.stable_pages().2 != epoch || bus.page_versions()[lo as usize..hi as usize] == before[lo as usize..hi as usize],
        "a generated store changed a version the flash epoch vouches for");
    4
}
