//! The core as TinyUSB's dcd_dwc2 drives it in buffer DMA mode, against the link, with the
//! driver's register sequences written out by hand.
use super::*;
use emu_core::Fault;

const BASE: u32 = 0x4FF0_0000;
const SETUP_BUF: u32 = BASE + 0x100;
const DESC: u32 = BASE + 0x200;
const BUF: u32 = BASE + 0x1000;
const HZ: u64 = 360_000_000;

struct Mem(Vec<u8>);
impl DmaMemory for Mem {
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Fault> {
        let o = addr.checked_sub(BASE).ok_or(Fault::Unmapped)? as usize;
        buf.copy_from_slice(self.0.get(o..o + buf.len()).ok_or(Fault::Unmapped)?);
        Ok(())
    }
    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), Fault> {
        let o = addr.checked_sub(BASE).ok_or(Fault::Unmapped)? as usize;
        self.0.get_mut(o..o + data.len()).ok_or(Fault::Unmapped)?.copy_from_slice(data);
        Ok(())
    }
}

struct Bench { d: Dwc2, mem: Mem }

fn in_reg(n: u32, r: u32) -> u32 { reg::DIEP0 + reg::EP_STRIDE * n + r }
fn out_reg(n: u32, r: u32) -> u32 { reg::DOEP0 + reg::EP_STRIDE * n + r }

impl Bench {
    fn new() -> Self { Self::with(Dwc2Config::esp32p4_hs(HZ)) }
    fn with(cfg: Dwc2Config) -> Self {
        let mut d = Dwc2::new(cfg);
        d.attach(0, UsbLink::new(UsbTiming::default(), HZ, 0));
        Bench { d, mem: Mem(vec![0; 0x10000]) }
    }
    fn link(&mut self) -> &mut UsbLink { self.d.link.as_mut().unwrap() }
    fn rd(&mut self, off: u32) -> u32 { Device::read(&mut self.d, off) }
    fn wr(&mut self, off: u32, v: u32) { Device::write(&mut self.d, off, v); }
    fn us(&self) -> u64 { self.d.now / (HZ / 1_000_000) }

    /// Let time run `ticks`, stopping at every deadline as the chip's bus would.
    fn advance(&mut self, mut ticks: u64) {
        while ticks > 0 {
            let step = Device::next_deadline(&self.d).unwrap_or(u64::MAX).clamp(1, ticks);
            Device::tick(&mut self.d, step);
            ticks -= step;
            if self.d.pending() { self.d.run(&mut self.mem); }
        }
    }
    fn advance_us(&mut self, us: u64) { self.advance(us * (HZ / 1_000_000)); }
    /// Run in 1 µs steps until `f` holds (the "firmware" polling its registers).
    fn until(&mut self, limit_us: u64, mut f: impl FnMut(&mut Bench) -> bool) -> bool {
        for _ in 0..limit_us { if f(self) { return true; } self.advance_us(1); }
        f(self)
    }
    fn complete(&mut self, limit_us: u64) -> Completion {
        let mut c = None;
        self.until(limit_us, |b| { c = b.link().take_completion(); c.is_some() });
        c.expect("the link finished the request")
    }
    fn submit(&mut self, r: Request) { self.link().submit(r).unwrap(); }

    /// dcd_init: device mode, high speed, DMA, the interrupts, then connect (SDIS cleared).
    fn dcd_init(&mut self) {
        self.wr(reg::GUSBCFG, gusbcfg::FDMOD | 9 << 10);
        self.wr(reg::DCFG, 1 << 2);
        self.wr(reg::GAHBCFG, GAHBCFG_GINT | GAHBCFG_DMAEN);
        self.wr(reg::GINTMSK, gint::OTGINT | gint::USBRST | gint::ENUMDNE | gint::WKUINT | gint::IEPINT | gint::OEPINT | gint::USBSUSP);
        self.wr(reg::DCTL, 0);
    }
    /// handle_bus_reset: EP0 interrupts, EP0 IN's FIFO, control OUT 0 armed for 3 SETUPs.
    fn bus_reset_handler(&mut self) {
        self.wr(reg::DAINTMSK, 1 | 1 << 16);
        self.wr(reg::DOEPMSK, epint::SETUP | epint::XFRC);
        self.wr(reg::DIEPMSK, 1 << 3 | epint::XFRC);
        self.wr(reg::GRXFSIZ, 80);
        self.wr(reg::GNPTXFSIZ, 16 << 16 | 976);
        self.arm_setup();
    }
    fn arm_setup(&mut self) {
        self.wr(out_reg(0, reg::EPTSIZ), 3 << STUPCNT_SHIFT);
        self.wr(out_reg(0, reg::EPDMA), SETUP_BUF);
        let c = self.rd(out_reg(0, reg::EPCTL));
        self.wr(out_reg(0, reg::EPCTL), c | epctl::EPENA | epctl::USBAEP);
    }
    /// Attach, wait for the pull-up, reset; returns the reset's completion.
    fn enumerate(&mut self) -> Completion {
        self.dcd_init();
        self.submit(Request::Attach);
        assert_eq!(self.complete(10).outcome, Outcome::Done(vec![]));
        assert!(self.link().port().connected, "the device pulls D+ up once SDIS is clear");
        self.submit(Request::Reset);
        assert!(self.until(10, |b| b.rd(reg::GINTSTS) & gint::USBRST != 0), "USBRST at the start of SE0");
        self.wr(reg::GINTSTS, gint::USBRST);
        self.bus_reset_handler();
        let c = self.complete(60_000);
        assert!(self.rd(reg::GINTSTS) & gint::ENUMDNE != 0);
        self.wr(reg::GINTSTS, gint::ENUMDNE);
        c
    }
    /// What usbd and dcd_dwc2 do for a control read: on SETUP done read the request at DOEPDMA0-8,
    /// send `reply` from EP0 IN, then take the status OUT and re-arm for SETUPs.
    fn serve_control_in(&mut self, reply: &[u8]) -> [u8; 8] {
        assert!(self.until(2_000, |b| b.rd(out_reg(0, reg::EPINT)) & epint::SETUP != 0), "SETUP phase done");
        assert!(self.d.irq(), "OEPINT through DAINTMSK and GINTMSK");
        let at = self.rd(out_reg(0, reg::EPDMA)) - 8;
        let mut setup = [0u8; 8];
        self.mem.read(at, &mut setup).unwrap();
        let int = self.rd(out_reg(0, reg::EPINT));
        self.wr(out_reg(0, reg::EPINT), int);
        self.mem.write(DESC, reply).unwrap();
        for chunk in 0..reply.len().div_ceil(64).max(1) {
            let n = (reply.len() - chunk * 64).min(64) as u32;
            self.wr(in_reg(0, reg::EPTSIZ), n | 1 << PKTCNT_SHIFT);
            self.wr(in_reg(0, reg::EPDMA), DESC + chunk as u32 * 64);
            let c = self.rd(in_reg(0, reg::EPCTL));
            self.wr(in_reg(0, reg::EPCTL), c | epctl::EPENA | epctl::CNAK);
            assert!(self.until(2_000, |b| b.rd(in_reg(0, reg::EPINT)) & epint::XFRC != 0), "EP0 IN chunk {chunk} sent");
            self.wr(in_reg(0, reg::EPINT), epint::XFRC);
        }
        // status: a zero-length OUT
        self.wr(out_reg(0, reg::EPTSIZ), 1 << PKTCNT_SHIFT);
        self.wr(out_reg(0, reg::EPDMA), 0);
        let c = self.rd(out_reg(0, reg::EPCTL));
        self.wr(out_reg(0, reg::EPCTL), c | epctl::EPENA | epctl::CNAK);
        assert!(self.until(2_000, |b| b.rd(out_reg(0, reg::EPINT)) & epint::XFRC != 0), "status stage taken");
        self.wr(out_reg(0, reg::EPINT), epint::XFRC);
        self.arm_setup();
        setup
    }
    /// A control write as dcd_dwc2 serves one: control OUT 0 armed a packet at a time for the
    /// data stage, then a zero-length IN for the status stage.
    fn serve_control_out(&mut self, len: usize) -> Vec<u8> {
        assert!(self.until(2_000, |b| b.rd(out_reg(0, reg::EPINT)) & epint::SETUP != 0), "SETUP phase done");
        let int = self.rd(out_reg(0, reg::EPINT));
        self.wr(out_reg(0, reg::EPINT), int);
        for chunk in 0..len.div_ceil(64) {
            let n = (len - chunk * 64).min(64) as u32;
            self.wr(out_reg(0, reg::EPTSIZ), n | 1 << PKTCNT_SHIFT);
            self.wr(out_reg(0, reg::EPDMA), BUF + chunk as u32 * 64);
            let c = self.rd(out_reg(0, reg::EPCTL));
            self.wr(out_reg(0, reg::EPCTL), c | epctl::EPENA | epctl::CNAK);
            assert!(self.until(2_000, |b| b.rd(out_reg(0, reg::EPINT)) & epint::XFRC != 0), "data packet {chunk} taken");
            self.wr(out_reg(0, reg::EPINT), epint::XFRC);
        }
        self.wr(in_reg(0, reg::EPTSIZ), 1 << PKTCNT_SHIFT);
        let c = self.rd(in_reg(0, reg::EPCTL));
        self.wr(in_reg(0, reg::EPCTL), c | epctl::EPENA | epctl::CNAK);
        assert!(self.until(2_000, |b| b.rd(in_reg(0, reg::EPINT)) & epint::XFRC != 0), "status IN sent");
        self.wr(in_reg(0, reg::EPINT), epint::XFRC);
        self.arm_setup();
        self.mem.0[0x1000..0x1000 + len].to_vec()
    }
    /// edpt_activate for bulk endpoint 1 both ways, 512-byte packets, and its TX FIFO.
    fn open_bulk1(&mut self) {
        self.wr(reg::DIEPTXF1, 128 << 16 | 848);
        self.wr(in_reg(1, reg::EPCTL), 512 | epctl::USBAEP | 2 << 18 | 1 << 22 | epctl::SD0PID);
        self.wr(out_reg(1, reg::EPCTL), 512 | epctl::USBAEP | 2 << 18 | epctl::SD0PID);
        let m = self.rd(reg::DAINTMSK);
        self.wr(reg::DAINTMSK, m | 2 | 2 << 16);
    }
    fn arm(&mut self, out: bool, n: u32, addr: u32, len: u32) {
        let r = if out { out_reg } else { in_reg };
        let pkts = len.div_ceil(512).max(1);
        self.wr(r(n, reg::EPTSIZ), len | pkts << PKTCNT_SHIFT);
        self.wr(r(n, reg::EPDMA), addr);
        let c = self.rd(r(n, reg::EPCTL));
        self.wr(r(n, reg::EPCTL), c | epctl::EPENA | epctl::CNAK);
    }
}

fn get_descriptor(kind: u8, len: u16) -> [u8; 8] { let l = len.to_le_bytes(); [0x80, 6, 0, kind, 0, 0, l[0], l[1]] }

#[test]
fn identity_and_core_soft_reset_from_4_20a() {
    let mut b = Bench::new();
    assert_eq!(b.rd(reg::GSNPSID) >> 16, 0x4F54, "TinyUSB's check_dwc2 looks for the OTG ID");
    assert_eq!(b.rd(reg::GHWCFG1 + 4) >> 3 & 3, 2, "GHWCFG2: internal DMA");
    assert_eq!(b.rd(reg::GHWCFG1 + 4) >> 6 & 3, 3, "UTMI+ HS PHY");
    assert_eq!(b.rd(reg::GHWCFG1 + 4) >> 10 & 15, 15, "16 device endpoints");
    assert_eq!(b.rd(reg::GHWCFG4) >> 14 & 3, 2, "8/16-bit PHY: TinyUSB picks 16 bits");
    assert_ne!(b.rd(reg::GRSTCTL) & grstctl::AHBIDL, 0);
    b.wr(reg::GINTMSK, 0xffff);
    b.wr(reg::GUSBCFG, gusbcfg::PHYIF16 | 9 << 10 | gusbcfg::FDMOD);
    let g = b.rd(reg::GRSTCTL);
    b.wr(reg::GRSTCTL, g | grstctl::CSRST);
    // dwc2_common.c reset_core, v4.20a and later: wait for CSRSTDONE, then clear CSRST and write 1 to DONE
    let g = b.rd(reg::GRSTCTL);
    assert_eq!(g & (grstctl::CSRST | grstctl::CSRSTDONE | grstctl::AHBIDL), grstctl::CSRST | grstctl::CSRSTDONE | grstctl::AHBIDL);
    b.wr(reg::GRSTCTL, (g & !grstctl::CSRST) | grstctl::CSRSTDONE);
    assert_eq!(b.rd(reg::GRSTCTL), grstctl::AHBIDL);
    assert_eq!(b.rd(reg::GINTMSK), 0, "the soft reset clears the interrupt masks");
    assert_eq!(b.rd(reg::GUSBCFG), gusbcfg::PHYIF16 | 9 << 10, "keeps the PHY selection and turnaround, drops the forced mode");
    assert_ne!(b.rd(reg::DCTL) & dctl::SDIS, 0, "and the soft disconnect");
    // flushes clear at once; TxFNum reads back
    b.wr(reg::GRSTCTL, grstctl::TXFFLSH | 0x10 << 6);
    assert_eq!(b.rd(reg::GRSTCTL) & (grstctl::TXFFLSH | grstctl::TXFNUM), 0x10 << 6);
    b.wr(reg::GRSTCTL, grstctl::RXFFLSH);
    assert_eq!(b.rd(reg::GRSTCTL) & grstctl::RXFFLSH, 0);
    // before 4.20a CSRST clears itself
    let mut old = Bench::with(Dwc2Config { gsnpsid: 0x4F54_400A, ..Dwc2Config::esp32p4_hs(HZ) });
    old.wr(reg::GRSTCTL, grstctl::CSRST);
    assert_eq!(old.rd(reg::GRSTCTL), grstctl::AHBIDL);
}

#[test]
fn attach_reset_and_enumeration_done_at_high_speed() {
    let mut b = Bench::new();
    b.submit(Request::Attach);
    b.complete(10);
    assert!(!b.link().port().connected, "SDIS resets to 1: no pull-up before the driver connects");
    assert_ne!(b.rd(reg::GOTGCTL) & gotgctl::BSESVLD, 0, "B-session valid from VBUS");
    assert_ne!(b.rd(reg::GINTSTS) & gint::SRQINT, 0, "new session detected");
    let t0 = b.us();
    let c = b.enumerate();
    assert_eq!(c.outcome, Outcome::Done(vec![Speed::High as u8]));
    assert!((50_000..50_010).contains(&(b.us() - t0)), "SE0 for 50 ms (TDRSTR), took {} us", b.us() - t0);
    assert_eq!(b.rd(reg::DSTS) >> 1 & 3, 0, "ENUMSPD high speed");
    // SOFs every microframe, counted in DSTS.SOFFN
    b.wr(reg::GINTSTS, gint::SOF);
    let f0 = b.rd(reg::DSTS) >> 8 & 0x3fff;
    b.advance_us(126);
    assert_ne!(b.rd(reg::GINTSTS) & gint::SOF, 0);
    assert_eq!((b.rd(reg::DSTS) >> 8 & 0x3fff) - f0, 1);
    b.advance_us(1000);
    assert_eq!((b.rd(reg::DSTS) >> 8 & 0x3fff) - f0, 9, "a microframe each 125 us");
    // the SOF interrupt wakes the core at the microframe, as the deadline says
    b.wr(reg::GINTSTS, gint::SOF);
    b.wr(reg::GINTMSK, gint::SOF);
    assert!(!b.d.irq());
    let dl = Device::next_deadline(&b.d).unwrap();
    assert!(dl <= 125 * 360);
    b.advance(dl);
    assert!(b.d.irq());
}

#[test]
fn control_read_with_the_driver_answering_from_dma() {
    let mut b = Bench::new();
    b.enumerate();
    let mut desc = [0u8; 18];
    desc[0] = 18; desc[1] = 1; desc[7] = 64;
    b.submit(Request::Control { setup: get_descriptor(1, 64), data: vec![], mps: 64 });
    let setup = b.serve_control_in(&desc);
    assert_eq!(setup, get_descriptor(1, 64), "the SETUP the core wrote at DOEPDMA0 - 8");
    let c = b.complete(1000);
    assert_eq!(c.outcome, Outcome::Done(desc.to_vec()), "a short data stage ends the control read");
    assert!(c.naks >= 1, "the IN after the SETUP waited for the driver ({} NAKs)", c.naks);
    // 130 bytes: three IN packets, the last short
    let long: Vec<u8> = (0..130u8).collect();
    b.submit(Request::Control { setup: get_descriptor(2, 255), data: vec![], mps: 64 });
    b.serve_control_in(&long);
    assert_eq!(b.complete(1000).outcome, Outcome::Done(long));
    assert_eq!(b.d.stats.setups, 2);
}

#[test]
fn back_to_back_setups_leave_the_last_at_doepdma_minus_8() {
    let mut b = Bench::new();
    b.enumerate();
    let (first, second) = (get_descriptor(1, 18), [0, 9, 1, 0, 0, 0, 0, 0]);
    UsbFunction::setup(&mut b.d, &mut b.mem, first);
    UsbFunction::setup(&mut b.d, &mut b.mem, second);
    assert_eq!(b.rd(out_reg(0, reg::EPINT)) & epint::STPKTRX, epint::STPKTRX, "Setup Packet Received");
    assert_eq!(b.rd(out_reg(0, reg::EPINT)) & epint::SETUP, 0, "no SETUP done before a token follows");
    assert_eq!(b.rd(out_reg(0, reg::EPTSIZ)) >> STUPCNT_SHIFT & 3, 1);
    assert_eq!(b.rd(out_reg(0, reg::EPDMA)), SETUP_BUF + 16);
    // the status IN of the no-data request ends the setup phase and NAKs: EP0 IN is not armed
    assert_eq!(UsbFunction::token_in(&mut b.d, &mut b.mem, 0, 64), InReply::Nak);
    assert_ne!(b.rd(out_reg(0, reg::EPINT)) & epint::SETUP, 0);
    assert_eq!(b.rd(out_reg(0, reg::EPCTL)) & epctl::EPENA, 0, "the core gives control OUT 0 back");
    let (mut last, at) = ([0u8; 8], b.rd(out_reg(0, reg::EPDMA)) - 8);
    b.mem.read(at, &mut last).unwrap();
    assert_eq!(last, second);
    assert_ne!(b.rd(in_reg(0, reg::EPCTL)) & epctl::NAKSTS, 0, "a SETUP sets NAK on both control endpoints");
    // a fourth SETUP past STUPCNT rewrites the last slot
    b.arm_setup();
    b.wr(out_reg(0, reg::EPTSIZ), 1 << STUPCNT_SHIFT);
    UsbFunction::setup(&mut b.d, &mut b.mem, first);
    UsbFunction::setup(&mut b.d, &mut b.mem, second);
    assert_ne!(b.rd(out_reg(0, reg::EPINT)) & epint::B2BSTUP, 0);
    assert_eq!(b.rd(out_reg(0, reg::EPDMA)), SETUP_BUF + 8);
}

#[test]
fn bulk_out_multi_packet_and_short_through_dma() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    let data: Vec<u8> = (0..1300).map(|i| (i * 7) as u8).collect();
    b.arm(true, 1, BUF, 2048);
    let t0 = b.us();
    b.submit(Request::BulkOut { ep: 1, data: data.clone(), mps: 512, zlp: false });
    let c = b.complete(1000);
    assert_eq!((c.outcome, c.packets, c.naks), (Outcome::Done(vec![]), 3, 0));
    assert!(b.us() - t0 < 40, "three high-speed packets back to back");
    assert_eq!(&b.mem.0[0x1000..0x1000 + 1300], &data[..]);
    let int = b.rd(out_reg(1, reg::EPINT));
    assert_ne!(int & epint::XFRC, 0, "the short packet ends the transfer");
    assert_eq!(b.rd(out_reg(1, reg::EPTSIZ)) & 0x1ffff, 748, "XferSize counts down by what came in");
    assert_eq!(b.rd(out_reg(1, reg::EPDMA)), BUF + 1300);
    assert_eq!(b.rd(out_reg(1, reg::EPCTL)) & (epctl::EPENA | epctl::NAKSTS), epctl::NAKSTS);
    assert!(b.d.irq() && b.rd(reg::DAINT) & 1 << 17 != 0);
    // an exact multiple of the packet size ends on the packet count
    b.wr(out_reg(1, reg::EPINT), int);
    b.arm(true, 1, BUF, 1024);
    b.submit(Request::BulkOut { ep: 1, data: vec![0x5a; 1024], mps: 512, zlp: false });
    assert_eq!(b.complete(1000).outcome, Outcome::Done(vec![]));
    assert_eq!(b.rd(out_reg(1, reg::EPTSIZ)) & 0x1ffff, 0);
}

#[test]
fn bulk_in_until_a_short_packet_and_toggles() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    let data: Vec<u8> = (0..1100).map(|i| (i * 13) as u8).collect();
    b.mem.0[0x1000..0x1000 + 1100].copy_from_slice(&data);
    b.arm(false, 1, BUF, 1100);
    b.submit(Request::BulkIn { ep: 1, len: 4096, mps: 512 });
    let c = b.complete(1000);
    assert_eq!((c.outcome, c.packets), (Outcome::Done(data), 3));
    assert_ne!(b.rd(in_reg(1, reg::EPINT)) & epint::XFRC, 0);
    assert_eq!(b.rd(in_reg(1, reg::EPCTL)) & epctl::DPID, epctl::DPID, "DATA0, DATA1, DATA0 sent: DATA1 next");
    // a zero-length packet
    b.wr(in_reg(1, reg::EPINT), 0xff);
    b.arm(false, 1, BUF, 0);
    b.submit(Request::BulkIn { ep: 1, len: 512, mps: 512 });
    assert_eq!(b.complete(1000).outcome, Outcome::Done(vec![]));
    // the device more than the host asked for
    b.arm(false, 1, BUF, 512);
    b.submit(Request::BulkIn { ep: 1, len: 13, mps: 512 });
    assert!(matches!(b.complete(1000).outcome, Outcome::Babble(_)));
}

#[test]
fn nak_until_armed_then_stall() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    b.submit(Request::BulkIn { ep: 1, len: 512, mps: 512 });
    b.advance_us(1000);
    assert!(b.link().busy(), "an unarmed endpoint NAKs");
    let naks = b.d.stats.naks;
    assert!((8..=10).contains(&naks), "retried once a microframe: {naks} NAKs in 1 ms");
    assert_ne!(b.rd(in_reg(1, reg::EPINT)) & epint::NAK, 0);
    b.mem.0[0x1000..0x1004].copy_from_slice(b"USBS");
    b.arm(false, 1, BUF, 4);
    let c = b.complete(200);
    assert_eq!(c.outcome, Outcome::Done(b"USBS".to_vec()));
    // STALL on IN and OUT until cleared, and a SETUP clears a control endpoint's STALL
    let c = b.rd(in_reg(1, reg::EPCTL));
    b.wr(in_reg(1, reg::EPCTL), c | epctl::STALL);
    b.submit(Request::BulkIn { ep: 1, len: 512, mps: 512 });
    assert_eq!(b.complete(100).outcome, Outcome::Stall(vec![]));
    let c = b.rd(out_reg(1, reg::EPCTL));
    b.wr(out_reg(1, reg::EPCTL), c | epctl::STALL);
    b.submit(Request::BulkOut { ep: 1, data: vec![1; 31], mps: 512, zlp: false });
    assert_eq!(b.complete(100).outcome, Outcome::Stall(vec![]));
    let c = b.rd(out_reg(1, reg::EPCTL));
    b.wr(out_reg(1, reg::EPCTL), (c & !epctl::STALL) | epctl::SD0PID);
    b.link().reset_toggle(0x01);
    b.arm(true, 1, BUF, 31);
    b.submit(Request::BulkOut { ep: 1, data: vec![1; 31], mps: 512, zlp: false });
    assert_eq!(b.complete(100).outcome, Outcome::Done(vec![]));
    let c = b.rd(in_reg(0, reg::EPCTL));
    b.wr(in_reg(0, reg::EPCTL), c | epctl::STALL);
    b.submit(Request::Control { setup: get_descriptor(6, 10), data: vec![], mps: 64 });
    b.until(2_000, |b| b.rd(out_reg(0, reg::EPINT)) & epint::SETUP != 0);
    assert_eq!(b.rd(in_reg(0, reg::EPCTL)) & epctl::STALL, 0, "the SETUP cleared the STALL");
    let c = b.rd(in_reg(0, reg::EPCTL));
    b.wr(in_reg(0, reg::EPCTL), c | epctl::STALL);
    assert_eq!(b.complete(1000).outcome, Outcome::Stall(vec![]), "a request error: the data stage STALLs");
}

#[test]
fn an_out_packet_with_the_wrong_data_pid_is_acknowledged_and_dropped() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    b.arm(true, 1, BUF, 512);
    assert_eq!(UsbFunction::token_out(&mut b.d, &mut b.mem, 1, true, &[9; 31]), Handshake::Ack);
    assert_eq!(b.d.stats.toggle_discards, 1);
    assert_eq!(b.rd(out_reg(1, reg::EPTSIZ)) & 0x1ffff, 512, "nothing taken");
    assert_eq!(UsbFunction::token_out(&mut b.d, &mut b.mem, 1, false, &[9; 31]), Handshake::Ack);
    assert_eq!(b.rd(out_reg(1, reg::EPTSIZ)) & 0x1ffff, 481);
}

#[test]
fn global_out_nak_and_endpoint_disable_as_edpt_disable_waits_for_them() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    b.arm(false, 1, BUF, 512);
    let c = b.rd(in_reg(1, reg::EPCTL));
    b.wr(in_reg(1, reg::EPCTL), c | epctl::SNAK);
    assert_ne!(b.rd(in_reg(1, reg::EPINT)) & epint::INEPNE, 0, "IN NAK effective");
    let c = b.rd(in_reg(1, reg::EPCTL));
    b.wr(in_reg(1, reg::EPCTL), c | epctl::EPDIS);
    assert_ne!(b.rd(in_reg(1, reg::EPINT)) & epint::EPDISD, 0);
    assert_eq!(b.rd(in_reg(1, reg::EPCTL)) & (epctl::EPENA | epctl::EPDIS), 0);
    b.arm(true, 1, BUF, 512);
    b.wr(reg::DCTL, dctl::SGONAK);
    assert_ne!(b.rd(reg::GINTSTS) & gint::BOUTNAKEFF, 0);
    assert_eq!(UsbFunction::token_out(&mut b.d, &mut b.mem, 1, false, &[1; 8]), Handshake::Nak);
    let c = b.rd(out_reg(1, reg::EPCTL));
    b.wr(out_reg(1, reg::EPCTL), c | epctl::EPDIS);
    assert_ne!(b.rd(out_reg(1, reg::EPINT)) & epint::EPDISD, 0);
    b.wr(reg::DCTL, dctl::CGONAK);
    assert_eq!(b.rd(reg::GINTSTS) & gint::BOUTNAKEFF, 0);
}

#[test]
fn usb_reset_mid_session_clears_active_endpoints() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    assert_ne!(b.rd(in_reg(1, reg::EPCTL)) & epctl::USBAEP, 0);
    b.submit(Request::Reset);
    assert!(b.until(10, |b| b.rd(reg::GINTSTS) & gint::USBRST != 0));
    assert_eq!(b.rd(in_reg(1, reg::EPCTL)) & epctl::USBAEP, 0, "USBActEP cleared by the reset");
    assert_ne!(b.rd(in_reg(0, reg::EPCTL)) & epctl::USBAEP, 0, "but not EP0's");
    b.wr(reg::GINTSTS, gint::USBRST);
    b.bus_reset_handler();
    assert_eq!(b.complete(60_000).outcome, Outcome::Done(vec![Speed::High as u8]));
}

#[test]
fn suspend_after_3_ms_and_resume() {
    let mut b = Bench::new();
    b.enumerate();
    b.submit(Request::Suspend);
    b.complete(10);
    let t0 = b.us();
    assert!(b.until(5_000, |b| b.rd(reg::GINTSTS) & gint::ESUSP != 0));
    assert_eq!(b.us() - t0, 3_000, "early suspend after 3 ms of idle");
    assert!(b.until(500, |b| b.rd(reg::GINTSTS) & gint::USBSUSP != 0));
    assert_eq!(b.us() - t0, 3_125);
    assert_eq!(b.rd(reg::DSTS) & 1, 1, "SUSPSTS");
    assert!(b.d.irq(), "USBSUSP unmasked");
    let f = b.rd(reg::DSTS) >> 8;
    b.advance_us(500);
    assert_eq!(b.rd(reg::DSTS) >> 8, f, "no SOFs while suspended");
    b.submit(Request::BulkIn { ep: 1, len: 64, mps: 512 });
    assert_eq!(b.complete(10).outcome, Outcome::NotRunning);
    b.wr(reg::GINTSTS, gint::USBSUSP | gint::ESUSP);
    b.submit(Request::Resume);
    assert!(b.until(10, |b| b.rd(reg::GINTSTS) & gint::WKUINT != 0), "resume detected at once");
    assert_eq!(b.rd(reg::DSTS) & 1, 0);
    let t1 = b.us();
    b.complete(30_000);
    assert!((20_000..20_010).contains(&(b.us() - t1)), "K for 20 ms (TDRSMDN)");
    b.advance_us(130);
    assert_ne!(b.rd(reg::DSTS) >> 8, f, "SOFs again");
    // remote wakeup while suspended shows on the port
    b.submit(Request::Suspend);
    b.complete(10);
    b.wr(reg::DCTL, dctl::RWUSIG);
    assert!(b.link().port().remote_wakeup);
}

#[test]
fn soft_disconnect_and_vbus_loss() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    b.submit(Request::BulkIn { ep: 1, len: 512, mps: 512 });
    b.advance_us(300);
    b.wr(reg::DCTL, dctl::SDIS);
    let c = b.complete(10);
    assert_eq!(c.outcome, Outcome::Disconnected, "the port sees the pull-up go");
    let p = b.link().port();
    assert!(!p.connected && !p.enabled);
    b.submit(Request::Reset);
    assert_eq!(b.complete(10).outcome, Outcome::Disconnected);
    b.wr(reg::DCTL, 0);
    assert!(b.link().port().connected, "connected again");
    b.submit(Request::Detach);
    b.complete(10);
    assert_ne!(b.rd(reg::GOTGINT) & GOTGINT_SEDET, 0, "session end");
    assert_ne!(b.rd(reg::GINTSTS) & gint::OTGINT, 0);
    assert_eq!(b.rd(reg::GOTGCTL) & gotgctl::BSESVLD, 0);
    assert!(b.d.irq(), "OTGINT unmasked: TinyUSB reports the unplug");
    b.wr(reg::GOTGINT, GOTGINT_SEDET);
    assert_eq!(b.rd(reg::GINTSTS) & gint::OTGINT, 0);
}

#[test]
fn a_tx_fifo_smaller_than_the_packet_naks() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    b.wr(reg::DIEPTXF1, 16 << 16 | 848);
    b.arm(false, 1, BUF, 512);
    assert_eq!(b.d.stats.fifo_errors, 1);
    assert_eq!(UsbFunction::token_in(&mut b.d, &mut b.mem, 1, 512), InReply::Nak);
    assert_eq!(b.rd(in_reg(1, reg::DTXFSTS)), 16);
}

/// The HS core has 16 OUT endpoints but 8 IN (OTG20_NUM_IN_EPS) and DIEPTXF1-7: IN endpoint 8
/// and DIEPTXF8 read 0 and drop writes, OUT endpoint 8 does not.
#[test]
fn in_endpoints_past_the_eighth_are_absent() {
    let mut b = Bench::new();
    b.enumerate();
    for (off, v) in [(in_reg(7, reg::EPDMA), 0x1234), (in_reg(8, reg::EPDMA), 0x1234), (out_reg(8, reg::EPDMA), 0x1234), (reg::DIEPTXF1 + 4 * 6, 64 << 16 | 900), (reg::DIEPTXF1 + 4 * 7, 64 << 16 | 900)] { b.wr(off, v); }
    assert_eq!((b.rd(in_reg(7, reg::EPDMA)), b.rd(in_reg(8, reg::EPDMA)), b.rd(out_reg(8, reg::EPDMA))), (0x1234, 0, 0x1234));
    assert_eq!((b.rd(reg::DIEPTXF1 + 4 * 6), b.rd(reg::DIEPTXF1 + 4 * 7)), (64 << 16 | 900, 0), "DIEPTXF7, DIEPTXF8");
    assert_eq!(UsbFunction::token_in(&mut b.d, &mut b.mem, 8, 512), InReply::Nak);
}

#[test]
fn dma_fault_is_an_ahb_error() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    b.arm(false, 1, 0x1000, 64);
    assert_eq!(UsbFunction::token_in(&mut b.d, &mut b.mem, 1, 512), InReply::Nak);
    assert_ne!(b.rd(in_reg(1, reg::EPINT)) & epint::AHBERR, 0);
    assert_eq!(b.d.stats.dma_faults, 1);
}

#[test]
fn a_rate_capped_host_spreads_packets_over_microframes() {
    let mut b = Bench::new();
    b.link().timing = UsbTiming::default().with_rate(8_192_000);
    b.enumerate();
    b.open_bulk1();
    b.arm(true, 1, BUF, 8192);
    let t0 = b.us();
    b.submit(Request::BulkOut { ep: 1, data: vec![3; 8192], mps: 512, zlp: false });
    b.complete(10_000);
    let took = b.us() - t0;
    assert!((875..=1000).contains(&took), "two 512-byte packets a microframe: 16 packets in 8 microframes, took {took} us");
    let mut fast = Bench::new();
    fast.enumerate();
    fast.open_bulk1();
    fast.arm(true, 1, BUF, 8192);
    let t0 = fast.us();
    fast.submit(Request::BulkOut { ep: 1, data: vec![3; 8192], mps: 512, zlp: false });
    fast.complete(10_000);
    let took = fast.us() - t0;
    assert!((140..=260).contains(&took), "line rate: 13 packets in the first microframe, 3 in the next, took {took} us");
}

/// A control write's status stage is a DATA1 zero-length IN whatever the data stage's packets
/// left the toggle at (USB 2.0 §8.5.3): one packet and two.
#[test]
fn control_writes_of_one_and_two_packets_end_in_a_data1_status() {
    let mut b = Bench::new();
    b.enumerate();
    for len in [7usize, 70] {
        let data: Vec<u8> = (0..len as u8).map(|i| i ^ 0x5a).collect();
        let l = (len as u16).to_le_bytes();
        b.submit(Request::Control { setup: [0x21, 0x09, 0, 2, 0, 0, l[0], l[1]], data: data.clone(), mps: 64 });
        assert_eq!(b.serve_control_out(len), data, "the data stage landed by DMA");
        let c = b.complete(2_000);
        assert_eq!(c.outcome, Outcome::Done(vec![]), "{len} bytes");
        assert!(c.naks < 8, "{len} bytes: {} NAKs", c.naks);
    }
    assert_eq!(b.link().stats.toggle_errors, 0);
}

/// EPDIS completes only once the endpoint NAKs: an IN endpoint after SNAK (INEPNE), an OUT
/// endpoint under the global OUT NAK (BOUTNAKEFF); until then it stays set and the endpoint
/// enabled.
#[test]
fn an_endpoint_disable_waits_for_its_nak() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    b.arm(false, 1, BUF, 512);
    let c = b.rd(in_reg(1, reg::EPCTL));
    b.wr(in_reg(1, reg::EPCTL), c | epctl::EPDIS);
    assert_eq!(b.rd(in_reg(1, reg::EPCTL)) & (epctl::EPENA | epctl::EPDIS), epctl::EPENA | epctl::EPDIS, "not NAKing: pending");
    assert_eq!(b.rd(in_reg(1, reg::EPINT)) & epint::EPDISD, 0);
    let c = b.rd(in_reg(1, reg::EPCTL));
    b.wr(in_reg(1, reg::EPCTL), c | epctl::SNAK);
    assert_eq!(b.rd(in_reg(1, reg::EPCTL)) & (epctl::EPENA | epctl::EPDIS), 0, "SNAK lets it complete");
    assert_ne!(b.rd(in_reg(1, reg::EPINT)) & epint::EPDISD, 0);
    b.arm(true, 1, BUF, 512);
    let c = b.rd(out_reg(1, reg::EPCTL));
    b.wr(out_reg(1, reg::EPCTL), c | epctl::SNAK | epctl::EPDIS);
    assert_ne!(b.rd(out_reg(1, reg::EPCTL)) & epctl::EPDIS, 0, "an OUT endpoint needs the global OUT NAK");
    b.wr(reg::DCTL, dctl::SGONAK);
    assert_eq!(b.rd(out_reg(1, reg::EPCTL)) & (epctl::EPENA | epctl::EPDIS), 0);
    assert_ne!(b.rd(out_reg(1, reg::EPINT)) & epint::EPDISD, 0);
}

/// The block's reset line: asserting it resets the registers, writes are ignored while it is
/// held, and releasing it leaves the core at its reset values.
#[test]
fn the_reset_line_holds_the_core() {
    let mut b = Bench::new();
    b.dcd_init();
    assert_ne!(b.rd(reg::GAHBCFG), 0);
    b.d.set_reset(true);
    assert_eq!(b.rd(reg::GAHBCFG), 0);
    b.wr(reg::GAHBCFG, GAHBCFG_GINT);
    assert_eq!(b.rd(reg::GAHBCFG), 0, "ignored while held");
    b.d.set_reset(true);
    b.d.set_reset(false);
    b.wr(reg::GAHBCFG, GAHBCFG_GINT);
    assert_eq!(b.rd(reg::GAHBCFG), GAHBCFG_GINT);
}

/// A VBUS loss injected at a set time ends the transfer in flight and is a session end for the
/// core, whatever the host was doing.
#[test]
fn an_injected_vbus_loss_ends_a_transfer_in_flight() {
    let mut b = Bench::new();
    b.enumerate();
    b.open_bulk1();
    b.submit(Request::BulkIn { ep: 1, len: 512, mps: 512 });
    let now = b.d.now;
    let at = b.link().ticks_to_ns(now) + 300_000;
    b.link().inject(at, LinkFault::VbusOff);
    let c = b.complete(1000);
    assert_eq!(c.outcome, Outcome::Disconnected);
    let now = b.d.now;
    assert!(b.link().ticks_to_ns(now) >= at);
    let p = b.link().port();
    assert!(!p.power && !p.connected);
    assert_ne!(b.rd(reg::GOTGINT) & GOTGINT_SEDET, 0, "session end");
    assert_eq!(b.rd(reg::GOTGCTL) & gotgctl::BSESVLD, 0);
}
