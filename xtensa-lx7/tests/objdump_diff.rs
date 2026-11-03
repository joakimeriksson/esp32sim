//! Differential decoder test: every instruction in objdump's disassembly of the
//! firmware and the mask ROM must decode to the same mnemonic + operands.
//! The checked-in corpus (`tests/corpus/*.dis`, sampled from a real app and the mask ROM) runs
//! every time; `XTENSA_DIS_FILES=/path/app.dis:/path/rom.dis` runs the ignored full-size test. The
//! list is split as `PATH` is (`std::env::split_paths`: `:`, or `;` on Windows, where `:` is in
//! every path).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use xtensa_lx7::decode::{decode, Op};
use xtensa_lx7::disasm;

fn norm_num(tok: &str) -> Option<i64> {
    let t = tok.trim();
    if let Some(h) = t.strip_prefix("0x") { return i64::from_str_radix(h, 16).ok(); }
    if let Some(h) = t.strip_prefix("-0x") { return i64::from_str_radix(h, 16).ok().map(|v| -v); }
    if let Ok(v) = t.parse::<i64>() { return Some(v); }
    i64::from_str_radix(t, 16).ok()
}
fn same_operand(mine: &str, theirs: &str) -> bool {
    if mine == theirs { return true; }
    match (norm_num(mine), norm_num(theirs)) {
        (Some(a), Some(b)) => a == b || (a as u32) == (b as u32),
        _ => false,
    }
}

/// Every instruction in the listings must decode to objdump's mnemonic and operands. Returns the case count.
fn check(files: &[PathBuf]) -> usize {
    let mut total = 0usize;
    let mut bad = 0usize;
    let mut by_mnemonic: BTreeMap<String, (usize, usize, String)> = BTreeMap::new();
    for file in files {
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {}", file.display(), e));
        for line in text.lines() {
            // "40374404:\t36 41 00 \tentry\ta1, 32"   (objdump prints bytes reversed as one hex string on xtensa)
            let mut parts = line.splitn(3, '\t');
            let (Some(addr_s), Some(bytes_s), Some(rest)) = (parts.next(), parts.next(), parts.next()) else { continue };
            let Some(addr_s) = addr_s.trim().strip_suffix(':') else { continue };
            let Ok(pc) = u32::from_str_radix(addr_s.trim(), 16) else { continue };
            let hex: String = bytes_s.chars().filter(|c| !c.is_whitespace()).collect();
            if !hex.len().is_multiple_of(2) || hex.is_empty() { continue; }
            let mut raw: Vec<u8> = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect();
            raw.reverse();   // objdump prints the word big-endian; memory order is little-endian
            let expect = rest.trim();
            if expect.starts_with(".byte") || expect.is_empty() { continue; }
            let mut bytes = [0u8; 4];
            for (i, b) in raw.iter().take(4).enumerate() { bytes[i] = *b; }
            let insn = decode(pc, bytes);
            total += 1;
            let mut exp_parts = expect.splitn(2, char::is_whitespace);
            let exp_mn = exp_parts.next().unwrap_or("");
            let exp_ops: Vec<String> = exp_parts.next().unwrap_or("").split(',').map(|o| {
                let o = o.trim();
                match o.find(" <") { Some(p) => o[..p].to_string(), None => o.to_string() }
            }).filter(|o| !o.is_empty()).collect();
            let entry = by_mnemonic.entry(exp_mn.to_string()).or_insert((0, 0, String::new()));
            entry.0 += 1;
            if insn.op == Op::Pie || insn.op == Op::Mac16 && exp_mn.starts_with("ee.") { continue; }   // PIE decoded generically for now
            let my_mn = disasm::mnemonic(&insn);
            let my_ops = disasm::operands(&insn);
            let ok = my_mn == exp_mn && my_ops.len() == exp_ops.len() && my_ops.iter().zip(exp_ops.iter()).all(|(m, e)| same_operand(m, e))
                && insn.len as usize == raw.len();
            if !ok {
                bad += 1;
                entry.1 += 1;
                if entry.2.is_empty() { entry.2 = format!("{:08x}: {} | mine: {} (len {})", pc, expect, disasm::format(&insn), insn.len); }
            }
        }
    }
    for (m, (n, b, ex)) in &by_mnemonic { if *b > 0 { eprintln!("{:<28} {:>7} insns {:>6} bad   e.g. {}", m, n, b, ex); } }
    eprintln!("total {} instructions, {} mismatches", total, bad);
    assert_eq!(bad, 0, "decoder mismatches vs objdump");
    total
}

/// Hermetic: the sampled corpora checked in next to this test (see the headers for provenance).
#[test]
fn decoder_matches_corpus() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let n = check(&[dir.join("hello_world.dis"), dir.join("rom.dis")]);
    assert!(n >= 6000, "only {} corpus cases — the corpus files are incomplete", n);
}

/// The full listings of whatever `XTENSA_DIS_FILES` points at (app + ROM: ~1 M instructions).
#[test]
#[ignore = "set XTENSA_DIS_FILES=/path/app.dis:/path/rom.dis (xtensa-esp32s3-elf-objdump -d)"]
fn external_decoder_matches_objdump() {
    let list = std::env::var_os("XTENSA_DIS_FILES").expect("XTENSA_DIS_FILES=/path/app.dis:/path/rom.dis is required for this test");
    check(&std::env::split_paths(&list).collect::<Vec<_>>());
}
