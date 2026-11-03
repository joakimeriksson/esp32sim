//! The SD/MMC host (`Sdmmc`) and the SD card (`SdCard`) from outside the crate: the host's
//! self-clearing resets and START bit, its interrupt registers, the IDMAC's descriptor walk
//! with the OWN hand-back, suspend and resume; the card's identification sequence, R1 status,
//! data transfers, stop and errors (SD Physical Layer Simplified Specification v9.10).
use emu_core::Fault;
use esp_periph::sdmmc::card::{crc7, status, CardState, FileStore, MemStore, SdCard, SdCardConfig, SdStore};
use esp_periph::sdmmc::{des0, idsts, int, reg, SdCmdError, SdDataError, SdDevice, SdResponse, Sdmmc, SdmmcLayout, SdmmcMemory, BMOD_DE, CMD_AUTO_STOP, CMD_DATA, CMD_RESP, CMD_RESP_CRC, CMD_RESP_LONG, CMD_START, CMD_UPDATE_CLK, CMD_WRITE, CTRL_INT_ENABLE, CTRL_USE_INTERNAL_DMA};
use esp_periph::Device;

const RCA_ARG: u32 = 0xe624 << 16;

/// A 4 MiB image whose bytes count up, block by block.
fn image() -> Vec<u8> { (0..4 << 20).map(|i: u32| (i / 512 + i % 251) as u8).collect() }
fn card() -> SdCard { SdCard::new(Box::new(MemStore(image()))).unwrap() }
fn short(r: Result<SdResponse, SdCmdError>) -> u32 { match r { Ok(SdResponse::Short(v)) => v, other => panic!("expected a short response, got {other:?}") } }
fn state(r1: u32) -> u32 { r1 >> 9 & 0xf }

/// CMD0, CMD8, ACMD41 until ready, CMD2, CMD3, CMD7: the card in transfer state.
fn to_tran(c: &mut SdCard) {
    assert_eq!(c.command(0, 0, 0), Err(SdCmdError::NoResponse));
    assert_eq!(short(c.command(0, 8, 0x1aa)), 0x1aa);
    loop {
        short(c.command(0, 55, 0));
        if short(c.command(0, 41, 0x40ff_8000)) & 1 << 31 != 0 { break; }
    }
    assert!(matches!(c.command(0, 2, 0), Ok(SdResponse::Long(_))));
    short(c.command(0, 3, 0));
    short(c.command(0, 7, RCA_ARG));
}

// ------------------------------------------------------------------ card

#[test]
fn crc7_matches_the_specification_examples() {
    // SD spec §4.5: CMD0 with argument 0 ends 0x95, CMD8 with 0x1AA ends 0x87.
    assert_eq!(crc7(&[0x40, 0, 0, 0, 0]) << 1 | 1, 0x95);
    assert_eq!(crc7(&[0x48, 0, 0, 1, 0xaa]) << 1 | 1, 0x87);
}

#[test]
fn identification_takes_the_card_from_idle_to_transfer() {
    let mut c = card();
    assert_eq!(c.command(0, 0, 0), Err(SdCmdError::NoResponse), "CMD0 has no response");
    assert_eq!(short(c.command(0, 8, 0x1aa)), 0x1aa, "CMD8 echoes the voltage and the check pattern (R7)");
    let r1 = short(c.command(0, 55, 0));
    assert_eq!((state(r1), r1 & status::APP_CMD), (0, status::APP_CMD), "CMD55 in idle expects an ACMD");
    let ocr = short(c.command(0, 41, 0x40ff_8000));
    assert_eq!(ocr & 1 << 31, 0, "the first ACMD41 finds the card still powering up");
    short(c.command(0, 55, 0));
    let ocr = short(c.command(0, 41, 0x40ff_8000));
    assert_eq!(ocr, 0xc0ff_8000, "ready, high capacity (CCS), 2.7-3.6 V");
    assert_eq!(c.state(), CardState::Ready);
    let Ok(SdResponse::Long(cid)) = c.command(0, 2, 0) else { panic!("CMD2 answers R2") };
    let name: Vec<u8> = (0..5).map(|i| { let p = 96 - 8 * i; (cid[p / 32] >> (p % 32)) as u8 }).collect();
    assert_eq!(&name, b"SIMSD", "CID product name");
    assert_eq!(cid[0] & 0xff, (crc7(&c.cid().to_be_bytes()[..15]) << 1 | 1) as u32, "CID ends with its CRC7 and a 1");
    let r6 = short(c.command(0, 3, 0));
    assert_eq!((r6 >> 16, r6 >> 9 & 0xf), (0xe624, CardState::Ident as u32), "R6: the new RCA, the state CMD3 found");
    let Ok(SdResponse::Long(csd)) = c.command(0, 9, RCA_ARG) else { panic!("CMD9 answers R2") };
    assert_eq!(csd[3] >> 30, 1, "CSD version 2.0");
    let c_size = (csd[1] >> 16) | (csd[2] & 0x3f) << 16;
    assert_eq!((c_size as u64 + 1) * 1024, c.blocks(), "capacity from C_SIZE");
    assert_eq!(c.blocks(), 8192, "4 MiB");
    assert_eq!(c.command(0, 9, 0x1234 << 16), Err(SdCmdError::NoResponse), "a card that is not addressed stays silent");
    let r1 = short(c.command(0, 7, RCA_ARG));
    assert_eq!(state(r1), CardState::Stby as u32, "CMD7's R1b shows standby, the state it found");
    let r1 = short(c.command(0, 13, RCA_ARG));
    assert_eq!((state(r1), r1 & status::READY_FOR_DATA), (CardState::Tran as u32, status::READY_FOR_DATA));
}

#[test]
fn the_card_needs_a_compatible_host() {
    let mut c = card();
    assert_eq!(c.command(0, 0, 0), Err(SdCmdError::NoResponse));
    assert_eq!(c.command(0, 8, 0x2aa), Err(SdCmdError::NoResponse), "CMD8 with a voltage the card cannot take is not answered");
    for _ in 0..3 { short(c.command(0, 55, 0)); assert_eq!(short(c.command(0, 41, 0x40ff_8000)) & 1 << 31, 0, "without CMD8 a high capacity card stays busy"); }
    assert_eq!(c.command(0, 0, 0), Err(SdCmdError::NoResponse));
    short(c.command(0, 8, 0x1aa));
    for _ in 0..3 { short(c.command(0, 55, 0)); assert_eq!(short(c.command(0, 41, 0x00ff_8000)) & 1 << 31, 0, "nor without HCS"); }
    short(c.command(0, 55, 0));
    assert_eq!(short(c.command(0, 41, 0)), 0x00ff_8000, "an empty voltage window only asks for the OCR");
}

#[test]
fn an_illegal_command_is_unanswered_and_reported_once() {
    let mut c = card();
    to_tran(&mut c);
    assert_eq!(c.command(0, 2, 0), Err(SdCmdError::NoResponse), "CMD2 is not legal in transfer state");
    assert_eq!(c.command(0, 52, 0x8000_0c08), Err(SdCmdError::NoResponse), "nor are the SDIO commands on a memory card");
    let r1 = short(c.command(0, 13, RCA_ARG));
    assert_eq!(r1 & status::ILLEGAL_COMMAND, status::ILLEGAL_COMMAND, "the next response reports it");
    assert_eq!(short(c.command(0, 13, RCA_ARG)) & status::ILLEGAL_COMMAND, 0, "and that clears it");
    assert_eq!(c.state(), CardState::Tran, "an illegal command changes no state");
}

#[test]
fn single_and_multi_block_transfers_and_stop() {
    let mut c = card();
    to_tran(&mut c);
    let img = image();
    let r1 = short(c.command(0, 17, 3));
    assert_eq!(state(r1), CardState::Tran as u32);
    assert_eq!(c.state(), CardState::Data);
    let mut b = [0u8; 512];
    c.read_data(0, &mut b).unwrap();
    assert_eq!(&b[..], &img[3 * 512..4 * 512]);
    assert_eq!(c.state(), CardState::Tran, "back to transfer after one block");
    assert_eq!(c.read_data(0, &mut b), Err(SdDataError::Timeout), "nothing more to send");

    let data: Vec<u8> = (0..3 * 512).map(|i| (i * 7) as u8).collect();
    short(c.command(0, 25, 100));
    assert_eq!(c.state(), CardState::Rcv);
    c.write_data(0, &data[..700]).unwrap();
    c.write_data(0, &data[700..]).unwrap();
    let r1 = short(c.command(0, 12, 0));
    assert_eq!(state(r1), CardState::Rcv as u32, "CMD12 found the card receiving");
    assert_eq!(c.state(), CardState::Tran, "programming is instant");
    assert_eq!(c.blocks_written, 3);
    short(c.command(0, 55, RCA_ARG));
    short(c.command(0, 22, 0));
    let mut n = [0u8; 4];
    c.read_data(0, &mut n).unwrap();
    assert_eq!(u32::from_be_bytes(n), 3, "ACMD22 counts the blocks written");

    short(c.command(0, 18, 99));
    let mut out = vec![0u8; 5 * 512];
    c.read_data(0, &mut out[..1000]).unwrap();
    c.read_data(0, &mut out[1000..]).unwrap();
    assert_eq!(&out[..512], &img[99 * 512..100 * 512]);
    assert_eq!(&out[512..4 * 512], &data[..]);
    assert_eq!(&out[4 * 512..], &img[103 * 512..104 * 512]);
    assert_eq!(c.state(), CardState::Data, "a multi-block read runs until CMD12");
    assert_eq!(state(short(c.command(0, 12, 0))), CardState::Data as u32);
    assert_eq!(c.state(), CardState::Tran);
    assert_eq!(c.command(0, 12, 0), Err(SdCmdError::NoResponse), "CMD12 with nothing to stop is illegal");

    short(c.command(0, 23, 2));
    short(c.command(0, 18, 10));
    let mut two = vec![0u8; 1024];
    c.read_data(0, &mut two).unwrap();
    assert_eq!(c.state(), CardState::Tran, "CMD23 ends the read after its count");
}

#[test]
fn addresses_beyond_the_card_are_out_of_range() {
    let mut c = card();
    to_tran(&mut c);
    let end = c.blocks() as u32;
    let r1 = short(c.command(0, 17, end));
    assert_eq!(r1 & status::OUT_OF_RANGE, status::OUT_OF_RANGE);
    assert_eq!(c.state(), CardState::Tran, "and no data phase starts");
    let mut b = [0u8; 512];
    assert_eq!(c.read_data(0, &mut b), Err(SdDataError::Timeout));
    assert_eq!(short(c.command(0, 13, RCA_ARG)) & status::OUT_OF_RANGE, 0, "cleared by the response that reported it");
    short(c.command(0, 18, end - 1));
    let mut two = [0u8; 1024];
    assert_eq!(c.read_data(0, &mut two), Err(SdDataError::Timeout), "a multi-block read stops at the end");
    short(c.command(0, 12, 0));
    let r1 = short(c.command(0, 24, end));
    assert_eq!(r1 & status::OUT_OF_RANGE, status::OUT_OF_RANGE);
    assert_eq!(c.write_data(0, &[0; 512]), Err(SdDataError::NoCrcStatus), "the card does not take the block");
}

#[test]
fn registers_bus_width_and_high_speed() {
    let mut c = card();
    to_tran(&mut c);
    short(c.command(0, 55, RCA_ARG));
    let r1 = short(c.command(0, 51, 0));
    assert_eq!(r1 & status::APP_CMD, status::APP_CMD, "an ACMD's R1 says it was taken as one");
    let mut scr = [0u8; 8];
    c.read_data(0, &mut scr).unwrap();
    assert_eq!(scr[..4], [0x02, 0x35, 0x80, 0x02], "SCR: SD 3.0x, security 2.00, 1- and 4-bit, CMD23");
    assert_eq!(c.bus_width(), Some(1));
    short(c.command(0, 55, RCA_ARG));
    short(c.command(0, 6, 2));
    assert_eq!(c.bus_width(), Some(4), "ACMD6 selects four lines");
    short(c.command(0, 55, RCA_ARG));
    short(c.command(0, 13, 0));
    let mut ssr = [0u8; 64];
    c.read_data(0, &mut ssr).unwrap();
    assert_eq!(ssr[0] >> 6, 2, "SD status: four lines");
    short(c.command(0, 6, 0x00ff_fff1));
    let mut sw = [0u8; 64];
    c.read_data(0, &mut sw).unwrap();
    assert_eq!((sw[12], sw[13]), (0x80, 0x03), "group 1 offers default and high speed");
    assert_eq!(sw[16] & 0xf, 1, "a check of function 1 reports it would be selected");
    assert_eq!(sw[17], 1, "data structure version 1");
    assert_eq!(c.csd() >> 96 & 0xff, 0x32, "a check switches nothing");
    short(c.command(0, 6, 0x80ff_fff1));
    c.read_data(0, &mut sw).unwrap();
    assert_eq!(c.csd() >> 96 & 0xff, 0x5a, "TRAN_SPEED 50 MHz after the switch");
    short(c.command(0, 6, 0x80ff_ff2f));
    c.read_data(0, &mut sw).unwrap();
    assert_eq!(sw[16] >> 4, 0xf, "an unsupported function reads 0xF");
    assert_eq!(c.csd() >> 96 & 0xff, 0x5a, "and nothing switches");
}

#[test]
fn erase_zeroes_blocks() {
    let mut c = card();
    to_tran(&mut c);
    short(c.command(0, 32, 5));
    short(c.command(0, 33, 6));
    short(c.command(0, 38, 0));
    short(c.command(0, 17, 6));
    let mut b = [0xffu8; 512];
    c.read_data(0, &mut b).unwrap();
    assert!(b.iter().all(|&x| x == 0), "erased blocks read 0");
    assert_eq!(short(c.command(0, 38, 0)) & status::ERASE_SEQ_ERROR, status::ERASE_SEQ_ERROR, "CMD38 needs CMD32 and CMD33");

}

/// A card with a programming time: DAT0 stays busy for that long in emulated time after a
/// write, looking does not shorten it, and a read sent meanwhile is illegal, the next CMD13
/// reporting ILLEGAL_COMMAND in prg (0x400e00), as a card does while it programs.
#[test]
fn programming_takes_its_time_and_refuses_reads_meanwhile() {
    let mut c = SdCard::with_config(Box::new(MemStore(image())), SdCardConfig { prg_ns: 1_000_000, ..Default::default() }).unwrap();
    to_tran(&mut c);
    short(c.command(0, 24, 7));
    c.write_data(10_000, &[9; 512]).unwrap();
    assert_eq!(c.state(), CardState::Prg);
    for t in [10_000, 500_000, 1_009_999] { assert!(c.busy(t), "DAT0 busy at {t} ns"); }
    assert!(!c.busy(1_010_000), "released after 1 ms");
    let r1 = short(c.command(400_000, 13, RCA_ARG));
    assert_eq!((state(r1), r1 & status::READY_FOR_DATA), (CardState::Prg as u32, 0), "programming, not ready for data");
    assert_eq!(c.command(600_000, 18, 0), Err(SdCmdError::NoResponse), "CMD18 is illegal in prg: no response");
    assert_eq!(short(c.command(700_000, 13, RCA_ARG)), 0x0040_0e00, "ILLEGAL_COMMAND, CURRENT_STATE prg, not ready");
    let r1 = short(c.command(1_010_000, 13, RCA_ARG));
    assert_eq!((state(r1), r1 & status::READY_FOR_DATA), (CardState::Tran as u32, status::READY_FOR_DATA), "back in transfer state");
    short(c.command(2_000_000, 25, 8));
    c.write_data(2_000_000, &[1; 1024]).unwrap();
    assert!(!c.busy(2_000_000), "a multi-block write programs after CMD12");
    short(c.command(2_100_000, 12, 0));
    assert!(c.busy(2_100_000) && !c.busy(3_100_000));
}

/// Deselected while programming (CMD7 to another RCA), a card goes to dis, then stby when
/// programming ends; selected again it returns to prg only while still programming (SD spec
/// §4.8).
#[test]
fn deselect_while_programming() {
    let mut c = SdCard::with_config(Box::new(MemStore(image())), SdCardConfig { prg_ns: 1_000_000, ..Default::default() }).unwrap();
    to_tran(&mut c);
    short(c.command(0, 24, 7));
    c.write_data(0, &[9; 512]).unwrap();
    assert_eq!(c.command(100_000, 7, 0), Err(SdCmdError::NoResponse), "deselect: not addressed, no answer");
    assert_eq!(c.state(), CardState::Dis);
    let r1 = short(c.command(200_000, 7, RCA_ARG));
    assert_eq!((state(r1), c.state()), (CardState::Dis as u32, CardState::Prg), "still programming: back to prg");
    c.command(300_000, 7, 0).unwrap_err();
    let r1 = short(c.command(2_000_000, 13, RCA_ARG));
    assert_eq!(state(r1), CardState::Stby as u32, "programming ended while deselected: stby");
    short(c.command(2_000_000, 7, RCA_ARG));
    assert_eq!(c.state(), CardState::Tran, "and selecting it now gives tran");
}

#[test]
fn file_images_copy_on_write_or_write_through() {
    let path = std::env::temp_dir().join(format!("esp32sim-sd-{}.img", std::process::id()));
    std::fs::write(&path, vec![0x11u8; 1 << 20]).unwrap();
    let p = path.to_str().unwrap();
    let mut cow = FileStore::copy_on_write(p).unwrap();
    cow.write(3, &[0x22; 512]).unwrap();
    let mut b = [0u8; 512];
    cow.read(3, &mut b).unwrap();
    assert_eq!(b[0], 0x22, "the overlay answers reads");
    assert!(std::fs::read(&path).unwrap().iter().all(|&x| x == 0x11), "the file is untouched");
    let mut wt = FileStore::write_through(p).unwrap();
    wt.write(3, &[0x33; 512]).unwrap();
    assert_eq!(std::fs::read(&path).unwrap()[3 * 512], 0x33);
    assert!(SdCard::new(Box::new(MemStore(vec![0; 1000]))).is_err(), "less than one capacity unit");
    std::fs::remove_file(&path).unwrap();
}

// ------------------------------------------------------------------ host

/// Memory at 0x4FF00000 for the IDMAC.
struct Ram(Vec<u8>);
const RAM: u32 = 0x4ff0_0000;
impl Ram {
    fn range(&self, a: u32, n: usize) -> Result<std::ops::Range<usize>, Fault> {
        let s = a.checked_sub(RAM).ok_or(Fault::Unmapped)? as usize;
        if s + n > self.0.len() { return Err(Fault::Unmapped); }
        Ok(s..s + n)
    }
    fn desc(&mut self, at: u32, des0: u32, size: u32, buf: u32, next: u32) {
        for (i, w) in [des0, size, buf, next].iter().enumerate() { let r = self.range(at + 4 * i as u32, 4).unwrap(); self.0[r].copy_from_slice(&w.to_le_bytes()); }
    }
    fn word(&self, at: u32) -> u32 { u32::from_le_bytes(self.0[self.range(at, 4).unwrap()].try_into().unwrap()) }
}
impl SdmmcMemory for Ram {
    fn read(&mut self, a: u32, buf: &mut [u8]) -> Result<(), Fault> { let r = self.range(a, buf.len())?; buf.copy_from_slice(&self.0[r]); Ok(()) }
    fn write(&mut self, a: u32, data: &[u8]) -> Result<(), Fault> { let r = self.range(a, data.len())?; self.0[r].copy_from_slice(data); Ok(()) }
}

fn host() -> (Sdmmc, Ram) {
    let mut h = Sdmmc::new(SdmmcLayout::Esp32p4);
    let i = h.attach(Box::new(card()));
    h.route[1] = Some(i);
    (h, Ram(vec![0; 64 << 10]))
}

/// Start a command on slot 1 and let the host carry it out; returns RINTSTS, cleared.
fn send(h: &mut Sdmmc, m: &mut Ram, index: u32, arg: u32, flags: u32) -> u32 {
    h.write(reg::CMDARG, arg);
    h.write(reg::CMD, CMD_START | 1 << 16 | flags | index);
    h.run(m);
    let r = h.read(reg::RINTSTS);
    h.write(reg::RINTSTS, r);
    r
}

fn host_to_tran(h: &mut Sdmmc, m: &mut Ram) {
    send(h, m, 0, 0, 0);
    send(h, m, 8, 0x1aa, CMD_RESP);
    loop {
        send(h, m, 55, 0, CMD_RESP);
        send(h, m, 41, 0x40ff_8000, CMD_RESP);
        if h.read(reg::RESP0) & 1 << 31 != 0 { break; }
    }
    send(h, m, 2, 0, CMD_RESP | CMD_RESP_LONG);
    send(h, m, 3, 0, CMD_RESP);
    send(h, m, 7, RCA_ARG, CMD_RESP);
}

#[test]
fn resets_and_start_bits_clear_themselves() {
    let (mut h, mut m) = host();
    h.write(reg::CTRL, 7 | CTRL_INT_ENABLE);
    assert_eq!(h.read(reg::CTRL), CTRL_INT_ENABLE, "controller, FIFO and DMA reset done; the rest stays");
    assert_eq!((h.read(reg::BLKSIZ), h.read(reg::BYTCNT), h.read(reg::TMOUT)), (512, 512, 0xffff_ff40), "reset values");
    assert_eq!(h.read(reg::VERID), 0x5432_270a);
    h.write(reg::CMD, CMD_START | CMD_UPDATE_CLK | 1 << 13);
    assert_eq!(h.read(reg::CMD) & CMD_START, 0, "a clock update is taken at once");
    assert!(!h.pending());
    assert_eq!(h.read(reg::RINTSTS), 0, "and raises no CMD_DONE");
    h.write(reg::CMDARG, 0x1aa);
    h.write(reg::CMD, CMD_START | 1 << 16 | CMD_RESP | 8);
    assert!(h.pending() && h.read(reg::CMD) & CMD_START != 0, "a command waits for the CIU");
    h.run(&mut m);
    assert_eq!(h.read(reg::CMD) & CMD_START, 0, "taken");
    assert_eq!(h.read(reg::RESP0), 0x1aa);
    assert_eq!(h.read(reg::STATUS) >> 11 & 0x3f, 8, "STATUS response index");
    h.write(reg::BMOD, 1 | BMOD_DE);
    assert_eq!(h.read(reg::BMOD), BMOD_DE, "the IDMAC's software reset clears itself");
}

#[test]
fn interrupts_mask_and_clear() {
    let (mut h, mut m) = host();
    h.write(reg::CMD, CMD_START | 1 << 16);
    h.run(&mut m);
    assert_eq!(h.read(reg::RINTSTS), int::CMD_DONE, "CMD0: done, nothing expected back");
    assert_eq!(h.read(reg::MINTSTS), 0, "masked");
    assert!(!h.irq());
    h.write(reg::INTMASK, int::CMD_DONE | int::RTO);
    assert_eq!(h.read(reg::MINTSTS), int::CMD_DONE);
    assert!(!h.irq(), "the global interrupt enable is off");
    h.write(reg::CTRL, CTRL_INT_ENABLE);
    assert!(h.irq());
    h.write(reg::RINTSTS, int::DATA_OVER);
    assert_eq!(h.read(reg::RINTSTS), int::CMD_DONE, "write 1 to clear: other bits stay");
    h.write(reg::RINTSTS, int::CMD_DONE);
    assert!(!h.irq());
    h.write(reg::CMD, CMD_START | CMD_RESP | 55);
    h.run(&mut m);
    assert_eq!(h.read(reg::RINTSTS), int::RTO | int::CMD_DONE, "slot 0 holds nothing: response timeout");
    assert_eq!(h.stats.timeouts, 1);
}

#[test]
fn responses_short_and_long() {
    let (mut h, mut m) = host();
    host_to_tran(&mut h, &mut m);
    send(&mut h, &mut m, 7, 0, 0);
    let r = send(&mut h, &mut m, 9, RCA_ARG, CMD_RESP | CMD_RESP_LONG);
    assert_eq!(r, int::CMD_DONE);
    assert_eq!(h.read(reg::RESP0 + 12) >> 30, 1, "RESP3 holds CSD bits 127:96");
    send(&mut h, &mut m, 7, RCA_ARG, CMD_RESP);
    send(&mut h, &mut m, 13, RCA_ARG, CMD_RESP);
    assert_eq!(h.read(reg::RESP0) >> 9 & 0xf, CardState::Tran as u32);
}

/// IDMAC on, interrupts enabled, `bytes` in blocks of 512.
fn arm(h: &mut Sdmmc, bytes: u32, first: u32) {
    h.write(reg::CTRL, CTRL_INT_ENABLE | CTRL_USE_INTERNAL_DMA | 1 << 5);
    h.write(reg::BMOD, BMOD_DE | 1 << 1);
    h.write(reg::IDINTEN, idsts::TI | idsts::RI | idsts::NIS);
    h.write(reg::INTMASK, int::ALL);
    h.write(reg::BYTCNT, bytes);
    h.write(reg::BLKSIZ, 512);
    h.write(reg::DBADDR, first);
    h.write(reg::PLDMND, 1);
}

#[test]
fn the_idmac_walks_chained_descriptors_and_hands_them_back() {
    let (mut h, mut m) = host();
    host_to_tran(&mut h, &mut m);
    let (d0, d1, b0, b1) = (RAM, RAM + 0x40, RAM + 0x1000, RAM + 0x2000);
    m.desc(d0, des0::OWN | des0::CH | des0::FS, 1024, b0, d1);
    m.desc(d1, des0::OWN | des0::CH | des0::LD, 512, b1, 0);
    arm(&mut h, 3 * 512, d0);
    let r = send(&mut h, &mut m, 18, 20, CMD_RESP | CMD_DATA | CMD_AUTO_STOP);
    assert_eq!(r, int::CMD_DONE | int::DATA_OVER | int::ACD, "command, data and the auto-stop");
    let img = image();
    assert_eq!(&m.0[0x1000..0x1400], &img[20 * 512..22 * 512]);
    assert_eq!(&m.0[0x2000..0x2200], &img[22 * 512..23 * 512]);
    assert_eq!((m.word(d0) & des0::OWN, m.word(d1) & des0::OWN), (0, 0), "both descriptors handed back");
    assert_eq!(m.word(d1), des0::CH | des0::LD, "with their other bits kept");
    assert_eq!(h.read(reg::IDSTS), idsts::RI | idsts::NIS, "receive interrupt and its summary");
    assert!(h.irq());
    assert_eq!(h.read(reg::RESP0 + 4) >> 9 & 0xf, CardState::Data as u32, "RESP1: the auto-stop's R1b");
    assert_eq!((h.read(reg::TCBCNT), h.read(reg::TBBCNT)), (1536, 1536));
    assert_eq!(h.read(reg::DSCADDR), d1);
    h.write(reg::IDSTS, idsts::RI | idsts::NIS);
    assert!(!h.irq());
    send(&mut h, &mut m, 13, RCA_ARG, CMD_RESP);
    assert_eq!(h.read(reg::RESP0) >> 9 & 0xf, CardState::Tran as u32);
}

#[test]
fn the_idmac_suspends_on_a_cpu_owned_descriptor_and_resumes_on_poll_demand() {
    let (mut h, mut m) = host();
    host_to_tran(&mut h, &mut m);
    let (d0, d1) = (RAM, RAM + 0x40);
    m.desc(d0, des0::OWN | des0::CH, 512, RAM + 0x1000, d1);
    m.desc(d1, des0::CH | des0::LD, 512, RAM + 0x1200, 0);
    arm(&mut h, 1024, d0);
    h.write(reg::IDINTEN, idsts::TI | idsts::RI | idsts::NIS | idsts::DU | idsts::AIS);
    let r = send(&mut h, &mut m, 18, 0, CMD_RESP | CMD_DATA | CMD_AUTO_STOP);
    assert_eq!(r, int::CMD_DONE, "no data-over yet");
    assert_eq!(h.read(reg::IDSTS), idsts::RI | idsts::NIS | idsts::DU | idsts::AIS, "one block in, then descriptor unavailable");
    assert_eq!(h.read(reg::DSCADDR), d1);
    h.write(reg::IDSTS, idsts::W1C);
    h.write(reg::PLDMND, 1);
    h.run(&mut m);
    assert_eq!(h.read(reg::IDSTS), idsts::DU | idsts::AIS, "a poll demand re-reads the same descriptor");
    h.write(reg::IDSTS, idsts::W1C);
    m.desc(d1, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1200, 0);
    h.write(reg::PLDMND, 1);
    assert!(h.pending());
    h.run(&mut m);
    assert_eq!(h.read(reg::RINTSTS), int::DATA_OVER | int::ACD);
    assert_eq!(h.read(reg::IDSTS), idsts::RI | idsts::NIS);
    assert_eq!(&m.0[0x1000..0x1400], &image()[..1024]);
}

#[test]
fn a_write_moves_memory_to_the_card() {
    let (mut h, mut m) = host();
    host_to_tran(&mut h, &mut m);
    for (i, b) in m.0[0x1000..0x1600].iter_mut().enumerate() { *b = (i % 13) as u8; }
    m.desc(RAM, des0::OWN | des0::CH | des0::LD | des0::DIC, 0x600, RAM + 0x1000, 0);
    arm(&mut h, 0x600, RAM);
    let r = send(&mut h, &mut m, 25, 40, CMD_RESP | CMD_DATA | CMD_WRITE | CMD_AUTO_STOP);
    assert_eq!(r, int::CMD_DONE | int::DATA_OVER | int::ACD);
    assert_eq!(h.read(reg::IDSTS), 0, "DIC: no transmit interrupt for this descriptor");
    assert_eq!(m.word(RAM) & des0::OWN, 0);
    assert_eq!(h.stats.bytes_written, 0x600);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 0x600, RAM + 0x3000, 0);
    arm(&mut h, 0x600, RAM);
    send(&mut h, &mut m, 18, 40, CMD_RESP | CMD_DATA | CMD_AUTO_STOP);
    assert_eq!(m.0[0x3000..0x3600], m.0[0x1000..0x1600], "read back what was written");
}

/// With CARDTHRCTL's busy-clear interrupt on, a write ends with BCI once DAT0 is released; the
/// card's programming time holds STATUS.data_busy until then.
#[test]
fn busy_clear_follows_the_card_after_a_write() {
    let mut h = Sdmmc::new(SdmmcLayout::Esp32p4);
    let c = SdCard::with_config(Box::new(MemStore(image())), SdCardConfig { prg_ns: 1000, ..Default::default() }).unwrap();
    let i = h.attach(Box::new(c));
    h.route[1] = Some(i);
    let mut m = Ram(vec![0; 64 << 10]);
    host_to_tran(&mut h, &mut m);
    h.write(reg::CARDTHRCTL, 1 << 1);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1000, 0);
    arm(&mut h, 512, RAM);
    h.now_ns = 5000;
    let r = send(&mut h, &mut m, 24, 9, CMD_RESP | CMD_DATA | CMD_WRITE);
    assert_eq!(r, int::CMD_DONE | int::DATA_OVER, "the card is still programming: no BCI yet");
    assert_eq!(h.read(reg::STATUS) >> 9 & 1, 1, "DAT0 busy");
    assert_eq!(h.read(reg::STATUS) >> 9 & 1, 1, "looking again changes nothing");
    assert!(!h.take_read_irq());
    h.now_ns = 6000;
    assert_eq!(h.read(reg::STATUS) >> 9 & 1, 0, "released 1 us on");
    assert_eq!(h.read(reg::RINTSTS), int::BCI);
    assert!(h.take_read_irq(), "the read raised the interrupt");
    h.write(reg::RINTSTS, int::BCI);
    h.write(reg::CARDTHRCTL, 0);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1000, 0);
    arm(&mut h, 512, RAM);
    let r = send(&mut h, &mut m, 24, 10, CMD_RESP | CMD_DATA | CMD_WRITE);
    assert_eq!(r, int::CMD_DONE | int::DATA_OVER, "no BCI with the enable off");
    assert_eq!(h.read(reg::STATUS) >> 9 & 1, 1, "DAT0 busy while the card programs");
    h.now_ns = 7000;
    assert_eq!(h.read(reg::STATUS) >> 9 & 1, 0);
    assert_eq!(h.read(reg::RINTSTS), 0);
}

#[test]
fn faults_widths_and_missing_data_end_the_transfer() {
    let (mut h, mut m) = host();
    host_to_tran(&mut h, &mut m);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 512, 0x1000_0000, 0);
    arm(&mut h, 512, RAM);
    h.write(reg::IDINTEN, idsts::W1C);
    send(&mut h, &mut m, 17, 0, CMD_RESP | CMD_DATA);
    assert_eq!(h.read(reg::IDSTS) & (idsts::FBE | idsts::AIS), idsts::FBE | idsts::AIS, "an unreachable buffer is a fatal bus error");
    assert_eq!(h.read(reg::IDSTS) >> idsts::FBE_CODE_SHIFT & 7, 2, "during reception");
    send(&mut h, &mut m, 12, 0, CMD_RESP);
    h.write(reg::IDSTS, idsts::W1C);

    send(&mut h, &mut m, 55, RCA_ARG, CMD_RESP);
    send(&mut h, &mut m, 6, 2, CMD_RESP);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1000, 0);
    arm(&mut h, 512, RAM);
    let r = send(&mut h, &mut m, 17, 0, CMD_RESP | CMD_DATA);
    assert_eq!(r, int::CMD_DONE | int::DCRC | int::DATA_OVER, "the card on four lines, the host on one: CRC errors");
    h.write(reg::CTYPE, 1 << 1);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 1024, RAM + 0x1000, 0);
    arm(&mut h, 1024, RAM);
    let r = send(&mut h, &mut m, 17, 0, CMD_RESP | CMD_DATA);
    assert_eq!(r, int::CMD_DONE | int::DRTO | int::DATA_OVER, "one block asked for two: the second times out");
    assert_eq!(h.read(reg::IDSTS) & idsts::CES, idsts::CES);
}

/// CMD.card_num is five guest-written bits; a slot the host does not have answers nothing.
#[test]
fn a_command_to_a_slot_the_host_lacks_times_out() {
    let (mut h, mut m) = host();
    for slot in [2, 16, 31] {
        h.write(reg::CMD, CMD_START | slot << 16 | CMD_DATA | 17);
        h.run(&mut m);
        let r = h.read(reg::RINTSTS);
        h.write(reg::RINTSTS, r);
        assert_eq!(r, int::RTO | int::CMD_DONE, "slot {slot}");
    }
}

/// A device that answers every command with `resp` and ends every read with `read`.
struct Faulty { resp: Result<SdResponse, SdCmdError>, read: Result<(), SdDataError> }
impl SdDevice for Faulty {
    fn command(&mut self, _now: u64, _cmd: u8, _arg: u32) -> Result<SdResponse, SdCmdError> { self.resp }
    fn read_data(&mut self, _now: u64, buf: &mut [u8]) -> Result<(), SdDataError> { buf.fill(0x5a); self.read }
    fn write_data(&mut self, _now: u64, _data: &[u8]) -> Result<(), SdDataError> { Ok(()) }
    fn name(&self) -> &str { "faulty" }
}

fn faulty(resp: Result<SdResponse, SdCmdError>, read: Result<(), SdDataError>) -> (Sdmmc, Ram) {
    let mut h = Sdmmc::new(SdmmcLayout::Esp32p4);
    let i = h.attach(Box::new(Faulty { resp, read }));
    h.route[1] = Some(i);
    (h, Ram(vec![0; 64 << 10]))
}

/// A response CRC error is RCRC when the command asks for the check, a malformed response
/// RESP_ERR; either keeps the data phase from starting. A read block with a bad CRC is DCRC.
#[test]
fn response_and_data_errors_map_to_their_bits() {
    let (mut h, mut m) = faulty(Err(SdCmdError::Crc(SdResponse::Short(0x900))), Ok(()));
    assert_eq!(send(&mut h, &mut m, 13, 0, CMD_RESP | CMD_RESP_CRC), int::CMD_DONE | int::RCRC);
    assert_eq!(h.read(reg::RESP0), 0x900, "the response as received");
    assert_eq!(send(&mut h, &mut m, 13, 0, CMD_RESP), int::CMD_DONE, "not checked: taken as it is");
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1000, 0);
    arm(&mut h, 512, RAM);
    assert_eq!(send(&mut h, &mut m, 17, 0, CMD_RESP | CMD_RESP_CRC | CMD_DATA), int::CMD_DONE | int::RCRC, "no data phase after a bad response");
    let (mut h, mut m) = faulty(Err(SdCmdError::Malformed(SdResponse::Short(1))), Ok(()));
    assert_eq!(send(&mut h, &mut m, 13, 0, CMD_RESP), int::CMD_DONE | int::RESP_ERR);
    let (mut h, mut m) = faulty(Ok(SdResponse::Short(0x900)), Err(SdDataError::Crc));
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1000, 0);
    arm(&mut h, 512, RAM);
    assert_eq!(send(&mut h, &mut m, 17, 0, CMD_RESP | CMD_DATA), int::CMD_DONE | int::DCRC | int::DATA_OVER, "a read block's CRC error");
    let (mut h, mut m) = host();
    host_to_tran(&mut h, &mut m);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1000, 0);
    arm(&mut h, 512, RAM);
    assert_eq!(send(&mut h, &mut m, 24, 1 << 20, CMD_RESP | CMD_DATA | CMD_WRITE), int::CMD_DONE | int::EBE | int::DATA_OVER, "a write past the end gets no CRC status");
    assert_eq!(h.read(reg::RESP0) & status::OUT_OF_RANGE, status::OUT_OF_RANGE);
}

/// A DMA reset, or a command with STOP_ABORT, ends a transfer the IDMAC has suspended: a poll
/// demand afterwards has nothing to resume.
#[test]
fn a_suspended_transfer_can_be_aborted() {
    for abort in 0..2 {
        let (mut h, mut m) = host();
        host_to_tran(&mut h, &mut m);
        m.desc(RAM, des0::OWN | des0::CH, 512, RAM + 0x1000, RAM + 0x40);
        m.desc(RAM + 0x40, des0::CH | des0::LD, 512, RAM + 0x1200, 0);
        arm(&mut h, 1024, RAM);
        send(&mut h, &mut m, 18, 0, CMD_RESP | CMD_DATA | CMD_AUTO_STOP);
        assert_eq!(h.read(reg::IDSTS) & idsts::DU, idsts::DU);
        if abort == 0 { let ctrl = h.read(reg::CTRL); h.write(reg::CTRL, ctrl | 1 << 2); } else { send(&mut h, &mut m, 12, 0, CMD_RESP | 1 << 14); }
        m.desc(RAM + 0x40, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1200, 0);
        h.write(reg::PLDMND, 1);
        assert!(!h.pending(), "{}: nothing to resume", if abort == 0 { "DMA reset" } else { "STOP_ABORT" });
        assert_eq!(m.word(RAM + 0x40) & des0::OWN, des0::OWN, "the second descriptor was never used");
        send(&mut h, &mut m, 13, RCA_ARG, CMD_RESP);
        let state_after = h.read(reg::RESP0) >> 9 & 0xf;
        assert_eq!(state_after, if abort == 0 { CardState::Data as u32 } else { CardState::Tran as u32 }, "the card stops only on CMD12");
    }
}

/// Descriptors in a list rather than chained: each with two buffers, the next 16 bytes on (no
/// skip), and back to DBADDR after the one marked end of ring.
#[test]
fn ring_descriptors_with_two_buffers() {
    let (mut h, mut m) = host();
    host_to_tran(&mut h, &mut m);
    m.desc(RAM, des0::OWN | des0::FS, 512 | 512 << 13, RAM + 0x1000, RAM + 0x2000);
    m.desc(RAM + 16, des0::OWN | des0::ER | des0::LD, 512, RAM + 0x3000, 0);
    arm(&mut h, 3 * 512, RAM);
    let r = send(&mut h, &mut m, 18, 50, CMD_RESP | CMD_DATA | CMD_AUTO_STOP);
    assert_eq!(r, int::CMD_DONE | int::DATA_OVER | int::ACD);
    let img = image();
    assert_eq!(&m.0[0x1000..0x1200], &img[50 * 512..51 * 512], "buffer 1");
    assert_eq!(&m.0[0x2000..0x2200], &img[51 * 512..52 * 512], "buffer 2 of the same descriptor");
    assert_eq!(&m.0[0x3000..0x3200], &img[52 * 512..53 * 512], "the next descriptor in the list");
    assert_eq!((m.word(RAM) & des0::OWN, m.word(RAM + 16) & des0::OWN), (0, 0));
    assert_eq!(h.read(reg::DSCADDR), RAM + 16);
}

/// TI and RI interrupt only with IDINTEN's normal summary enable, FBE/DU/CES only with the
/// abnormal one.
#[test]
fn idmac_interrupts_need_their_summary_enable() {
    let (mut h, mut m) = host();
    host_to_tran(&mut h, &mut m);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1000, 0);
    arm(&mut h, 512, RAM);
    h.write(reg::INTMASK, 0);
    h.write(reg::IDINTEN, idsts::RI);
    send(&mut h, &mut m, 17, 0, CMD_RESP | CMD_DATA);
    assert_eq!(h.read(reg::IDSTS) & idsts::RI, idsts::RI);
    assert!(!h.irq(), "RI without NI");
    h.write(reg::IDINTEN, idsts::RI | idsts::NIS);
    assert!(h.irq());
    h.write(reg::IDSTS, idsts::W1C);
    m.desc(RAM, des0::CH | des0::LD, 512, RAM + 0x1000, 0);
    arm(&mut h, 512, RAM);
    h.write(reg::INTMASK, 0);
    h.write(reg::IDINTEN, idsts::DU | idsts::NIS);
    send(&mut h, &mut m, 17, 0, CMD_RESP | CMD_DATA);
    assert!(!h.irq(), "DU without AI");
    h.write(reg::IDINTEN, idsts::DU | idsts::AIS);
    assert!(h.irq());
}

/// Write-through survives reopening the file; a copy-on-write erase is a range, not a block
/// per address, and a later write within it reads back.
#[test]
fn file_store_write_through_and_ranged_erase() {
    let path = std::env::temp_dir().join(format!("esp32sim-sd-erase-{}.img", std::process::id()));
    std::fs::write(&path, vec![0x11u8; 1 << 20]).unwrap();
    let p = path.to_str().unwrap();
    FileStore::write_through(p).unwrap().write(5, &[0x44; 512]).unwrap();
    let mut b = [0u8; 512];
    FileStore::copy_on_write(p).unwrap().read(5, &mut b).unwrap();
    assert_eq!(b[0], 0x44, "on the file after reopening");
    let mut cow = FileStore::copy_on_write(p).unwrap();
    cow.write(10, &[0x55; 512]).unwrap();
    cow.erase(0, 2047).unwrap();
    cow.read(5, &mut b).unwrap();
    assert!(b.iter().all(|&x| x == 0), "erased");
    cow.read(10, &mut b).unwrap();
    assert!(b.iter().all(|&x| x == 0), "an earlier write is erased too");
    cow.write(7, &[0x66; 512]).unwrap();
    cow.read(7, &mut b).unwrap();
    assert_eq!(b[0], 0x66, "a write after the erase wins");
    assert_eq!(std::fs::read(&path).unwrap()[5 * 512], 0x44, "the file keeps what it had");
    let mut wt = FileStore::write_through(p).unwrap();
    wt.erase(4, 6).unwrap();
    let f = std::fs::read(&path).unwrap();
    assert!(f[4 * 512..7 * 512].iter().all(|&x| x == 0) && f[3 * 512] == 0x11 && f[7 * 512] == 0x11, "write-through erases just the range");
    std::fs::remove_file(&path).unwrap();
}

/// The board's failure, through the host: a write's DATA_OVER comes before the card has
/// programmed it, so a CMD18 sent meanwhile times out (IDF: ESP_ERR_TIMEOUT) and the CMD13
/// that follows reads 0x400e00, ILLEGAL_COMMAND in prg.
#[test]
fn a_read_during_programming_times_out() {
    let mut h = Sdmmc::new(SdmmcLayout::Esp32p4);
    let c = SdCard::with_config(Box::new(MemStore(image())), SdCardConfig { prg_ns: 2_000_000, ..Default::default() }).unwrap();
    let i = h.attach(Box::new(c));
    h.route[1] = Some(i);
    let mut m = Ram(vec![0; 64 << 10]);
    host_to_tran(&mut h, &mut m);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 1024, RAM + 0x1000, 0);
    arm(&mut h, 1024, RAM);
    h.now_ns = 1_000_000;
    assert_eq!(send(&mut h, &mut m, 25, 64, CMD_RESP | CMD_DATA | CMD_WRITE | CMD_AUTO_STOP), int::CMD_DONE | int::DATA_OVER | int::ACD);
    assert_eq!(h.read(reg::STATUS) >> 9 & 1, 1, "DAT0 busy after the auto-stop");
    h.now_ns = 1_500_000;
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 1024, RAM + 0x1000, 0);
    arm(&mut h, 1024, RAM);
    assert_eq!(send(&mut h, &mut m, 18, 64, CMD_RESP | CMD_DATA | CMD_AUTO_STOP), int::CMD_DONE | int::RTO, "no response, no data phase");
    send(&mut h, &mut m, 13, RCA_ARG, CMD_RESP);
    assert_eq!(h.read(reg::RESP0), 0x0040_0e00);
    h.now_ns = 3_000_000;
    send(&mut h, &mut m, 13, RCA_ARG, CMD_RESP);
    assert_eq!(h.read(reg::RESP0), 0x900, "transfer state, ready for data");
}

/// With the bus timed, CMD_DONE comes when the command and its response are through at the card
/// clock (here 160 MHz / (2 x 2) = 40 MHz on slot 1's divider), DATA_OVER when every block is
/// through at the bus width, the auto-stop's ACD after its own command, and BCI when the card
/// that took a write has programmed it; `advance` raises each at its time.
#[test]
fn a_timed_bus_raises_command_and_data_done_when_the_bits_are_through() {
    let mut h = Sdmmc::new(SdmmcLayout::Esp32p4);
    let c = SdCard::with_config(Box::new(MemStore(image())), SdCardConfig { prg_ns: 50_000, ..Default::default() }).unwrap();
    let i = h.attach(Box::new(c));
    h.route[1] = Some(i);
    let mut m = Ram(vec![0; 64 << 10]);
    host_to_tran(&mut h, &mut m);
    h.timed = true;
    h.ciu_hz = 160_000_000;
    h.write(reg::CLKSRC, 1 << 2);
    h.write(reg::CLKDIV, 2 << 8);
    h.now_ns = 1_000_000;
    h.write(reg::CMDARG, RCA_ARG);
    h.write(reg::CMD, CMD_START | 1 << 16 | CMD_RESP | 13);
    h.run(&mut m);
    assert_eq!(h.read(reg::RINTSTS), 0, "the response is still on the line");
    // 48 command bits, NCR 2, 48 response bits, NCC 8: 106 clocks of 25 ns
    assert_eq!(h.next_event_ns(), Some(1_002_650));
    assert!(!h.advance(1_002_649));
    assert!(h.advance(1_002_650));
    assert_eq!(h.read(reg::RINTSTS), int::CMD_DONE);
    h.write(reg::RINTSTS, int::ALL);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 3 * 512, RAM + 0x1000, 0);
    arm(&mut h, 3 * 512, RAM);
    h.write(reg::CMDARG, 20);
    h.write(reg::CMD, CMD_START | 1 << 16 | CMD_RESP | CMD_DATA | CMD_AUTO_STOP | 18);
    h.run(&mut m);
    assert_eq!(&m.0[0x1000..0x1600], &image()[20 * 512..23 * 512], "the data is in memory at once");
    let cmd_end = 1_002_650 + 2_650;
    // one line: a start bit, 4096 data bits, CRC16, an end bit and NAC per block
    let data_end = cmd_end + 3 * 4116 * 25;
    for (t, want) in [(cmd_end, int::CMD_DONE), (data_end, int::CMD_DONE | int::DATA_OVER), (data_end + 2_650, int::CMD_DONE | int::DATA_OVER | int::ACD)] {
        assert_eq!(h.next_event_ns(), Some(t));
        h.advance(t);
        assert_eq!(h.read(reg::RINTSTS), want);
    }
    assert_eq!(h.next_event_ns(), None);
    h.write(reg::RINTSTS, int::ALL);
    // a write: the card programs from the end of its data on, and BCI follows
    h.write(reg::CARDTHRCTL, 1 << 1);
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 512, RAM + 0x1000, 0);
    arm(&mut h, 512, RAM);
    let t0 = data_end + 2_650;
    h.now_ns = t0;
    h.write(reg::CMDARG, 30);
    h.write(reg::CMD, CMD_START | 1 << 16 | CMD_RESP | CMD_DATA | CMD_WRITE | 24);
    h.run(&mut m);
    let data_end = t0 + 2_650 + (4116 + 7) * 25;
    h.advance(data_end);
    assert_eq!(h.read(reg::RINTSTS), int::CMD_DONE | int::DATA_OVER, "programming: no BCI yet");
    assert_eq!(h.next_event_ns(), Some(data_end + 50_000), "the card's busy end is the next event");
    h.advance(data_end + 50_000);
    assert_eq!(h.read(reg::RINTSTS), int::CMD_DONE | int::DATA_OVER | int::BCI);
}

/// On a timed bus the auto-stop reaches the card after the data phase, so a multi-block write
/// programs from its end: BCI after that, and a read meanwhile is not answered.
#[test]
fn a_timed_multi_block_write_programs_from_its_auto_stop() {
    let mut h = Sdmmc::new(SdmmcLayout::Esp32p4);
    let c = SdCard::with_config(Box::new(MemStore(image())), SdCardConfig { prg_ns: 50_000, ..Default::default() }).unwrap();
    let i = h.attach(Box::new(c));
    h.route[1] = Some(i);
    let mut m = Ram(vec![0; 64 << 10]);
    host_to_tran(&mut h, &mut m);
    (h.timed, h.ciu_hz) = (true, 160_000_000);
    h.write(reg::CLKSRC, 1 << 2);
    h.write(reg::CLKDIV, 2 << 8);
    h.write(reg::CARDTHRCTL, 1 << 1);
    let t0 = 1_000_000;
    h.now_ns = t0;
    m.desc(RAM, des0::OWN | des0::CH | des0::LD, 3 * 512, RAM + 0x1000, 0);
    arm(&mut h, 3 * 512, RAM);
    h.write(reg::CMDARG, 40);
    h.write(reg::CMD, CMD_START | 1 << 16 | CMD_RESP | CMD_DATA | CMD_WRITE | CMD_AUTO_STOP | 25);
    h.run(&mut m);
    // the command, then three blocks with their CRC status on one line
    let data_end = t0 + 2_650 + 3 * (4116 + 7) * 25;
    h.advance(data_end + 2_650);
    assert_eq!(h.read(reg::RINTSTS), int::CMD_DONE | int::DATA_OVER | int::ACD, "programming: no BCI yet");
    assert_eq!(h.next_event_ns(), Some(data_end + 50_000), "programmed from the auto-stop on, not from the command");
    h.write(reg::RINTSTS, int::ALL);
    h.now_ns = data_end + 40_000;
    h.write(reg::CMDARG, 40);
    h.write(reg::CMD, CMD_START | 1 << 16 | CMD_RESP | 17);
    h.run(&mut m);
    h.advance(data_end + 45_000);
    assert_eq!(h.read(reg::RINTSTS) & int::RTO, int::RTO, "a read while the card programs gets no response");
    h.write(reg::RINTSTS, int::ALL);
    h.advance(data_end + 50_000);
    assert_eq!(h.read(reg::RINTSTS), int::BCI);
}

/// An SDIO function whose interrupt the test sets.
struct Function(std::rc::Rc<std::cell::Cell<bool>>);
impl SdDevice for Function {
    fn command(&mut self, _now: u64, _cmd: u8, _arg: u32) -> Result<SdResponse, SdCmdError> { Ok(SdResponse::Short(0)) }
    fn read_data(&mut self, _now: u64, _buf: &mut [u8]) -> Result<(), SdDataError> { Ok(()) }
    fn write_data(&mut self, _now: u64, _data: &[u8]) -> Result<(), SdDataError> { Ok(()) }
    fn interrupt(&self) -> bool { self.0.get() }
    fn io(&self) -> bool { true }
    fn name(&self) -> &str { "function" }
}

/// RINTSTS's SDIO bit for the slot is set when DAT1 goes low, once per assertion: cleared while
/// DAT1 stays low it stays clear (IDF reads the D1 pin for the level, `sdmmc_host_io_int_wait`).
/// A memory card on the other slot never raises it.
#[test]
fn an_sdio_interrupt_is_latched_on_its_falling_edge() {
    let mut h = Sdmmc::new(SdmmcLayout::Esp32p4);
    let line = std::rc::Rc::new(std::cell::Cell::new(false));
    let card = h.attach(Box::new(card()));
    let f = h.attach(Box::new(Function(line.clone())));
    h.route = [Some(card), Some(f)];
    h.write(reg::INTMASK, int::SDIO_SLOT0 << 1);
    h.write(reg::CTRL, CTRL_INT_ENABLE);
    assert!(!h.poll_sdio() && !h.irq());
    line.set(true);
    assert!(h.poll_sdio());
    assert_eq!(h.read(reg::RINTSTS), int::SDIO_SLOT0 << 1, "slot 1's bit, not slot 0's");
    assert!(h.irq());
    h.write(reg::RINTSTS, int::SDIO_SLOT0 << 1);
    assert!(!h.poll_sdio(), "still low: no new edge");
    assert_eq!(h.read(reg::RINTSTS), 0);
    line.set(false);
    h.poll_sdio();
    line.set(true);
    assert!(h.poll_sdio(), "released and asserted again: a new edge");
}
