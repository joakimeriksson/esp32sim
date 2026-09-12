//! Memory bus abstraction between a core and the SoC. One trait for every core: the six
//! accessors and `fetch` are what an interpreter needs; the rest are hooks a block cache or a
//! JIT uses and a simple bus leaves at their defaults.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// no memory / device at this address
    Unmapped,
    /// address exists but access of this kind is not allowed (e.g. exec from data-only)
    Prohibited,
    /// address is not aligned for this access width
    Misaligned,
}

/// Number of entries in a bus's software TLB and the write-version page size, fixed here
/// because generated code indexes both directly.
pub const TLB_ENTRIES: usize = 512;
pub const VPAGE_SHIFT: u32 = 8;
#[inline(always)]
pub fn tlb_index(addr: u32) -> usize { (((addr >> 16) ^ (addr >> 24)) as usize) & (TLB_ENTRIES - 1) }

/// One software-TLB entry: guest `[lo, hi)` is host memory starting at `base`; `vbase` is the
/// write-version index of `lo`. Layout is fixed (32 bytes) because the JIT reads it. Copying or
/// sharing an entry never dereferences `base`; a JIT owner must separately keep its backing buffer
/// alive and unmoved, and serialize generated access to it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TlbEntry { pub lo: u32, pub hi: u32, pub base: *mut u8, pub vbase: u32, pub writable: u32, pub off: u32, pub src: u32 }
impl TlbEntry { pub const EMPTY: TlbEntry = TlbEntry { lo: 1, hi: 0, base: std::ptr::null_mut(), vbase: 0, writable: 0, off: 0, src: 0 }; }
// SAFETY: Sending this Copy value transfers only address bits. TlbEntry has no safe operation that
// dereferences `base`; generated access must separately uphold the documented owner invariants.
unsafe impl Send for TlbEntry {}
// SAFETY: Sharing this value exposes address bits but performs no dereference. Generated access
// through `base` must separately uphold the documented lifetime and synchronization invariants.
unsafe impl Sync for TlbEntry {}

/// What generated code needs to access memory without calling back: the TLB and the
/// write-version counters. Both pointers, and every backing buffer named by a TLB entry, must stay
/// valid and unmoved while generated code can access them.
#[derive(Clone, Copy)]
pub struct FastMem { pub tlb: *const TlbEntry, pub page_ver: *mut u32 }

pub trait Bus {
    fn read8(&mut self, addr: u32) -> Result<u8, Fault>;
    fn read16(&mut self, addr: u32) -> Result<u16, Fault>;
    fn read32(&mut self, addr: u32) -> Result<u32, Fault>;
    fn write8(&mut self, addr: u32, v: u8) -> Result<(), Fault>;
    fn write16(&mut self, addr: u32, v: u16) -> Result<(), Fault>;
    fn write32(&mut self, addr: u32, v: u32) -> Result<(), Fault>;
    /// Fetch up to 4 instruction bytes at `pc` (any alignment). Bytes past the
    /// end of a mapped region may be zero.
    fn fetch(&mut self, pc: u32) -> Result<[u8; 4], Fault>;
    /// Write-version counters, one per page of guest memory: `page_versions()[code_page(pc)]`
    /// changes whenever a write could have altered an instruction at `pc`. A decode-cache entry
    /// remembers its page index and the version it saw, so the common path is one indexed load
    /// instead of re-fetching and comparing the bytes; `code_page` is only called on a miss.
    /// Pages must be at least 128 bytes: a block (`block::MAX_LEN` instructions) records the
    /// versions of the pages holding its first and last byte and assumes there is no third.
    fn page_versions(&self) -> &[u32] { &[] }
    fn code_page(&mut self, pc: u32) -> u32 { let _ = pc; 0 }
    /// The pc of the instruction about to execute, for buses that attribute accesses to code.
    #[inline(always)]
    fn note_pc(&mut self, pc: u32) { let _ = pc; }
    /// True when the last instruction may have changed an interrupt line, so a block must end
    /// and let the machine re-derive the CPU's interrupt inputs before the next instruction.
    #[inline(always)]
    fn block_break(&self) -> bool { false }
    /// Direct memory access for generated code, if the bus has a `TlbEntry` table.
    fn fast_mem(&mut self) -> Option<FastMem> { None }
    /// Copy guest memory at `addr` into `out` in one piece when the whole range is plain
    /// memory with no device behind it. `false` changes nothing and means the caller must use
    /// the per-access reads, which also report where a fault is. A fast path for vector loads.
    fn read_bulk(&mut self, addr: u32, out: &mut [u8]) -> bool { let _ = (addr, out); false }
    /// Called after every executed instruction with the cycle estimate; lets the
    /// SoC advance timers and DMA. Return pending external level-interrupt lines.
    fn tick(&mut self, cycles: u32) -> u32 { let _ = cycles; 0 }
}

/// Simple flat RAM for unit tests.
pub struct FlatRam {
    pub base: u32,
    pub mem: Vec<u8>,
    /// bumped on every write: coarse, but this RAM only backs unit tests
    pub ver: u32,
}

impl FlatRam {
    pub fn new(base: u32, size: usize) -> Self { FlatRam { base, mem: vec![0; size], ver: 0 } }
    fn off(&self, addr: u32, n: usize) -> Result<usize, Fault> {
        let o = addr.wrapping_sub(self.base) as usize;
        if o + n <= self.mem.len() { Ok(o) } else { Err(Fault::Unmapped) }
    }
}

impl Bus for FlatRam {
    fn read8(&mut self, a: u32) -> Result<u8, Fault> { let o = self.off(a, 1)?; Ok(self.mem[o]) }
    fn read16(&mut self, a: u32) -> Result<u16, Fault> { let o = self.off(a, 2)?; Ok(u16::from_le_bytes([self.mem[o], self.mem[o + 1]])) }
    fn read32(&mut self, a: u32) -> Result<u32, Fault> { let o = self.off(a, 4)?; Ok(u32::from_le_bytes(self.mem[o..o + 4].try_into().unwrap())) }
    fn read_bulk(&mut self, a: u32, out: &mut [u8]) -> bool {
        match self.off(a, out.len()) { Ok(o) => { out.copy_from_slice(&self.mem[o..o + out.len()]); true } Err(_) => false }
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), Fault> { let o = self.off(a, 1)?; self.mem[o] = v; self.ver += 1; Ok(()) }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), Fault> { let o = self.off(a, 2)?; self.mem[o..o + 2].copy_from_slice(&v.to_le_bytes()); self.ver += 1; Ok(()) }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), Fault> { let o = self.off(a, 4)?; self.mem[o..o + 4].copy_from_slice(&v.to_le_bytes()); self.ver += 1; Ok(()) }
    fn page_versions(&self) -> &[u32] { std::slice::from_ref(&self.ver) }
    fn fetch(&mut self, pc: u32) -> Result<[u8; 4], Fault> {
        let o = self.off(pc, 1)?;
        let mut b = [0u8; 4];
        for (i, byte) in b.iter_mut().enumerate() { if o + i < self.mem.len() { *byte = self.mem[o + i]; } }
        Ok(b)
    }
}
