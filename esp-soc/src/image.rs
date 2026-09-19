//! ESP-IDF application image format (what esptool writes to flash).
pub struct ImageSegment {
    pub load_addr: u32,
    /// offset of the segment data from the start of the image
    pub file_off: u32,
    pub len: u32,
}

pub struct AppImage {
    pub entry: u32,
    pub segments: Vec<ImageSegment>,
}

pub fn parse(d: &[u8]) -> Result<AppImage, String> {
    if d.len() < 24 || d[0] != 0xE9 { return Err("not an ESP image (magic 0xE9 missing)".into()); }
    let nseg = d[1] as usize;
    let entry = u32::from_le_bytes(d[4..8].try_into().unwrap());
    let mut off = 24usize;
    let mut segments = Vec::new();
    for _ in 0..nseg {
        let header = d.get(off..).and_then(|tail| tail.get(..8)).ok_or("truncated image segment header")?;
        let load_addr = u32::from_le_bytes(header[..4].try_into().unwrap());
        let len = u32::from_le_bytes(header[4..].try_into().unwrap());
        off += 8; // The complete header fits in d.
        let end = off.checked_add(len as usize).filter(|&end| end <= d.len()).ok_or("truncated image segment data")?;
        let file_off = u32::try_from(off).map_err(|_| "image segment offset exceeds 32 bits")?;
        segments.push(ImageSegment { load_addr, file_off, len });
        off = end;
    }
    Ok(AppImage { entry, segments })
}
