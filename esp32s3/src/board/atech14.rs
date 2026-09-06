//! The Atech 14-port motherboard with its modules:
//!   - ST7735 160×80 TFT on bit-banged SPI: SCLK 2, CS 41, MOSI 1, DC 40 (the firmware also drives
//!     it from SPI2 when it can)
//!   - Knob V1.1: WS2812 12-LED ring on GPIO 8 (via RMT), rotary encoder CLK 5 / DT 4 / SW 9
//!   - Light Grid V1.1 ×2: 3×3 WS2812 on port 7 (GPIO 6) and port 11 (GPIO 43), via RMT
//!   - buttons GPIO 17 / 16 (active low)
//!   - MAX98357A I2S amp: BCLK 12, LRCLK 13, DIN 10
use esp_soc::board::BoardModel;
use esp_soc::devices::{DcsPanel, SpiBitBang, Ws2812Chain};

pub const PIN_TFT_SCLK: u8 = 2;
pub const PIN_TFT_CS: u8 = 41;
pub const PIN_TFT_MOSI: u8 = 1;
pub const PIN_TFT_DC: u8 = 40;
pub const PIN_RING: u8 = 8;
/// Light Grid V1.1 (NeoPixel 3x3) data lines: port 7 top-middle, port 11 right column. Each module
/// has a Line A (WS2812B, RGB) and a Line B (SK6812, RGBW) and the driver writes both; these are
/// the RGB revision, so only Line A carries a strip we can decode.
pub const PIN_GRID7_A: u8 = 6;
pub const PIN_GRID11_A: u8 = 43;
pub const PIN_ENC_CLK: u8 = 5;
pub const PIN_ENC_DT: u8 = 4;
pub const PIN_ENC_SW: u8 = 9;
pub const PIN_BTN1: u8 = 17;
pub const PIN_BTN2: u8 = 16;

/// Where chain index `i` of the Knob V1.1's ring sits physically, as a position counted
/// counter-clockwise from 12 o'clock. The ring is wired for compatibility with the 6-LED knob:
/// chain indices 0–5 are the six original positions, 6–11 the in-between ones (firmware:
/// `rotary_encoder.h`).
pub const KNOB_RING_PHYSICAL: [usize; 12] = [0, 2, 4, 6, 8, 10, 11, 1, 3, 5, 7, 9];

/// Where chain index `i` of the Light Grid V1.1 in **port 7** sits on the glass, as a row-major
/// cell of the 3×3 the page draws, with the motherboard upright (USB-C at the bottom). The
/// chain is column-serpentine: down the left column (0, 1, 2), up the middle (3 at the bottom,
/// 5 at the top), down the right (6, 7, 8). Measured on the board one chain LED at a time with
/// the `grid-selftest` firmware — chain 0 top-left, 1 middle-left, 3 bottom-centre pin it; the
/// rest follow from the serpentine.
pub const GRID_PHYSICAL_PORT7: [usize; 9] = [0, 3, 6, 7, 4, 1, 2, 5, 8];

/// The same module in **port 11**, which is not the same map: a slot's `side` decides how a
/// module physically mounts, and port 7 is the isolated top slot while port 11 is in the right
/// column, so the module sits turned 90° clockwise. Measured the same way, with both grids
/// showing the same chain index at once: chain 0 is top-left on port 7 but top-right on
/// port 11. A rigid module cannot be mirrored, so one reading fixes the rotation, and this is
/// `GRID_PHYSICAL_PORT7` rotated: cell (row, col) becomes (col, 2 − row).
pub const GRID_PHYSICAL_PORT11: [usize; 9] = [2, 1, 0, 3, 4, 5, 8, 7, 6];

/// The visible 160×80 landscape frame of the 0.96" TFT: GRAM columns 26..106 × rows 1..161.
/// With the driver's rotation 3 (MADCTL MV|MX|BGR) the app's x axis runs down GRAM rows and its
/// y axis runs right-to-left across GRAM columns. MADCTL BGR compensates the physical panel's
/// subpixel order; the app's RGB565 intent is what we show.
pub fn tft_frame_160x80(p: &DcsPanel) -> Vec<u16> {
    let mut out = vec![0u16; 160 * 80];
    let mv = p.madctl & 0x20 != 0;
    for y in 0..80 { for x in 0..160 {
        let (col, row) = if mv { (105 - y, 1 + x) } else { (26 + x.min(79), 1 + y) };
        let (col, row) = (col.min(p.cols - 1), row.min(p.rows - 1));
        let mut px = p.gram[row * p.cols + col];
        if p.inverted { px = !px; }
        out[y * 160 + x] = px;
    } }
    out
}

pub struct Atech14 {
    /// the ST7735 behind the 0.96" TFT; `tft_frame` is what the glass shows
    pub tft: DcsPanel,
    spi: SpiBitBang,
    /// the Knob V1.1's 12-LED ring, in physical order (see `KNOB_RING_PHYSICAL`)
    pub ring: Ws2812Chain,
    /// the two 3×3 Light Grids, in glass order with the board upright — a different map each,
    /// since the two slots mount the module at 90° to one another
    pub grid_7: Ws2812Chain,
    pub grid_11: Ws2812Chain,
    pub gpio_events: u64,
}

impl Default for Atech14 { fn default() -> Self { Self::new() } }

impl Atech14 {
    pub fn new() -> Self {
        Atech14 { tft: DcsPanel::st7735(), spi: SpiBitBang::new(PIN_TFT_SCLK, PIN_TFT_MOSI, PIN_TFT_CS), ring: Ws2812Chain::mapped(&KNOB_RING_PHYSICAL),
                  grid_7: Ws2812Chain::mapped(&GRID_PHYSICAL_PORT7), grid_11: Ws2812Chain::mapped(&GRID_PHYSICAL_PORT11), gpio_events: 0 }
    }
    /// What the 160×80 glass shows.
    pub fn tft_frame(&self) -> Vec<u16> { tft_frame_160x80(&self.tft) }
}

impl BoardModel for Atech14 {
    fn name(&self) -> &'static str { "atech14" }
    fn gpio_changes(&mut self, changes: &[(u8, bool)]) {
        for &(pin, level) in changes {
            self.gpio_events += 1;
            if pin == PIN_TFT_DC { self.tft.dc = level; } else if let Some(b) = self.spi.pin(pin, level) { self.tft.byte(b); }
        }
    }
    fn rmt_frame(&mut self, pin: u8, bits: &[bool]) {
        match pin {
            PIN_RING => self.ring.from_bits(bits),
            PIN_GRID7_A => self.grid_7.from_bits(bits),
            PIN_GRID11_A => self.grid_11.from_bits(bits),
            _ => {}                       // Line B (RGBW) on 7 and 44: not populated on this revision
        }
    }
    fn spi_tx(&mut self, host: u8, data: &[u8]) { if host == 2 { for &b in data { self.tft.byte(b); } } }
    fn gpio_events(&self) -> u64 { self.gpio_events }
    fn display(&self) -> Option<(u32, u32, Vec<u16>, u64)> { Some((160, 80, self.tft_frame(), self.tft.pixels_written)) }
    fn display_version(&self) -> u64 { self.tft.pixels_written }
    fn display_quiet_push(&self) -> bool { true }
    fn display_frames(&self) -> u64 { self.tft.frames }
    fn gram(&self) -> Option<(Vec<u16>, usize, usize)> { Some((self.tft.gram.clone(), self.tft.cols, self.tft.rows)) }
    fn leds(&self) -> Option<(&[[u8; 3]], u64)> { Some((&self.ring.leds, self.ring.updates)) }
    fn led_grids(&self) -> Vec<(&'static str, &[[u8; 3]], u64)> {
        vec![("7", &self.grid_7.leds, self.grid_7.updates), ("11", &self.grid_11.leds, self.grid_11.updates)]
    }
    fn named_pin(&self, name: &str) -> Option<u8> { match name { "btn1" => Some(PIN_BTN1), "btn2" => Some(PIN_BTN2), "sw" | "knob" => Some(PIN_ENC_SW), _ => None } }
    fn encoder(&self) -> Option<(u8, u8)> { Some((PIN_ENC_CLK, PIN_ENC_DT)) }
    fn report(&self) -> String {
        let t = &self.tft;
        format!("[emu] tft: {} RAMWR, {} pixels, madctl={:#x} inverted={} on={} bbox={:?} top colours {:x?}; gpio events {}\n[emu] ring: {} updates, leds {:?}",
                t.frames, t.pixels_written, t.madctl, t.inverted, t.on, t.bbox(), t.histogram(5), self.gpio_events, self.ring.updates, &self.ring.leds[..4])
            + &format!("\n[emu] light grids: port 7 {} updates, leds {:?}; port 11 {} updates, leds {:?}",
                       self.grid_7.updates, &self.grid_7.leds[..], self.grid_11.updates, &self.grid_11.leds[..])
    }
}
