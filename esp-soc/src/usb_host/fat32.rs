//! FAT32 on the host side of a block device, for the files a host writes to a mass-storage
//! device: mount (an MBR's first FAT32 partition, or a volume from LBA 0), list, read, and
//! write 8.3 files in the root directory in a vfat-like pattern: the directory entry first, the
//! data in chunks as it comes, the FAT sectors covering each chunk's clusters (both copies), and
//! at the end the entry's size and first cluster and FSInfo. It is not Linux's writeback order
//! (page-cache batches, buffered metadata, periodic inode writeback, FSInfo at sync or unmount):
//! a file cut off mid-write here keeps size 0 and no cluster, its data orphaned. Field offsets:
//! Microsoft FAT32 File System Specification 1.03 (fatgen103).

/// Blocks on the other end, by number.
pub trait BlockDevice {
    fn block_size(&self) -> u32;
    fn blocks(&self) -> u64;
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), String>;
    fn write_blocks(&mut self, lba: u64, data: &[u8]) -> Result<(), String>;
    /// Let time pass with nothing to do (until the next chunk is due).
    fn pause_ns(&mut self, _ns: u64) -> Result<(), String> { Ok(()) }
    /// Emulated time, for pacing; a device without a clock stays at 0.
    fn now_ns(&mut self) -> u64 { 0 }
}

/// A disk in memory.
pub struct MemDisk(pub Vec<u8>);
impl BlockDevice for MemDisk {
    fn block_size(&self) -> u32 { 512 }
    fn blocks(&self) -> u64 { self.0.len() as u64 / 512 }
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), String> {
        let o = lba as usize * 512;
        buf.copy_from_slice(self.0.get(o..o + buf.len()).ok_or("read past the end")?);
        Ok(())
    }
    fn write_blocks(&mut self, lba: u64, data: &[u8]) -> Result<(), String> {
        let o = lba as usize * 512;
        self.0.get_mut(o..o + data.len()).ok_or("write past the end")?.copy_from_slice(data);
        Ok(())
    }
}

/// What a write paced to a fixed schedule saw.
#[derive(Clone, Debug, Default)]
pub struct PacedStats {
    pub bytes: u64,
    pub chunks: usize,
    pub elapsed_ns: u64,
    /// how far the latest chunk started behind its due time, and which chunk that was
    pub worst_late_ns: u64,
    pub worst_chunk: usize,
    /// chunks that started a whole gap or more behind: the host needed more than one chunk of buffer
    pub behind: usize,
    /// bytes over time from the start of the second half's first chunk to the close
    pub second_half_mb_s: f64,
}

impl PacedStats {
    pub fn mb_s(&self) -> f64 { self.bytes as f64 * 1e3 / self.elapsed_ns.max(1) as f64 }
    /// Never a whole chunk behind the schedule.
    pub fn kept_up(&self, gap_ns: u64) -> bool { gap_ns == 0 || self.behind == 0 }
    pub fn summary(&self, gap_ns: u64) -> String {
        let pace = if gap_ns == 0 { "unpaced".to_string() } else {
            format!("paced at {:.2} MB/s, worst {:.1} ms late (chunk {}), {}", self.bytes as f64 * 1e3 / (self.chunks as u64 * gap_ns).max(1) as f64,
                    self.worst_late_ns as f64 / 1e6, self.worst_chunk, if self.kept_up(gap_ns) { "kept up".to_string() } else { format!("fell behind on {} chunks", self.behind) })
        };
        format!("{} bytes in {} chunks, {:.3} s, {:.2} MB/s, second half {:.2} MB/s; {pace}", self.bytes, self.chunks, self.elapsed_ns as f64 / 1e9, self.mb_s(), self.second_half_mb_s)
    }
}

/// Write `data` at a fixed rate: chunk i is ready at t0 + i·gap, t0 taken before the file is
/// created. The host writes a chunk as soon as it is ready and the previous one is done, and
/// waits only for one not ready yet; a chunk that starts after its due time counts as late.
pub fn write_paced(fs: &mut Fat32, dev: &mut dyn BlockDevice, name: &str, data: &[u8], chunk: usize, gap_ns: u64, fat_every: u32) -> Result<PacedStats, String> {
    let chunk = chunk.max(fs.cluster_bytes()) / fs.cluster_bytes() * fs.cluster_bytes();
    let t0 = dev.now_ns();
    let mut st = PacedStats { chunks: data.len().div_ceil(chunk), ..Default::default() };
    let (mut half_t, mut half_bytes) = (t0, 0u64);
    let mut f = fs.create(dev, name)?;
    f.fat_every = fat_every;
    for (i, part) in data.chunks(chunk).enumerate() {
        let due = t0 + i as u64 * gap_ns;
        let now = dev.now_ns();
        if due > now { dev.pause_ns(due - now)?; }
        let late = now.saturating_sub(due);
        if late > st.worst_late_ns { st.worst_late_ns = late; st.worst_chunk = i; }
        if gap_ns > 0 && late >= gap_ns { st.behind += 1; }
        if i == st.chunks / 2 { half_t = dev.now_ns(); half_bytes = (data.len() - i * chunk) as u64; }
        f.write(fs, dev, part)?;
    }
    st.bytes = f.close(fs, dev)?;
    let end = dev.now_ns();
    st.elapsed_ns = (end - t0).max(1);
    st.second_half_mb_s = half_bytes as f64 * 1e3 / (end - half_t).max(1) as f64;
    Ok(st)
}

const EOC: u32 = 0x0fff_ffff;
const FREE: u32 = 0;

fn get16(b: &[u8], at: usize) -> u16 { u16::from_le_bytes([b[at], b[at + 1]]) }
fn get32(b: &[u8], at: usize) -> u32 { u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) }
fn put32(b: &mut [u8], at: usize, v: u32) { b[at..at + 4].copy_from_slice(&v.to_le_bytes()); }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry { pub name: String, pub size: u32, pub cluster: u32, pub attr: u8 }

#[derive(Clone, Debug)]
pub struct Fat32 {
    pub volume_lba: u64,
    pub sectors_per_cluster: u32,
    pub reserved: u32,
    pub fats: u32,
    pub fat_sectors: u32,
    pub root_cluster: u32,
    pub fsinfo: u32,
    pub clusters: u32,
    pub data_lba: u64,
    pub label: String,
    /// clusters allocated since FSInfo was last written, and where allocation looks first: the
    /// last cluster allocated, or FSInfo's hint (fatgen103 calls it the next free cluster, Linux
    /// and FatFs keep the last allocated one there; a search that starts at it covers both)
    allocated: u32,
    hint: u32,
}

/// `name.ext` as the 11 bytes of a short directory entry, upper case.
pub fn short_name(name: &str) -> Result<[u8; 11], String> {
    let (base, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let ok = |s: &str, n: usize| !s.is_empty() && s.len() <= n && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-~!#$%&'()@^{}".contains(&c));
    if !ok(base, 8) || !(ext.is_empty() || ok(ext, 3)) { return Err(format!("{name}: not an 8.3 name")); }
    let mut out = [b' '; 11];
    for (i, c) in base.bytes().enumerate() { out[i] = c.to_ascii_uppercase(); }
    for (i, c) in ext.bytes().enumerate() { out[8 + i] = c.to_ascii_uppercase(); }
    Ok(out)
}

fn display_name(raw: &[u8]) -> String {
    let base = String::from_utf8_lossy(&raw[..8]).trim_end().to_string();
    let ext = String::from_utf8_lossy(&raw[8..11]).trim_end().to_string();
    if ext.is_empty() { base } else { format!("{base}.{ext}") }
}

impl Fat32 {
    pub fn mount(dev: &mut dyn BlockDevice) -> Result<Self, String> {
        if dev.block_size() != 512 { return Err(format!("{}-byte blocks: only 512 is supported", dev.block_size())); }
        let mut s = [0u8; 512];
        dev.read_blocks(0, &mut s)?;
        // An MBR's partition entries at 446; a FAT32 volume boot record starts with a jump.
        let mut volume = 0u64;
        if s[510..512] == [0x55, 0xaa] && !(s[0] == 0xeb || s[0] == 0xe9) {
            let p = (0..4).map(|i| &s[446 + 16 * i..462 + 16 * i]).find(|e| matches!(e[4], 0x0b | 0x0c)).ok_or("no FAT32 partition in the MBR")?;
            volume = get32(p, 8) as u64;
            dev.read_blocks(volume, &mut s)?;
        }
        if get16(&s, 11) != 512 || s[510..512] != [0x55, 0xaa] { return Err("no FAT32 boot sector".into()); }
        let (spc, reserved, fats) = (s[13] as u32, get16(&s, 14) as u32, s[16] as u32);
        let fat_sectors = get32(&s, 36);
        if get16(&s, 22) != 0 || fat_sectors == 0 || spc == 0 { return Err("not FAT32 (BPB_FATSz16 set)".into()); }
        let total = if get16(&s, 19) != 0 { get16(&s, 19) as u32 } else { get32(&s, 32) };
        let meta = reserved as u64 + fats as u64 * fat_sectors as u64;
        let data_lba = volume + meta;
        let clusters = (total as u64).checked_sub(meta).ok_or("not FAT32 (the FATs end past the volume)")? / spc as u64;
        // FAT32 entries hold 28 bits, so a volume has 0x0FFFFFF5 clusters at most.
        if clusters > 0x0fff_fff5 { return Err("not FAT32 (more clusters than its entries number)".into()); }
        let clusters = clusters as u32;
        let mut fs = Fat32 { volume_lba: volume, sectors_per_cluster: spc, reserved, fats, fat_sectors, root_cluster: get32(&s, 44), fsinfo: get16(&s, 48) as u32,
                             clusters, data_lba, label: String::from_utf8_lossy(&s[71..82]).trim_end().to_string(), allocated: 0, hint: 2 };
        let mut info = [0u8; 512];
        dev.read_blocks(volume + fs.fsinfo as u64, &mut info)?;
        if get32(&info, 0) == 0x4161_5252 && get32(&info, 484) == 0x6141_7272 {
            let hint = get32(&info, 492);
            if (2..clusters + 2).contains(&hint) { fs.hint = hint; }
        }
        Ok(fs)
    }

    pub fn cluster_bytes(&self) -> usize { self.sectors_per_cluster as usize * 512 }
    fn cluster_lba(&self, c: u32) -> u64 { self.data_lba + (c as u64 - 2) * self.sectors_per_cluster as u64 }
    fn fat_lba(&self, copy: u32, c: u32) -> u64 { self.volume_lba + self.reserved as u64 + copy as u64 * self.fat_sectors as u64 + (c / 128) as u64 }

    /// The cluster chain from `first`, each FAT sector read once while the chain stays in it.
    pub fn chain(&self, dev: &mut dyn BlockDevice, first: u32) -> Result<Vec<u32>, String> {
        let mut out = Vec::new();
        let (mut s, mut loaded) = ([0u8; 512], u32::MAX);
        let mut c = first;
        while (2..0x0fff_fff8).contains(&c) {
            if out.len() > self.clusters as usize { return Err("a cluster chain loops".into()); }
            out.push(c);
            if c / 128 != loaded { dev.read_blocks(self.fat_lba(0, c), &mut s)?; loaded = c / 128; }
            c = get32(&s, (c % 128) as usize * 4) & 0x0fff_ffff;
        }
        Ok(out)
    }

    /// Set FAT entries, both copies, one read-modify-write per FAT sector.
    fn write_fat(&self, dev: &mut dyn BlockDevice, entries: &[(u32, u32)]) -> Result<(), String> {
        let mut sectors: Vec<u32> = entries.iter().map(|&(c, _)| c / 128).collect();
        sectors.sort_unstable();
        sectors.dedup();
        for sec in sectors {
            let mut s = [0u8; 512];
            dev.read_blocks(self.fat_lba(0, sec * 128), &mut s)?;
            for &(c, v) in entries.iter().filter(|(c, _)| c / 128 == sec) {
                let at = (c % 128) as usize * 4;
                let old = get32(&s, at);
                put32(&mut s, at, (old & 0xf000_0000) | (v & 0x0fff_ffff));
            }
            for copy in 0..self.fats { dev.write_blocks(self.fat_lba(copy, sec * 128), &s)?; }
        }
        Ok(())
    }

    /// `n` free clusters from the hint on, read a FAT sector at a time.
    fn allocate(&mut self, dev: &mut dyn BlockDevice, n: usize) -> Result<Vec<u32>, String> {
        let mut out = Vec::with_capacity(n);
        let mut c = self.hint;
        let mut seen = 0u32;
        let mut s = [0u8; 512];
        let mut loaded = u32::MAX;
        while out.len() < n {
            if seen >= self.clusters { return Err("the volume is full".into()); }
            if c >= self.clusters + 2 { c = 2; }
            if c / 128 != loaded { dev.read_blocks(self.fat_lba(0, c), &mut s)?; loaded = c / 128; }
            if get32(&s, (c % 128) as usize * 4) & 0x0fff_ffff == FREE { out.push(c); }
            c += 1;
            seen += 1;
        }
        if let Some(&l) = out.last() { self.hint = l; }
        self.allocated += n as u32;
        Ok(out)
    }

    fn root_sectors(&self, dev: &mut dyn BlockDevice) -> Result<Vec<u64>, String> {
        let spc = self.sectors_per_cluster as u64;
        Ok(self.chain(dev, self.root_cluster)?.into_iter().flat_map(|c| { let l = self.cluster_lba(c); (0..spc).map(move |i| l + i) }).collect())
    }

    pub fn list_root(&self, dev: &mut dyn BlockDevice) -> Result<Vec<DirEntry>, String> {
        let mut out = Vec::new();
        for lba in self.root_sectors(dev)? {
            let mut s = [0u8; 512];
            dev.read_blocks(lba, &mut s)?;
            for e in s.as_chunks::<32>().0 {
                if e[0] == 0 { return Ok(out); }
                if e[0] == 0xe5 || e[11] & 0x0f == 0x0f { continue; }
                out.push(DirEntry { name: display_name(&e[..11]), size: get32(e, 28), cluster: (get16(e, 20) as u32) << 16 | get16(e, 26) as u32, attr: e[11] });
            }
        }
        Ok(out)
    }

    pub fn read_file(&self, dev: &mut dyn BlockDevice, name: &str) -> Result<Vec<u8>, String> {
        let want = display_name(&short_name(name)?);
        let e = self.list_root(dev)?.into_iter().find(|e| e.name == want && e.attr & 0x18 == 0).ok_or_else(|| format!("{name}: no such file"))?;
        let chain = self.chain(dev, e.cluster)?;
        let mut out = Vec::with_capacity(e.size as usize);
        // contiguous clusters in one read
        let mut i = 0;
        while i < chain.len() && out.len() < e.size as usize {
            let mut j = i + 1;
            while j < chain.len() && chain[j] == chain[j - 1] + 1 && (j - i + 1) * self.cluster_bytes() <= 128 << 10 { j += 1; }
            let mut buf = vec![0u8; (j - i) * self.cluster_bytes()];
            dev.read_blocks(self.cluster_lba(chain[i]), &mut buf)?;
            out.extend_from_slice(&buf);
            i = j;
        }
        if out.len() < e.size as usize { return Err(format!("{name}: the chain holds {} of {} bytes", out.len(), e.size)); }
        out.truncate(e.size as usize);
        Ok(out)
    }

    /// Start a file: its directory entry (size 0, no cluster) in the first free slot of the root
    /// directory, written at once, as `open(O_CREAT)` leaves it.
    pub fn create(&mut self, dev: &mut dyn BlockDevice, name: &str) -> Result<FileWriter, String> {
        let raw = short_name(name)?;
        if self.list_root(dev)?.iter().any(|e| e.name == display_name(&raw)) { return Err(format!("{name}: exists")); }
        for lba in self.root_sectors(dev)? {
            let mut s = [0u8; 512];
            dev.read_blocks(lba, &mut s)?;
            if let Some(slot) = s.as_chunks::<32>().0.iter().position(|e| e[0] == 0 || e[0] == 0xe5) {
                let e = &mut s[slot * 32..slot * 32 + 32];
                e.fill(0);
                e[..11].copy_from_slice(&raw);
                e[11] = 0x20;                                    // ATTR_ARCHIVE
                // a fixed time stamp, 2024-01-01 00:00, so the image is the same on every run
                let date = ((2024 - 1980) << 9 | 1 << 5 | 1) as u16;
                for at in [16, 18, 24] { e[at..at + 2].copy_from_slice(&date.to_le_bytes()); }
                dev.write_blocks(lba, &s)?;
                return Ok(FileWriter { dir_lba: lba, dir_slot: slot, first: 0, last: 0, size: 0, pending: Vec::new(), fat_every: 1, chunks: 0 });
            }
        }
        Err("the root directory is full (it is not extended)".into())
    }

    /// FSInfo as it stands on the volume, less what this host allocated since it last wrote it
    /// (an unknown count stays unknown), and the last cluster allocated as the hint: the device
    /// may have changed the volume since the mount.
    pub fn write_fsinfo(&mut self, dev: &mut dyn BlockDevice) -> Result<(), String> {
        let lba = self.volume_lba + self.fsinfo as u64;
        let mut s = [0u8; 512];
        dev.read_blocks(lba, &mut s)?;
        let free = get32(&s, 488);
        if free != u32::MAX { put32(&mut s, 488, free.saturating_sub(self.allocated)); }
        put32(&mut s, 492, self.hint);
        self.allocated = 0;
        dev.write_blocks(lba, &s)
    }
}

/// A file being written.
pub struct FileWriter {
    dir_lba: u64,
    dir_slot: usize,
    first: u32,
    last: u32,
    size: u64,
    /// FAT entries not written yet
    pending: Vec<(u32, u32)>,
    /// write the FAT after this many chunks
    pub fat_every: u32,
    chunks: u32,
}

impl FileWriter {
    /// Append `data` (a whole number of clusters, except for the file's last chunk): clusters
    /// from the volume's free ones, the data in one write per contiguous run, then the FAT
    /// sectors for them every `fat_every` chunks.
    pub fn write(&mut self, fs: &mut Fat32, dev: &mut dyn BlockDevice, data: &[u8]) -> Result<(), String> {
        let cb = fs.cluster_bytes();
        if data.is_empty() { return Ok(()); }
        if !self.size.is_multiple_of(cb as u64) { return Err("a chunk after a partial cluster".into()); }
        let clusters = fs.allocate(dev, data.len().div_ceil(cb))?;
        for &c in &clusters {
            if self.first == 0 { self.first = c; } else { self.pending.push((self.last, c)); }
            self.last = c;
        }
        self.pending.push((self.last, EOC));
        let mut i = 0;
        while i < clusters.len() {
            let mut j = i + 1;
            while j < clusters.len() && clusters[j] == clusters[j - 1] + 1 { j += 1; }
            let (from, to) = (i * cb, (j * cb).min(data.len()));
            let mut buf = data[from..to].to_vec();
            buf.resize((j - i) * cb, 0);
            dev.write_blocks(fs.cluster_lba(clusters[i]), &buf)?;
            i = j;
        }
        self.size += data.len() as u64;
        self.chunks += 1;
        if self.chunks.is_multiple_of(self.fat_every.max(1)) { self.flush_fat(fs, dev)?; }
        Ok(())
    }

    fn flush_fat(&mut self, fs: &Fat32, dev: &mut dyn BlockDevice) -> Result<(), String> {
        // the last entry for a cluster wins: an end-of-chain mark is replaced by its successor
        let mut entries: Vec<(u32, u32)> = Vec::new();
        for &(c, v) in &self.pending { if let Some(e) = entries.iter_mut().find(|e| e.0 == c) { e.1 = v; } else { entries.push((c, v)); } }
        fs.write_fat(dev, &entries)?;
        self.pending.clear();
        Ok(())
    }

    /// The FAT left, the directory entry's first cluster and size, FSInfo.
    pub fn close(mut self, fs: &mut Fat32, dev: &mut dyn BlockDevice) -> Result<u64, String> {
        self.flush_fat(fs, dev)?;
        let mut s = [0u8; 512];
        dev.read_blocks(self.dir_lba, &mut s)?;
        let e = &mut s[self.dir_slot * 32..self.dir_slot * 32 + 32];
        e[20..22].copy_from_slice(&((self.first >> 16) as u16).to_le_bytes());
        e[26..28].copy_from_slice(&(self.first as u16).to_le_bytes());
        put32(e, 28, self.size as u32);
        dev.write_blocks(self.dir_lba, &s)?;
        fs.write_fsinfo(dev)?;
        Ok(self.size)
    }
}

/// The bytes of a test file: a xorshift stream seeded from the name, so a writer and a checker
/// make the same file without keeping it.
pub fn file_bytes(name: &str, len: usize) -> Vec<u8> {
    let mut x: u64 = name.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3)) | 1;
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        x ^= x << 13; x ^= x >> 7; x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(len);
    out
}
