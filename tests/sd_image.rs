//! FAT32 SD card images for the tests, built by hand so that they are the same bytes on every
//! host: an MBR with one FAT32 (LBA) partition from LBA 2048, the way card formatters lay a card
//! out, an empty root directory, and a reader for the files the firmware leaves in it (8.3
//! names, as IDF's FatFs writes them without long file names). Field offsets: Microsoft's FAT32
//! File System Specification 1.03 (fatgen103), "Boot Sector and BPB", "FAT32 FSInfo Sector
//! Structure", "FAT Directory Structure".
#![allow(dead_code)]
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const SECTOR: u64 = 512;
pub const PARTITION_LBA: u64 = 2048;
const RESERVED: u32 = 32;

fn put16(b: &mut [u8], at: usize, v: u16) { b[at..at + 2].copy_from_slice(&v.to_le_bytes()); }
fn put32(b: &mut [u8], at: usize, v: u32) { b[at..at + 4].copy_from_slice(&v.to_le_bytes()); }
fn get16(b: &[u8], at: usize) -> u16 { u16::from_le_bytes(b[at..at + 2].try_into().unwrap()) }
fn get32(b: &[u8], at: usize) -> u32 { u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) }

/// Write a `size`-byte image (sparse where it is zero) with a FAT32 volume of `cluster`-byte
/// clusters. FAT32 needs at least 65525 clusters, so `size` must allow that.
pub fn fat32(path: &Path, size: u64, cluster: u32, label: &str) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.set_len(size)?;
    let total = (size / SECTOR - PARTITION_LBA) as u32;
    let spc = cluster / SECTOR as u32;
    // FAT size: fatgen103, "FAT Type Determination" (TmpVal1 / TmpVal2, rounded up).
    let fat = (total - RESERVED).div_ceil((256 * spc + 2) / 2);
    let clusters = (total - RESERVED - 2 * fat) / spc;
    assert!(clusters >= 65525, "{clusters} clusters is not FAT32");
    let mut mbr = [0u8; 512];
    put32(&mut mbr, 440, 0x5d1c_0b17);                          // disk signature
    mbr[446..462].copy_from_slice(&[0, 0xfe, 0xff, 0xff, 0x0c, 0xfe, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0]);
    put32(&mut mbr, 454, PARTITION_LBA as u32);
    put32(&mut mbr, 458, total);
    mbr[510] = 0x55; mbr[511] = 0xaa;
    let mut vbr = [0u8; 512];
    vbr[..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
    vbr[3..11].copy_from_slice(b"ESP32SIM");
    put16(&mut vbr, 11, 512);
    vbr[13] = spc as u8;
    put16(&mut vbr, 14, RESERVED as u16);
    vbr[16] = 2;                                                  // FATs
    vbr[21] = 0xf8;                                               // fixed media
    put16(&mut vbr, 24, 63); put16(&mut vbr, 26, 255);
    put32(&mut vbr, 28, PARTITION_LBA as u32);                   // hidden sectors
    put32(&mut vbr, 32, total);
    put32(&mut vbr, 36, fat);
    put32(&mut vbr, 44, 2);                                       // root directory cluster
    put16(&mut vbr, 48, 1); put16(&mut vbr, 50, 6);               // FSInfo, backup boot sector
    vbr[64] = 0x80; vbr[66] = 0x29;
    put32(&mut vbr, 67, 0x5d1c_0b17);                             // volume ID
    let mut l = [b' '; 11];
    l[..label.len().min(11)].copy_from_slice(&label.as_bytes()[..label.len().min(11)]);
    vbr[71..82].copy_from_slice(&l);
    vbr[82..90].copy_from_slice(b"FAT32   ");
    vbr[510] = 0x55; vbr[511] = 0xaa;
    let mut info = [0u8; 512];
    put32(&mut info, 0, 0x4161_5252); put32(&mut info, 484, 0x6141_7272);
    put32(&mut info, 488, clusters - 1);                          // free: all but the root's
    put32(&mut info, 492, 3);                                     // next free
    put32(&mut info, 508, 0xaa55_0000);
    let mut fat0 = [0u8; 512];
    put32(&mut fat0, 0, 0x0fff_fff8); put32(&mut fat0, 4, 0x0fff_ffff); put32(&mut fat0, 8, 0x0fff_ffff);
    let at = |s: u64| (PARTITION_LBA + s) * SECTOR;
    let mut put = |off: u64, b: &[u8]| -> std::io::Result<()> { f.seek(SeekFrom::Start(off))?; f.write_all(b) };
    put(0, &mbr)?;
    for base in [0, 6] { put(at(base), &vbr)?; put(at(base + 1), &info)?; }
    for n in 0..2 { put(at(RESERVED as u64 + n * fat as u64), &fat0)?; }
    if !label.is_empty() {
        let mut entry = [0u8; 32];
        entry[..11].copy_from_slice(&l);
        entry[11] = 0x08;                                         // volume label
        put(at((RESERVED + 2 * fat) as u64), &entry)?;
    }
    Ok(())
}

/// The volume of an image `fat32` wrote, as far as reading files from its root needs.
pub struct Volume { file: std::fs::File, spc: u64, fat: u64, data: u64 }
impl Volume {
    pub fn open(path: &Path) -> std::io::Result<Volume> {
        let mut file = std::fs::File::open(path)?;
        let mut mbr = [0u8; 512];
        file.read_exact(&mut mbr)?;
        let part = get32(&mbr, 454) as u64;
        let mut vbr = [0u8; 512];
        file.seek(SeekFrom::Start(part * SECTOR))?;
        file.read_exact(&mut vbr)?;
        let (spc, reserved, fats, fat_size) = (vbr[13] as u64, get16(&vbr, 14) as u64, vbr[16] as u64, get32(&vbr, 36) as u64);
        Ok(Volume { file, spc, fat: part + reserved, data: part + reserved + fats * fat_size })
    }
    fn cluster(&mut self, n: u32) -> Vec<u8> {
        let mut b = vec![0u8; (self.spc * SECTOR) as usize];
        self.file.seek(SeekFrom::Start((self.data + (n as u64 - 2) * self.spc) * SECTOR)).unwrap();
        self.file.read_exact(&mut b).unwrap();
        b
    }
    fn next(&mut self, n: u32) -> u32 {
        let mut e = [0u8; 4];
        self.file.seek(SeekFrom::Start(self.fat * SECTOR + 4 * n as u64)).unwrap();
        self.file.read_exact(&mut e).unwrap();
        u32::from_le_bytes(e) & 0x0fff_ffff
    }
    fn chain(&mut self, mut n: u32) -> Vec<u8> {
        let mut out = Vec::new();
        while (2..0x0fff_fff8).contains(&n) { out.extend(self.cluster(n)); n = self.next(n); }
        out
    }
    /// The files in the root directory: (8.3 name as "NAME.EXT", contents).
    pub fn root_files(&mut self) -> Vec<(String, Vec<u8>)> {
        let dir = self.chain(2);
        let mut files = Vec::new();
        for e in dir.chunks(32) {
            if e[0] == 0 { break; }
            if e[0] == 0xe5 || e[11] & 0x08 != 0 || e[11] == 0x0f { continue; }
            let name = String::from_utf8_lossy(&e[..8]).trim_end().to_string();
            let ext = String::from_utf8_lossy(&e[8..11]).trim_end().to_string();
            let first = (get16(e, 20) as u32) << 16 | get16(e, 26) as u32;
            let mut data = self.chain(first);
            data.truncate(get32(e, 28) as usize);
            files.push((if ext.is_empty() { name } else { format!("{name}.{ext}") }, data));
        }
        files
    }
}
