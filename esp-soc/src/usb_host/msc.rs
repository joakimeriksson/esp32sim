//! Mass storage on the host side: SCSI commands over bulk-only transport (USB Mass Storage Class
//! Bulk-Only Transport 1.0, "BOT"), and the LUN as a block device.
//!
//! A command is a CBW out, the data stage, a CSW in (BOT §5). The host checks the CSW as §6.3
//! says (13 bytes, signature, the CBW's tag; a status and residue that mean something): a
//! STALLed data stage is cleared and the CSW read; a STALLed CSW is cleared and read once more.
//! A phase error, a CSW it cannot trust or a command past its timeout is recovered as `Recovery`
//! says: Linux usb-storage's port reset by default, BOT's reset recovery (§5.3.4) as the option.
//! A command that fails (status 1) is followed by REQUEST SENSE.
//! SCSI: SPC-4 (INQUIRY, TEST UNIT READY, REQUEST SENSE, MODE SENSE(6), PREVENT ALLOW MEDIUM
//! REMOVAL) and SBC-3 (READ CAPACITY(10), READ(10), WRITE(10), START STOP UNIT, SYNCHRONIZE
//! CACHE(10)).
use super::fat32::BlockDevice;
use super::{setup_packet, UsbError, UsbHost};

pub const CBW_SIGNATURE: u32 = 0x4342_5355;
pub const CSW_SIGNATURE: u32 = 0x5342_5355;
/// class requests (BOT §3.1, §3.2)
pub const BULK_ONLY_RESET: u8 = 0xff;
pub const GET_MAX_LUN: u8 = 0xfe;
/// interface class 08h, SCSI transparent command set 06h, bulk-only 50h (Mass Storage Class
/// Specification Overview 1.4, §2, §3)
pub const CLASS_MSC: u8 = 0x08;
pub const SUBCLASS_SCSI: u8 = 0x06;
pub const PROTOCOL_BOT: u8 = 0x50;

/// SCSI operation codes.
pub mod op {
    pub const TEST_UNIT_READY: u8 = 0x00; pub const REQUEST_SENSE: u8 = 0x03; pub const INQUIRY: u8 = 0x12; pub const MODE_SENSE6: u8 = 0x1a;
    pub const START_STOP_UNIT: u8 = 0x1b; pub const PREVENT_ALLOW: u8 = 0x1e; pub const READ_CAPACITY10: u8 = 0x25; pub const READ10: u8 = 0x28;
    pub const WRITE10: u8 = 0x2a; pub const SYNCHRONIZE_CACHE10: u8 = 0x35;
}
/// Sense keys and additional sense codes (SPC-4 Table 48, Annex D).
pub mod sense {
    pub const NOT_READY: u8 = 0x02; pub const ILLEGAL_REQUEST: u8 = 0x05; pub const UNIT_ATTENTION: u8 = 0x06;
    pub const MEDIUM_NOT_PRESENT: u8 = 0x3a; pub const INVALID_OPCODE: u8 = 0x20;
}

/// The data stage of a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dir { None, In(usize), Out(Vec<u8>) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Csw { pub tag: u32, pub residue: u32, pub status: u8 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Sense { pub key: u8, pub asc: u8, pub ascq: u8 }
impl std::fmt::Display for Sense {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let what = match (self.key, self.asc, self.ascq) {
            (sense::NOT_READY, sense::MEDIUM_NOT_PRESENT, _) => " (MEDIUM NOT PRESENT)", (sense::UNIT_ATTENTION, 0x28, _) => " (NOT READY TO READY CHANGE)",
            (sense::ILLEGAL_REQUEST, sense::INVALID_OPCODE, _) => " (INVALID COMMAND OPERATION CODE)", _ => "",
        };
        write!(f, "sense {:x}/{:02x}/{:02x}{}", self.key, self.asc, self.ascq, what)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MscError {
    Usb(UsbError),
    /// CHECK CONDITION, with what REQUEST SENSE said
    Check(Sense),
    /// the device reported a phase error (status 2): recovered
    Phase,
    /// a CSW that is not valid or not meaningful (BOT §6.3): recovered
    BadCsw(String),
    /// the data stage moved less than the command needed
    Short { want: usize, got: usize, residue: u32 },
    NotMsc(String),
}
impl From<UsbError> for MscError { fn from(e: UsbError) -> Self { MscError::Usb(e) } }
impl std::fmt::Display for MscError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MscError::Usb(e) => write!(f, "{e}"), MscError::Check(s) => write!(f, "CHECK CONDITION, {s}"), MscError::Phase => write!(f, "phase error"),
            MscError::BadCsw(s) => write!(f, "bad CSW: {s}"), MscError::Short { want, got, residue } => write!(f, "{got} of {want} bytes (residue {residue})"),
            MscError::NotMsc(s) => write!(f, "{s}"),
        }
    }
}

/// How the host gets a device back after a transport error or a command that timed out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Recovery {
    /// Linux's usb-storage (v6.12): a port reset and re-enumeration, the class reset if that
    /// fails; a data stage that runs long gives a faked INVALID FIELD IN CDB; a zero-length CSW
    /// is read again; a residue past the transfer is clamped
    Linux,
    /// BOT 1.0's reset recovery (§5.3.4) for all of them
    Bot,
}

/// Why a transport did not end in a CSW to use.
enum Failure {
    /// the device or the link is gone: nothing to recover
    Fatal(UsbError),
    /// a transport error or a timeout: recover, then report this
    Recover(MscError),
    /// sense the host made up
    Check(Sense),
}

/// A command's result: the data stage and the CSW.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply { pub data: Vec<u8>, pub csw: Csw }

#[derive(Clone, Copy, Debug, Default)]
pub struct MscStats { pub commands: u64, pub check_conditions: u64, pub resets: u64, pub port_resets: u64, pub aborts: u64, pub stalls: u64, pub bytes_read: u64, pub bytes_written: u64 }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inquiry { pub device_type: u8, pub removable: bool, pub vendor: String, pub product: String, pub revision: String }

pub struct Msc<'p> {
    pub host: UsbHost<'p>,
    pub interface: u8,
    pub ep_in: u8,
    pub ep_out: u8,
    pub mps_in: u16,
    pub mps_out: u16,
    pub lun: u8,
    pub max_lun: u8,
    tag: u32,
    pub block_size: u32,
    pub blocks: u64,
    /// blocks per READ(10)/WRITE(10) at most (Linux usb-storage's default max_sectors, 240)
    pub max_blocks: u32,
    /// how long a command may take before it is aborted (Linux sd's SD_TIMEOUT, 30 s)
    pub timeout_ns: u64,
    pub recovery: Recovery,
    pub stats: MscStats,
}

fn be32(b: &[u8]) -> u32 { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) }
/// An INQUIRY string; some devices pad with NULs where SPC-4 says spaces.
fn trimmed(b: &[u8]) -> String { String::from_utf8_lossy(b).trim_end_matches(['\0', ' ']).to_string() }

impl<'p> Msc<'p> {
    /// The bulk-only SCSI interface of a configured device, and its LUN count (GET MAX LUN; a
    /// STALL means one LUN, BOT §3.2).
    pub fn open(host: UsbHost<'p>) -> Result<Self, MscError> {
        let cfg = host.config.clone().ok_or_else(|| MscError::NotMsc("the device is not configured".into()))?;
        let f = cfg.interfaces.iter().find(|f| f.class == CLASS_MSC && f.subclass == SUBCLASS_SCSI && f.protocol == PROTOCOL_BOT)
            .ok_or_else(|| MscError::NotMsc("no bulk-only SCSI interface".into()))?;
        let bulk = |dir_in: bool| f.endpoints.iter().find(|e| e.attributes & 3 == 2 && (e.address & 0x80 != 0) == dir_in).copied();
        let (i, o) = (bulk(true).ok_or_else(|| MscError::NotMsc("no bulk IN endpoint".into()))?, bulk(false).ok_or_else(|| MscError::NotMsc("no bulk OUT endpoint".into()))?);
        let mut m = Msc { interface: f.number, ep_in: i.address, ep_out: o.address, mps_in: i.mps, mps_out: o.mps, host, lun: 0, max_lun: 0, tag: 0,
                          block_size: 512, blocks: 0, max_blocks: 240, timeout_ns: 30_000_000_000, recovery: Recovery::Linux, stats: MscStats::default() };
        m.max_lun = match m.host.control(setup_packet(0xa1, GET_MAX_LUN, 0, m.interface as u16, 1), Vec::new()) {
            Ok(d) => d.first().copied().unwrap_or(0),
            Err(UsbError::Stall(_)) => 0,
            Err(e) => return Err(e.into()),
        };
        m.host.say(format_args!("mass storage: interface {}, bulk IN {:02x} OUT {:02x}, {} LUN(s)", m.interface, m.ep_in, m.ep_out, m.max_lun + 1));
        Ok(m)
    }

    /// Bulk-Only Mass Storage Reset and CLEAR_FEATURE(ENDPOINT_HALT) on both bulk endpoints (BOT §5.3.4).
    pub fn reset_recovery(&mut self) -> Result<(), MscError> {
        self.stats.resets += 1;
        self.host.say(format_args!("reset recovery"));
        self.host.control(setup_packet(0x21, BULK_ONLY_RESET, 0, self.interface as u16, 0), Vec::new())?;
        self.host.clear_halt(self.ep_in)?;
        self.host.clear_halt(self.ep_out)?;
        Ok(())
    }

    /// usb_stor_Bulk_reset (usb_stor_reset_common, Linux v6.12 drivers/usb/storage/transport.c):
    /// the class reset with a 5 s timeout, 6 s for the device to recover, then the halts cleared
    /// on bulk IN and bulk OUT.
    fn linux_bulk_reset(&mut self) -> Result<(), MscError> {
        self.stats.resets += 1;
        self.host.say(format_args!("Bulk-Only Mass Storage Reset"));
        let to = std::mem::replace(&mut self.host.timing.control_timeout_ns, 5_000_000_000);
        let r = self.host.control(setup_packet(0x21, BULK_ONLY_RESET, 0, self.interface as u16, 0), Vec::new());
        self.host.timing.control_timeout_ns = to;
        r?;
        self.host.idle_ns(6_000_000_000)?;
        let a = self.host.clear_halt(self.ep_in);
        self.host.clear_halt(self.ep_out)?;
        a.map_err(MscError::from)
    }

    /// After a transport error or an aborted command. Linux (usb_stor_invoke_transport,
    /// Handle_Errors): a port reset (usb_stor_port_reset, usb_reset_device), the class reset if
    /// that fails. BOT: reset recovery.
    fn recover(&mut self) -> Result<(), MscError> {
        match self.recovery {
            Recovery::Bot => self.reset_recovery(),
            Recovery::Linux => {
                self.stats.port_resets += 1;
                self.host.say(format_args!("port reset"));
                match self.host.reset_device() {
                    Ok(()) => Ok(()),
                    Err(e) => { self.host.say(format_args!("port reset failed: {e}")); self.linux_bulk_reset() }
                }
            }
        }
    }

    fn csw_in(&mut self, deadline: u64) -> Result<Vec<u8>, UsbError> {
        let t = deadline.saturating_sub(self.host.now_ns()).max(1);
        self.host.bulk_in(self.ep_in, self.mps_in, 13, t)
    }

    /// One command through bulk-only transport (BOT §5.3, the host side of §6.7), recovered as
    /// `recovery` says when it fails.
    pub fn command(&mut self, cdb: &[u8], dir: Dir) -> Result<Reply, MscError> {
        match self.transport(cdb, dir) {
            Ok(r) => Ok(r),
            Err(Failure::Fatal(e)) => Err(e.into()),
            Err(Failure::Check(s)) => { self.stats.check_conditions += 1; Err(MscError::Check(s)) }
            Err(Failure::Recover(e)) => {
                if e == MscError::Usb(UsbError::Timeout) { self.stats.aborts += 1; self.host.say(format_args!("command aborted after {} ms", self.timeout_ns / 1_000_000)); }
                self.recover()?;
                Err(e)
            }
        }
    }

    fn transport(&mut self, cdb: &[u8], dir: Dir) -> Result<Reply, Failure> {
        let linux = self.recovery == Recovery::Linux;
        self.tag = self.tag.wrapping_add(1);
        self.stats.commands += 1;
        let deadline = self.host.now_ns() + self.timeout_ns;
        let left = |h: &mut UsbHost| deadline.saturating_sub(h.now_ns()).max(1);
        // a STALL's halt is cleared before going on, as BOT §5.3.2 and Linux's interpret_urb_result do
        let fail = |e: UsbError| match e { UsbError::Timeout => Failure::Recover(MscError::Usb(UsbError::Timeout)), e => Failure::Fatal(e) };
        let (len, flags) = match &dir { Dir::None => (0, 0), Dir::In(n) => (*n, 0x80), Dir::Out(d) => (d.len(), 0) };
        let mut cbw = Vec::with_capacity(31);
        cbw.extend_from_slice(&CBW_SIGNATURE.to_le_bytes());
        cbw.extend_from_slice(&self.tag.to_le_bytes());
        cbw.extend_from_slice(&(len as u32).to_le_bytes());
        cbw.extend_from_slice(&[flags, self.lun & 15, cdb.len() as u8 & 31]);
        let mut cb = [0u8; 16];
        cb[..cdb.len().min(16)].copy_from_slice(&cdb[..cdb.len().min(16)]);
        cbw.extend_from_slice(&cb);
        let t = left(&mut self.host);
        match self.host.bulk_out(self.ep_out, self.mps_out, cbw, t) {
            Ok(()) => {}
            Err(UsbError::Stall(_)) => {
                self.stats.stalls += 1;
                if linux { self.host.clear_halt(self.ep_out).map_err(Failure::Fatal)?; }
                return Err(Failure::Recover(MscError::BadCsw("the CBW STALLed".into())));
            }
            Err(e) => return Err(fail(e)),
        }
        let mut data = Vec::new();
        let mut too_long = false;
        let t = left(&mut self.host);
        match dir {
            Dir::None => {}
            Dir::In(n) => match self.host.bulk_in(self.ep_in, self.mps_in, n, t) {
                Ok(d) => data = d,
                Err(UsbError::Stall(d)) => { self.stats.stalls += 1; data = d; self.host.clear_halt(self.ep_in).map_err(Failure::Fatal)?; }
                // Linux reads the CSW after a data stage that ran long and fakes the sense
                Err(UsbError::Babble) if linux => too_long = true,
                Err(UsbError::Babble) => return Err(Failure::Recover(MscError::Phase)),
                Err(e) => return Err(fail(e)),
            },
            Dir::Out(d) => match self.host.bulk_out(self.ep_out, self.mps_out, d, t) {
                Ok(()) => {}
                Err(UsbError::Stall(_)) => { self.stats.stalls += 1; self.host.clear_halt(self.ep_out).map_err(Failure::Fatal)?; }
                Err(e) => return Err(fail(e)),
            },
        }
        let mut raw = match self.csw_in(deadline) {
            Ok(r) => r,
            Err(UsbError::Stall(_)) => {
                self.stats.stalls += 1;
                self.host.clear_halt(self.ep_in).map_err(Failure::Fatal)?;
                match self.csw_in(deadline) {
                    Ok(r) => r,
                    Err(UsbError::Stall(_)) => return Err(Failure::Recover(MscError::BadCsw("the CSW STALLed twice".into()))),
                    Err(UsbError::Babble) => return Err(Failure::Recover(MscError::BadCsw("more than 13 bytes".into()))),
                    Err(e) => return Err(fail(e)),
                }
            }
            Err(UsbError::Babble) => return Err(Failure::Recover(MscError::BadCsw("more than 13 bytes".into()))),
            Err(e) => return Err(fail(e)),
        };
        // a zero-length packet where the CSW should be: Linux reads again
        if linux && raw.is_empty() { raw = self.csw_in(deadline).map_err(fail)?; }
        let valid = raw.len() == 13 && u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) == CSW_SIGNATURE;
        if !valid || u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) != self.tag {
            return Err(Failure::Recover(MscError::BadCsw(if valid { "tag mismatch".into() } else { format!("{} bytes: {:02x?}", raw.len(), raw) })));
        }
        let mut csw = Csw { tag: self.tag, residue: u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]), status: raw[12] };
        if csw.status == 2 { return Err(Failure::Recover(MscError::Phase)); }
        if csw.status > 2 { return Err(Failure::Recover(MscError::BadCsw(format!("status {}", csw.status)))); }
        if csw.residue as usize > len {
            if !linux { return Err(Failure::Recover(MscError::BadCsw(format!("residue {} for {} bytes", csw.residue, len)))); }
            csw.residue = len as u32;
        }
        // INVALID FIELD IN CDB, as Linux fakes it for a data stage that ran long and a good status
        if too_long && csw.status == 0 { return Err(Failure::Check(Sense { key: sense::ILLEGAL_REQUEST, asc: 0x24, ascq: 0 })); }
        Ok(Reply { data, csw })
    }

    /// A SCSI command; on CHECK CONDITION the sense data comes back as the error.
    pub fn scsi(&mut self, cdb: &[u8], dir: Dir) -> Result<Vec<u8>, MscError> {
        let r = self.command(cdb, dir)?;
        if r.csw.status == 1 {
            self.stats.check_conditions += 1;
            let s = self.request_sense()?;
            return Err(MscError::Check(s));
        }
        Ok(r.data)
    }

    pub fn request_sense(&mut self) -> Result<Sense, MscError> {
        let r = self.command(&[op::REQUEST_SENSE, 0, 0, 0, 18, 0], Dir::In(18))?;
        let d = &r.data;
        // fixed format (response code 70h/71h, SPC-4 §4.5.3)
        if d.len() < 14 || d[0] & 0x7e != 0x70 { return Err(MscError::BadCsw(format!("sense data {:02x?}", d))); }
        Ok(Sense { key: d[2] & 15, asc: d[12], ascq: d[13] })
    }

    pub fn inquiry(&mut self) -> Result<Inquiry, MscError> {
        let d = self.scsi(&[op::INQUIRY, 0, 0, 0, 36, 0], Dir::In(36))?;
        if d.len() < 36 { return Err(MscError::Short { want: 36, got: d.len(), residue: 0 }); }
        Ok(Inquiry { device_type: d[0] & 0x1f, removable: d[1] & 0x80 != 0, vendor: trimmed(&d[8..16]), product: trimmed(&d[16..32]), revision: trimmed(&d[32..36]) })
    }

    pub fn test_unit_ready(&mut self) -> Result<(), MscError> { self.scsi(&[op::TEST_UNIT_READY, 0, 0, 0, 0, 0], Dir::None).map(|_| ()) }

    /// (last LBA + 1, block length); kept for the block device.
    pub fn read_capacity(&mut self) -> Result<(u64, u32), MscError> {
        let d = self.scsi(&[op::READ_CAPACITY10, 0, 0, 0, 0, 0, 0, 0, 0, 0], Dir::In(8))?;
        if d.len() < 8 { return Err(MscError::Short { want: 8, got: d.len(), residue: 0 }); }
        self.blocks = be32(&d[..4]) as u64 + 1;
        self.block_size = be32(&d[4..8]);
        // Linux's sd_read_capacity: another block length leaves the disk at capacity 0 with
        // 512-byte blocks, so nothing divides by or allocates from it.
        if ![512, 1024, 2048, 4096].contains(&self.block_size) {
            self.host.say(format_args!("READ CAPACITY: unsupported block length {}, capacity 0", self.block_size));
            (self.blocks, self.block_size) = (0, 512);
        }
        Ok((self.blocks, self.block_size))
    }

    /// MODE SENSE(6) of all pages; true when the medium is write-protected (WP, bit 7 of the
    /// device-specific parameter, SBC-3 §6.3.1).
    pub fn mode_sense(&mut self) -> Result<bool, MscError> {
        let d = self.scsi(&[op::MODE_SENSE6, 0, 0x3f, 0, 192, 0], Dir::In(192))?;
        Ok(d.get(2).is_some_and(|b| b & 0x80 != 0))
    }

    pub fn prevent_removal(&mut self, prevent: bool) -> Result<(), MscError> { self.scsi(&[op::PREVENT_ALLOW, 0, 0, 0, prevent as u8, 0], Dir::None).map(|_| ()) }
    pub fn start_stop(&mut self, start: bool, eject: bool) -> Result<(), MscError> { self.scsi(&[op::START_STOP_UNIT, 0, 0, 0, start as u8 | (eject as u8) << 1, 0], Dir::None).map(|_| ()) }
    pub fn sync_cache(&mut self) -> Result<(), MscError> { self.scsi(&[op::SYNCHRONIZE_CACHE10, 0, 0, 0, 0, 0, 0, 0, 0, 0], Dir::None).map(|_| ()) }

    fn rw10(op: u8, lba: u32, blocks: u16) -> [u8; 10] { let l = lba.to_be_bytes(); let n = blocks.to_be_bytes(); [op, 0, l[0], l[1], l[2], l[3], 0, n[0], n[1], 0] }

    /// READ(10) of `blocks` blocks from `lba`, in commands of `max_blocks` at most.
    pub fn read10(&mut self, lba: u64, blocks: u32) -> Result<Vec<u8>, MscError> {
        let bs = self.block_size as usize;
        let mut out = Vec::with_capacity(blocks.min(self.max_blocks) as usize * bs);
        let (mut at, mut left) = (lba, blocks);
        while left > 0 {
            let n = left.min(self.max_blocks).min(0xffff);
            let want = n as usize * bs;
            let r = self.command(&Self::rw10(op::READ10, at as u32, n as u16), Dir::In(want))?;
            if r.csw.status == 1 { self.stats.check_conditions += 1; let s = self.request_sense()?; return Err(MscError::Check(s)); }
            if r.data.len() != want || r.csw.residue != 0 { return Err(MscError::Short { want, got: r.data.len(), residue: r.csw.residue }); }
            self.stats.bytes_read += want as u64;
            out.extend_from_slice(&r.data);
            at += n as u64;
            left -= n;
        }
        Ok(out)
    }

    /// WRITE(10) of `data` (whole blocks) at `lba`, in commands of `max_blocks` at most.
    pub fn write10(&mut self, lba: u64, data: &[u8]) -> Result<(), MscError> {
        let bs = self.block_size as usize;
        let mut at = lba;
        for chunk in data.chunks(self.max_blocks.min(0xffff) as usize * bs) {
            let n = chunk.len().div_ceil(bs);
            let r = self.command(&Self::rw10(op::WRITE10, at as u32, n as u16), Dir::Out(chunk.to_vec()))?;
            if r.csw.status == 1 { self.stats.check_conditions += 1; let s = self.request_sense()?; return Err(MscError::Check(s)); }
            if r.csw.residue != 0 { return Err(MscError::Short { want: chunk.len(), got: chunk.len() - r.csw.residue as usize, residue: r.csw.residue }); }
            self.stats.bytes_written += chunk.len() as u64;
            at += n as u64;
        }
        Ok(())
    }

    /// TEST UNIT READY every `interval_ns` until it passes, as a host polls a removable
    /// drive; returns how many polls failed. A failure other than CHECK CONDITION ends it.
    pub fn wait_ready(&mut self, interval_ns: u64, timeout_ns: u64) -> Result<u32, MscError> {
        let deadline = self.host.now_ns() + timeout_ns;
        let mut failed = 0;
        let mut last: Option<Sense> = None;
        loop {
            match self.test_unit_ready() {
                Ok(()) => return Ok(failed),
                Err(MscError::Check(s)) => {
                    if last != Some(s) { self.host.say(format_args!("TEST UNIT READY: {s}")); last = Some(s); }
                    failed += 1;
                }
                Err(e) => return Err(e),
            }
            if self.host.now_ns() + interval_ns > deadline { return Err(MscError::Usb(UsbError::Timeout)); }
            self.host.idle_ns(interval_ns)?;
        }
    }
}

impl BlockDevice for Msc<'_> {
    fn block_size(&self) -> u32 { self.block_size }
    fn blocks(&self) -> u64 { self.blocks }
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), String> {
        let bs = self.block_size as usize;
        if bs == 0 || !buf.len().is_multiple_of(bs) { return Err(format!("{} bytes is not a whole number of {bs}-byte blocks", buf.len())); }
        let d = self.read10(lba, (buf.len() / bs) as u32).map_err(|e| e.to_string())?;
        buf.copy_from_slice(&d);
        Ok(())
    }
    fn write_blocks(&mut self, lba: u64, data: &[u8]) -> Result<(), String> { self.write10(lba, data).map_err(|e| e.to_string()) }
    fn pause_ns(&mut self, ns: u64) -> Result<(), String> { self.host.idle_ns(ns).map_err(|e| e.to_string()) }
    fn now_ns(&mut self) -> u64 { self.host.now_ns() }
}
