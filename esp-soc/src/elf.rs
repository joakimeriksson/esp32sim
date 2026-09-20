//! Minimal ELF32 (little-endian) loader: PT_LOAD segments + symbol table.
use std::collections::BTreeMap;

pub struct Segment {
    pub vaddr: u32,
    pub paddr: u32,
    pub data: Vec<u8>,
    pub memsz: u32,
    pub flags: u32,
}

pub struct Section { pub name: String, pub addr: u32, pub data: Vec<u8>, pub is_bss: bool }

pub struct Elf {
    pub entry: u32,
    pub segments: Vec<Segment>,
    /// allocatable sections (PROGBITS with data, NOBITS as bss) — the ROM ELF keeps
    /// its RAM initialisers here without matching program headers
    pub sections: Vec<Section>,
    /// address -> symbol name (functions and objects)
    pub symbols: BTreeMap<u32, String>,
    /// name -> address (all symbols incl. NOTYPE linker symbols)
    pub by_name: std::collections::HashMap<String, u32>,
}

fn u16le(d: &[u8], o: usize) -> u16 { u16::from_le_bytes([d[o], d[o + 1]]) }
fn u32le(d: &[u8], o: usize) -> u32 { u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) }

fn bytes(d: &[u8], off: usize, len: usize) -> Result<&[u8], String> {
    if len == 0 { return Ok(&[]); } // Empty payloads have no file extent to validate.
    d.get(off..).and_then(|tail| tail.get(..len)).ok_or_else(|| "truncated ELF data".into())
}

/// Validate the whole table before reading fixed fields from any of its records.
fn table(d: &[u8], off: usize, entsize: usize, count: usize, minimum: usize) -> Result<&[u8], String> {
    if count == 0 { return Ok(&[]); }
    if entsize < minimum { return Err("ELF entry size is too small".into()); }
    let len = count.checked_mul(entsize).ok_or("ELF table size overflows")?;
    bytes(d, off, len)
}

fn section_data<'a>(d: &'a [u8], header: &[u8]) -> Result<&'a [u8], String> {
    bytes(d, u32le(header, 16) as usize, u32le(header, 20) as usize)
}

fn name(strings: &[u8], off: usize) -> Result<String, String> {
    if off == 0 { return Ok(String::new()); } // ELF index zero means no name, even for an empty table.
    let tail = strings.get(off..).ok_or("ELF string offset is out of bounds")?;
    let end = tail.iter().position(|&c| c == 0).ok_or("unterminated ELF string")?;
    Ok(String::from_utf8_lossy(&tail[..end]).into_owned())
}

pub fn parse(d: &[u8]) -> Result<Elf, String> {
    // Segments and sections may overlap in the file. Bound their combined owned payloads,
    // rather than allowing each header to multiply the input's memory footprint.
    parse_with_copy_limit(d, 256 * 1024 * 1024)
}

fn parse_with_copy_limit(d: &[u8], mut remaining: usize) -> Result<Elf, String> {
    if d.len() < 52 || &d[0..4] != b"\x7fELF" { return Err("not an ELF file".into()); }
    if d[4] != 1 || d[5] != 1 { return Err("need ELF32 little-endian".into()); }
    let entry = u32le(d, 24);
    let phoff = u32le(d, 28) as usize;
    let shoff = u32le(d, 32) as usize;
    let phentsize = u16le(d, 42) as usize;
    let phnum = u16le(d, 44) as usize;
    let shentsize = u16le(d, 46) as usize;
    let shnum = u16le(d, 48) as usize;
    let programs = table(d, phoff, phentsize, phnum, 32)?;
    let sections = table(d, shoff, shentsize, shnum, 40)?;
    let mut segments = Vec::new();
    for p in programs.chunks_exact(phentsize.max(1)) {
        let ptype = u32le(p, 0);
        if ptype != 1 { continue; }
        let offset = u32le(p, 4) as usize;
        let vaddr = u32le(p, 8);
        let paddr = u32le(p, 12);
        let filesz = u32le(p, 16) as usize;
        let memsz = u32le(p, 20);
        let flags = u32le(p, 24);
        if filesz > memsz as usize { return Err("ELF segment file size exceeds memory size".into()); }
        let payload = bytes(d, offset, filesz)?;
        remaining = remaining.checked_sub(payload.len()).ok_or("ELF copied payload exceeds 256 MiB limit")?;
        let data = payload.to_vec();
        if memsz == 0 { continue; }
        segments.push(Segment { vaddr, paddr, data, memsz, flags });
    }
    // symbols
    let mut symbols = BTreeMap::new();
    let mut by_name = std::collections::HashMap::new();
    let mut alloc_sections = Vec::new();
    let shstrndx = u16le(d, 50) as usize;
    let section_headers = sections.chunks_exact(shentsize.max(1));
    let section_names = if shstrndx == 0 { &[][..] } else {
        let s = section_headers.clone().nth(shstrndx).ok_or("ELF section-name table index is out of bounds")?;
        section_data(d, s)?
    };
    for s in section_headers.clone() {
        let (stype, flags, addr, size) = (u32le(s, 4), u32le(s, 8), u32le(s, 12), u32le(s, 20));
        if size > 0 && addr != 0 && (stype == 1 || (stype == 8 && flags & 2 != 0)) {   // ROM ELFs mark RAM initialisers W-only (no SHF_ALLOC)
            let name = if shstrndx == 0 { String::new() } else {
                let Ok(name) = name(section_names, u32le(s, 0) as usize) else { continue };
                name
            };
            let data = if stype == 1 {
                let payload = section_data(d, s)?;
                remaining = remaining.checked_sub(payload.len()).ok_or("ELF copied payload exceeds 256 MiB limit")?;
                payload.to_vec()
            } else { Vec::new() };
            alloc_sections.push(Section { name, addr, data, is_bss: stype == 8 });
        }
    }
    for s in section_headers.clone() {
        if u32le(s, 4) != 2 { continue; }   // SHT_SYMTAB
        let entsize = u32le(s, 36) as usize;
        if entsize < 16 { return Err("ELF symbol entry size is too small".into()); }
        let records = section_data(d, s)?;
        if records.len() % entsize != 0 { return Err("truncated ELF symbol table".into()); }
        let link = u32le(s, 24) as usize;
        let strings_header = section_headers.clone().nth(link).ok_or("ELF symbol string table index is out of bounds")?;
        let strings = section_data(d, strings_header)?;
        for e in records.chunks_exact(entsize) {
            let name_off = u32le(e, 0) as usize;
            let value = u32le(e, 4);
            let info = e[12];
            let typ = info & 0xf;
            let Ok(name) = name(strings, name_off) else { continue };
            if name.is_empty() { continue; }
            by_name.entry(name.clone()).or_insert(value);
            if typ == 1 || typ == 2 { symbols.entry(value).or_insert(name); }   // OBJECT / FUNC
        }
    }
    Ok(Elf { entry, segments, sections: alloc_sections, symbols, by_name })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_limit_counts_segment_and_section_payloads_together() {
        let mut d = vec![0; 128];
        d[..6].copy_from_slice(b"\x7fELF\x01\x01");
        d[42] = 32; d[44] = 1; d[46] = 40; d[48] = 1;
        for (off, value) in [(28, 52u32), (32, 84), (52, 1), (56, 124), (68, 4),
                             (72, 4), (88, 1), (96, 0x4000), (100, 124), (104, 4)] {
            d[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }
        let e = parse_with_copy_limit(&d, 8).unwrap();
        assert_eq!(e.segments[0].data.len(), 4);
        assert_eq!(e.sections[0].data.len(), 4);
        assert!(parse_with_copy_limit(&d, 7).is_err());
    }
}
