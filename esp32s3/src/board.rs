//! Boards around the SoC. The SoC model emits generic events (GPIO edges, RMT symbol streams,
//! I2S samples); a `BoardModel` interprets them as the devices wired to the pins.
//!
//! `Atech14` — the Atech 14-port board:
//!   - ST7735 160x80 TFT on bit-banged SPI: SCLK 2, CS 41, MOSI 1, DC 40
//!   - WS2812 12-LED ring on GPIO 8 (via RMT)
//!   - rotary encoder CLK 5 / DT 4 / SW 9, buttons GPIO 17 / 16 (active low)
//!   - MAX98357A I2S amp: BCLK 12, LRCLK 13, DIN 10
//!
//! `NoBoard` — a bare module: nothing on the pins (any ESP32-S3 firmware, console only).

pub use esp_soc::board::{Board, BoardEdge, BoardModel, NoBoard, VirtualCycle};
use esp_soc::picture;

/// Board by name: `atech14` (default), `none`, or a supported Waveshare board.
pub fn make_board(name: &str) -> Option<Board> {
    match name {
        "atech14" | "atech" => Some(Box::new(Atech14::new())),
        "none" | "bare" => Some(Box::new(NoBoard)),
        "waveshare-cam" | "waveshare" => Some(Box::new(WaveshareCam::new())),
        "waveshare-lcd4b" | "lcd4b" => Some(Box::new(WaveshareLcd4b::new())),
        "waveshare-amoled18-v2" | "amoled18-v2" => Some(Box::new(WaveshareAmoled18V2::new())),
        _ => None,
    }
}

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

pub const PIN_AMOLED_SDIO0: u8 = 4;
pub const PIN_AMOLED_SDIO1: u8 = 5;
pub const PIN_AMOLED_SDIO2: u8 = 6;
pub const PIN_AMOLED_SDIO3: u8 = 7;
pub const PIN_AMOLED_SCLK: u8 = 11;
pub const PIN_AMOLED_CS: u8 = 12;
pub const PIN_AMOLED_TE: u8 = 13;
pub const PIN_AMOLED_I2C_SCL: u8 = 14;
pub const PIN_AMOLED_I2C_SDA: u8 = 15;
pub const PIN_AMOLED_TOUCH_INT: u8 = 21;

/// ST7735 controller model: 132x162 GRAM, address window, MADCTL, RGB565 pixels over SPI.
pub struct St7735 {
    pub gram: Vec<u16>,          // 162 rows x 132 cols, index = row*132 + col
    pub madctl: u8,
    pub colmod: u8,
    pub inverted: bool,
    pub sleeping: bool,
    pub on: bool,
    x0: u16, x1: u16, y0: u16, y1: u16,
    xc: u16, yc: u16,
    cmd: u8,
    argn: u8,
    args: [u8; 8],
    // SPI decode state
    sclk: bool, mosi: bool, cs: bool, dc: bool,
    shift: u8, nbits: u8,
    pixel_hi: Option<u8>,
    pub frames: u64,             // RAMWR commands seen
    pub pixels_written: u64,
}

impl Default for St7735 { fn default() -> Self { Self::new() } }

impl St7735 {
    pub const COLS: usize = 132;
    pub const ROWS: usize = 162;
    pub fn new() -> Self {
        St7735 { gram: vec![0; Self::COLS * Self::ROWS], madctl: 0, colmod: 6, inverted: false, sleeping: true, on: false,
                 x0: 0, x1: 131, y0: 0, y1: 161, xc: 0, yc: 0, cmd: 0, argn: 0, args: [0; 8], sclk: false, mosi: false, cs: true, dc: false,
                 shift: 0, nbits: 0, pixel_hi: None, frames: 0, pixels_written: 0 }
    }

    /// Feed one GPIO output change (in order). Returns nothing; decodes SPI on SCLK rising edges while CS is low.
    pub fn gpio(&mut self, pin: u8, level: bool) {
        match pin {
            PIN_TFT_MOSI => self.mosi = level,
            PIN_TFT_DC => self.dc = level,
            PIN_TFT_CS => { self.cs = level; if level { self.nbits = 0; self.shift = 0; } }
            PIN_TFT_SCLK => {
                let rising = level && !self.sclk;
                self.sclk = level;
                if rising && !self.cs {
                    self.shift = (self.shift << 1) | self.mosi as u8;
                    self.nbits += 1;
                    if self.nbits == 8 { let b = self.shift; self.nbits = 0; self.shift = 0; self.byte(b); }
                }
            }
            _ => {}
        }
    }

    /// A byte delivered by a hardware SPI master (DC still comes from its GPIO).
    pub fn spi_byte(&mut self, b: u8) { self.byte(b); }

    fn byte(&mut self, b: u8) {
        if !self.dc {
            self.cmd = b; self.argn = 0; self.pixel_hi = None;
            match b {
                0x01 => { self.madctl = 0; self.inverted = false; self.on = false; self.sleeping = true; }
                0x11 => self.sleeping = false, 0x10 => self.sleeping = true,
                0x20 => self.inverted = false, 0x21 => self.inverted = true,
                0x28 => self.on = false, 0x29 => self.on = true,
                0x2c => { self.xc = self.x0; self.yc = self.y0; self.frames += 1; }
                _ => {}
            }
            return;
        }
        match self.cmd {
            0x2a | 0x2b => {
                if (self.argn as usize) < 4 { self.args[self.argn as usize] = b; self.argn += 1; }
                if self.argn == 4 {
                    let s = ((self.args[0] as u16) << 8) | self.args[1] as u16;
                    let e = ((self.args[2] as u16) << 8) | self.args[3] as u16;
                    if self.cmd == 0x2a { self.x0 = s; self.x1 = e; self.xc = s; } else { self.y0 = s; self.y1 = e; self.yc = s; }
                }
            }
            0x36 => self.madctl = b,
            0x3a => self.colmod = b,
            0x2c => {
                match self.pixel_hi.take() {
                    None => self.pixel_hi = Some(b),
                    Some(hi) => { let px = ((hi as u16) << 8) | b as u16; self.write_pixel(px); }
                }
            }
            _ => {}
        }
    }

    fn write_pixel(&mut self, px: u16) {
        // address counters in the controller's frame; MADCTL MV swaps which counter is "column"
        let (mut col, mut row) = (self.xc as usize, self.yc as usize);
        if self.madctl & 0x20 != 0 { std::mem::swap(&mut col, &mut row); }
        if self.madctl & 0x40 != 0 { col = Self::COLS - 1 - col.min(Self::COLS - 1); }
        if self.madctl & 0x80 != 0 { row = Self::ROWS - 1 - row.min(Self::ROWS - 1); }
        if col < Self::COLS && row < Self::ROWS { self.gram[row * Self::COLS + col] = px; self.pixels_written += 1; }
        // advance within window (x fastest)
        if self.xc >= self.x1 { self.xc = self.x0; if self.yc >= self.y1 { self.yc = self.y0; } else { self.yc += 1; } } else { self.xc += 1; }
    }

    /// Bounding box of non-zero GRAM (for locating the panel's visible window).
    pub fn bbox(&self) -> Option<(usize, usize, usize, usize)> {
        let (mut c0, mut c1, mut r0, mut r1) = (usize::MAX, 0, usize::MAX, 0);
        for r in 0..Self::ROWS { for c in 0..Self::COLS { if self.gram[r * Self::COLS + c] != 0 { c0 = c0.min(c); c1 = c1.max(c); r0 = r0.min(r); r1 = r1.max(r); } } }
        if c1 >= c0 && r1 >= r0 && c0 != usize::MAX { Some((c0, r0, c1, r1)) } else { None }
    }

    /// Visible 160x80 landscape frame. The 0.96" panel maps GRAM cols 26..106 x rows 1..161.
    /// With the driver's rotation 3 (MADCTL MV|MX|BGR) the app's x axis runs down GRAM rows and
    /// its y axis runs right-to-left across GRAM columns.
    pub fn frame_160x80(&self) -> Vec<u16> {
        let mut out = vec![0u16; 160 * 80];
        let mv = self.madctl & 0x20 != 0;
        for y in 0..80 { for x in 0..160 {
            let (col, row) = if mv { (105 - y, 1 + x) } else { (26 + x.min(79), 1 + y) };
            let (col, row) = (col.min(Self::COLS - 1), row.min(Self::ROWS - 1));
            let mut px = self.gram[row * Self::COLS + col];
            if self.inverted { px = !px; }
            // MADCTL BGR compensates the physical panel's subpixel order; the app's RGB565 intent is what we show
            out[y * 160 + x] = px;
        } }
        out
    }

    /// Most common non-zero pixel values (for checking colour decoding).
    pub fn histogram(&self, top: usize) -> Vec<(u16, usize)> {
        let mut m: std::collections::HashMap<u16, usize> = Default::default();
        for &p in &self.gram { if p != 0 { *m.entry(p).or_insert(0) += 1; } }
        let mut v: Vec<(u16, usize)> = m.into_iter().collect(); v.sort_by_key(|a| std::cmp::Reverse(a.1)); v.truncate(top); v
    }
}

/// WS2812 ring fed by RMT symbols.
/// Where chain index `i` of a Light Grid V1.1 sits on the glass, as a row-major cell of the 3×3 the
/// page draws. The module is mounted so that the firmware's row-major `xyToIndex` comes out
/// anti-transposed: cell (row, col) of the chain lands at (2 − col, 2 − row). Read off the
/// board: with every SID voice at its lowest lit level the firmware writes only its bottom
/// row — blue, magenta, orange in chain order — and the glass shows a left column, orange at
/// the top and blue at the bottom.
pub const GRID_PHYSICAL: [usize; 9] = [8, 5, 2, 7, 4, 1, 6, 3, 0];

pub struct Ring { pub leds: Vec<[u8; 3]>, pub updates: u64, physical: Option<&'static [usize; 9]> }
impl Ring {
    pub fn new(n: usize) -> Self { Ring { leds: vec![[0; 3]; n], updates: 0, physical: None } }
    /// A 3×3 grid: chain order in, physical order out (`leds[GRID_PHYSICAL[i]]` holds chain LED `i`).
    pub fn grid() -> Self { Ring { leds: vec![[0; 3]; 9], updates: 0, physical: Some(&GRID_PHYSICAL) } }
    /// Decode a WS2812 bit stream (GRB order) into LED colours.
    pub fn from_bits(&mut self, bits: &[bool]) {
        let n = bits.len() / 24;
        for i in 0..n.min(self.leds.len()) {
            let mut v = 0u32;
            for b in 0..24 { v = (v << 1) | bits[i * 24 + b] as u32; }
            let at = match self.physical { Some(m) => m[i], None => i };
            self.leds[at] = [((v >> 8) & 0xff) as u8, ((v >> 16) & 0xff) as u8, (v & 0xff) as u8];   // GRB -> RGB
        }
        self.updates += 1;
    }
}

pub struct Atech14 {
    pub tft: St7735,
    pub ring: Ring,
    /// the two 3x3 Light Grids, as 9-LED strips
    pub grid_7: Ring,
    pub grid_11: Ring,
    pub gpio_events: u64,
}

impl Default for Atech14 { fn default() -> Self { Self::new() } }

impl Atech14 {
    pub fn new() -> Self { Atech14 { tft: St7735::new(), ring: Ring::new(12), grid_7: Ring::grid(), grid_11: Ring::grid(), gpio_events: 0 } }
}

impl BoardModel for Atech14 {
    fn name(&self) -> &'static str { "atech14" }
    fn gpio_changes(&mut self, changes: &[(u8, bool)]) {
        for &(pin, level) in changes { self.gpio_events += 1; self.tft.gpio(pin, level); }
    }
    fn rmt_frame(&mut self, pin: u8, bits: &[bool]) {
        match pin {
            PIN_RING => self.ring.from_bits(bits),
            PIN_GRID7_A => self.grid_7.from_bits(bits),
            PIN_GRID11_A => self.grid_11.from_bits(bits),
            _ => {}                       // Line B (RGBW) on 7 and 44: not populated on this revision
        }
    }
    fn spi_tx(&mut self, host: u8, data: &[u8]) { if host == 2 { for &b in data { self.tft.spi_byte(b); } } }
    fn gpio_events(&self) -> u64 { self.gpio_events }
    fn display(&self) -> Option<(u32, u32, Vec<u16>, u64)> { Some((160, 80, self.tft.frame_160x80(), self.tft.pixels_written)) }
    fn display_version(&self) -> u64 { self.tft.pixels_written }
    fn display_quiet_push(&self) -> bool { true }
    fn display_frames(&self) -> u64 { self.tft.frames }
    fn gram(&self) -> Option<(Vec<u16>, usize, usize)> { Some((self.tft.gram.clone(), St7735::COLS, St7735::ROWS)) }
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

/// Waveshare ESP32-S3-CAM-OV5640: OV5640 on the LCD_CAM DVP port (SCCB on I2C0 GPIO 7/8),
/// CH32V003 IO expander, ES8311 speaker codec + ES7210 mic ADC on I2C0, audio on I2S0
/// (MCLK 10, BCLK 11, LRCLK 12, DIN 13, DOUT 14), buttons GPIO 0 / 15.
pub struct WaveshareCam { pub gpio_events: u64, pub preview_dirty: bool, sensor: std::sync::Arc<std::sync::Mutex<crate::i2c::SensorState>>, picture: Option<picture::Picture>, frame: Option<(u32, u32, std::sync::Arc<Vec<u8>>)>, pub frames: u64 }
impl Default for WaveshareCam { fn default() -> Self { Self::new() } }

impl WaveshareCam { pub fn new() -> Self { WaveshareCam { gpio_events: 0, preview_dirty: false, sensor: Default::default(), picture: None, frame: None, frames: 0 } } }
impl BoardModel for WaveshareCam {
    fn name(&self) -> &'static str { "waveshare-cam" }
    fn gpio_changes(&mut self, changes: &[(u8, bool)]) { self.gpio_events += changes.len() as u64; }
    fn gpio_events(&self) -> u64 { self.gpio_events }
    fn set_camera_picture(&mut self, p: picture::Picture) { self.picture = Some(p); self.frame = None; self.preview_dirty = true; }
    fn camera_preview(&self, w: u32, h: u32) -> Option<Vec<u8>> {
        let p = self.picture.as_ref()?;
        let mut out = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h { let sy = (y as u64 * p.h as u64 / h as u64) as usize; for x in 0..w { let sx = (x as u64 * p.w as u64 / w as u64) as usize; let o = (sy * p.w as usize + sx) * 3; out.extend_from_slice(&p.rgb[o..o + 3]); } }
        Some(out)
    }
    fn camera_frame(&mut self) -> Option<(u32, u32, std::sync::Arc<Vec<u8>>)> {
        let (w, h) = { let s = self.sensor.lock().unwrap(); (s.width, s.height) };
        if w == 0 || h == 0 { return None; }
        let stale = match &self.frame { Some((fw, fh, _)) => *fw != w || *fh != h, None => true };
        if stale {
            let p = self.picture.as_ref()?;
            self.frame = Some((w, h, std::sync::Arc::new(picture::to_yuyv(p, w, h))));
        }
        self.frames += 1;
        self.frame.clone()
    }
    fn i2c_devices(&mut self) -> Vec<(u8, u8, Box<dyn crate::i2c::I2cDevice>)> {
        use crate::i2c::*;
        vec![
            (0, 0x24, Box::new(Ch32v003::new())),
            (0, 0x3c, Box::new(Ov5640::new(self.sensor.clone()))),
            (0, 0x18, Box::new(Reg8Device::new("es8311", &[(0xfd, 0x83), (0xfe, 0x11)]))),
            (0, 0x40, Box::new(Reg8Device::new("es7210", &[(0x3d, 0x72), (0x3e, 0x10)]))),
        ]
    }
}

/// Waveshare ESP32-S3-Touch-LCD-4B: ST7701S 480x480 on the LCD_CAM RGB bus (16-bit, DE 17, VSYNC 3,
/// HSYNC 46, PCLK 9), its init SPI bit-banged through a TCA9554 (I2C0 @0x20, SDA 47 / SCL 48),
/// GT911 touch @0x14, ES8311/ES7210 codecs on I2S0 (MCLK 5, BCLK 16), backlight LEDC on GPIO 4.
pub struct WaveshareLcd4b {
    pub gpio_events: u64, pub w: u32, pub h: u32, pub frame: Vec<u16>, pub frames: u64,
    pub panel: std::sync::Arc<std::sync::Mutex<crate::i2c::St7701State>>,
    pub touch_state: std::sync::Arc<std::sync::Mutex<crate::i2c::TouchState>>,
}
impl Default for WaveshareLcd4b { fn default() -> Self { Self::new() } }

impl WaveshareLcd4b {
    pub fn new() -> Self { WaveshareLcd4b { gpio_events: 0, w: 480, h: 480, frame: vec![0; 480 * 480], frames: 0, panel: Default::default(), touch_state: Default::default() } }
}
impl BoardModel for WaveshareLcd4b {
    fn name(&self) -> &'static str { "waveshare-lcd4b" }
    fn gpio_changes(&mut self, changes: &[(u8, bool)]) { self.gpio_events += changes.len() as u64; }
    fn gpio_events(&self) -> u64 { self.gpio_events }
    fn i2c_devices(&mut self) -> Vec<(u8, u8, Box<dyn crate::i2c::I2cDevice>)> {
        use crate::i2c::*;
        vec![
            (0, 0x20, Box::new(Tca9554::new(self.panel.clone()))),
            (0, 0x14, Box::new(Gt911::new(self.touch_state.clone(), 480, 480))),
            (0, 0x18, Box::new(Reg8Device::new("es8311", &[(0xfd, 0x83), (0xfe, 0x11)]))),
            (0, 0x40, Box::new(Reg8Device::new("es7210", &[(0x3d, 0x72), (0x3e, 0x10)]))),
        ]
    }
    fn lcd_frame(&mut self, w: u32, h: u32, rgb565: &[u8]) {
        if (w, h) != (self.w, self.h) { self.w = w; self.h = h; self.frame = vec![0; (w * h) as usize]; }
        for (i, px) in rgb565.as_chunks::<2>().0.iter().enumerate().take(self.frame.len()) { self.frame[i] = u16::from_le_bytes([px[0], px[1]]); }
        self.frames += 1;
    }
    fn display(&self) -> Option<(u32, u32, Vec<u16>, u64)> { Some((self.w, self.h, self.frame.clone(), self.frames)) }
    fn display_version(&self) -> u64 { self.frames }
    fn display_frames(&self) -> u64 { self.frames }
    fn touch(&mut self, x: u16, y: u16, down: bool) {
        let mut t = self.touch_state.lock().unwrap(); t.x = x; t.y = y;
        if down { t.down = true; t.seen = false; t.release_pending = false; } else if t.seen { t.down = false; } else { t.release_pending = true; }
    }
}

/// CO5300 controller and 368x448 RGB565 frame memory used by the Waveshare AMOLED V2 board.
pub struct Co5300 {
    pub frame: Vec<u16>,
    pub frames: u64,
    pub pixels_written: u64,
    x0: u16,
    x1: u16,
    y0: u16,
    y1: u16,
    x: u16,
    y: u16,
    pending: Option<u8>,
    pixel_hi: Option<u8>,
}

impl Co5300 {
    pub const WIDTH: usize = 368;
    pub const HEIGHT: usize = 448;
    pub const X_OFFSET: u16 = 0x10;

    pub fn new() -> Self {
        Self {
            frame: vec![0; Self::WIDTH * Self::HEIGHT],
            frames: 0,
            pixels_written: 0,
            x0: Self::X_OFFSET,
            x1: Self::X_OFFSET + Self::WIDTH as u16 - 1,
            y0: 0,
            y1: Self::HEIGHT as u16 - 1,
            x: 0,
            y: 0,
            pending: None,
            pixel_hi: None,
        }
    }

    pub fn transaction(&mut self, tx: &[u8]) {
        let data = if tx.len() >= 4 && matches!(tx[0], 0x02 | 0x32) {
            let command = tx[2];
            if command == 0x01 {
                *self = Self::new();
                return;
            }
            self.pending = Some(command);
            self.pixel_hi = None;
            if command == 0x2c { self.x = self.x0; self.y = self.y0; self.frames += 1; }
            &tx[4..]
        } else { tx };
        match self.pending {
            Some(0x2a) if data.len() >= 4 => {
                self.x0 = u16::from_be_bytes([data[0], data[1]]);
                self.x1 = u16::from_be_bytes([data[2], data[3]]);
                self.x = self.x0;
                self.pending = None;
            }
            Some(0x2b) if data.len() >= 4 => {
                self.y0 = u16::from_be_bytes([data[0], data[1]]);
                self.y1 = u16::from_be_bytes([data[2], data[3]]);
                self.y = self.y0;
                self.pending = None;
            }
            // ESP-IDF sends the command and its four parameter bytes in separate
            // SPI transfers while keeping chip select active. Preserve the command
            // after the header-only transfer so the next transfer sets the window.
            Some(0x2a | 0x2b) => {}
            Some(0x2c | 0x3c) => {
                for &byte in data {
                    match self.pixel_hi.take() {
                        None => self.pixel_hi = Some(byte),
                        Some(high) => self.write_pixel(u16::from_be_bytes([high, byte])),
                    }
                }
            }
            Some(_) => self.pending = None,
            None => {}
        }
    }

    fn write_pixel(&mut self, pixel: u16) {
        let framebuffer_x = self.x.checked_sub(Self::X_OFFSET).map(usize::from);
        if framebuffer_x.is_some_and(|x| x < Self::WIDTH) && (self.y as usize) < Self::HEIGHT {
            self.frame[self.y as usize * Self::WIDTH + framebuffer_x.expect("checked panel column must exist")] = pixel;
            self.pixels_written += 1;
        }
        if self.x >= self.x1 {
            self.x = self.x0;
            if self.y >= self.y1 { self.y = self.y0; } else { self.y += 1; }
        } else {
            self.x += 1;
        }
    }
}

impl Default for Co5300 { fn default() -> Self { Self::new() } }

/// Waveshare ESP32-S3-Touch-AMOLED-1.8 V2 with the CO5300 and CST820 device set.
pub struct WaveshareAmoled18V2 {
    pub gpio_events: u64,
    pub panel: Co5300,
    pub touch_state: std::sync::Arc<std::sync::Mutex<crate::i2c::TouchState>>,
    cycle: VirtualCycle,
    next_te_cycle: Option<VirtualCycle>,
    te_level: bool,
    touch_irq_level: bool,
    /// At most two touch IRQ transitions: the first observable edge and a coalesced final level.
    pending_touch_irq: Option<(VirtualCycle, bool)>,
    pending_touch_final: Option<bool>,
    edges: Vec<BoardEdge>,
}

impl WaveshareAmoled18V2 {
    const APPROXIMATE_TE_HALF_PERIOD: VirtualCycle = crate::periph::CPU_HZ / 120;

    pub fn new() -> Self {
        Self {
            gpio_events: 0,
            panel: Co5300::new(),
            touch_state: Default::default(),
            cycle: 0,
            next_te_cycle: Some(Self::APPROXIMATE_TE_HALF_PERIOD),
            te_level: true,
            touch_irq_level: true,
            pending_touch_irq: None,
            pending_touch_final: None,
            edges: Vec::new(),
        }
    }

    fn queue_touch(&mut self, cycle: VirtualCycle, x: u16, y: u16, down: bool) {
        let mut touch = self.touch_state.lock().expect("AMOLED touch state mutex poisoned");
        touch.x = x.min(Co5300::WIDTH as u16 - 1);
        touch.y = y.min(Co5300::HEIGHT as u16 - 1);
        if down { touch.down = true; touch.seen = false; touch.release_pending = false; }
        else if touch.seen { touch.down = false; }
        else { touch.release_pending = true; }
        drop(touch);
        let level = !down;
        let queued_level = self.pending_touch_final.or(self.pending_touch_irq.map(|(_, level)| level)).unwrap_or(self.touch_irq_level);
        if level != queued_level {
            match self.pending_touch_irq {
                Some((_, first_level)) if level == first_level => self.pending_touch_final = None,
                Some(_) => self.pending_touch_final = Some(level),
                None => self.pending_touch_irq = Some((cycle.max(self.cycle).saturating_add(1), level)),
            }
        }
    }
}

impl Default for WaveshareAmoled18V2 { fn default() -> Self { Self::new() } }

impl BoardModel for WaveshareAmoled18V2 {
    fn name(&self) -> &'static str { "waveshare-amoled18-v2" }
    fn gpio_changes(&mut self, changes: &[(u8, bool)]) { self.gpio_events += changes.len() as u64; }
    fn gpio_events(&self) -> u64 { self.gpio_events }
    fn spi_transfer(&mut self, host: u8, tx: &[u8], rx_len: usize) -> Vec<u8> {
        if host == 2 { self.panel.transaction(tx); }
        vec![0xff; rx_len]
    }
    fn i2c_devices(&mut self) -> Vec<(u8, u8, Box<dyn crate::i2c::I2cDevice>)> {
        use crate::i2c::*;
        vec![
            (0, 0x15, Box::new(Cst820::new(self.touch_state.clone()))),
            (0, 0x20, Box::new(Tca9554::register_ram_stub())),
            (0, 0x34, Box::new(Reg8Device::new("axp2101-pmic-initialization-stub", &[(0x03, 0x4a)]))),
            (0, 0x51, Box::new(Reg8Device::new("pcf85063a-rtc-initialization-stub", &[]))),
            (0, 0x6b, Box::new(Reg8Device::new("qmi8658-imu-initialization-stub", &[(0x00, 0x05)]))),
        ]
    }
    fn display(&self) -> Option<(u32, u32, Vec<u16>, u64)> {
        Some((Co5300::WIDTH as u32, Co5300::HEIGHT as u32, self.panel.frame.clone(), self.panel.pixels_written))
    }
    fn display_version(&self) -> u64 { self.panel.pixels_written }
    fn display_frames(&self) -> u64 { self.panel.frames }
    fn display_quiet_push(&self) -> bool { true }
    fn input_levels(&self) -> Vec<(u8, bool)> {
        vec![(PIN_AMOLED_TE, self.te_level), (PIN_AMOLED_TOUCH_INT, self.touch_irq_level)]
    }
    fn touch(&mut self, x: u16, y: u16, down: bool) { self.queue_touch(self.cycle, x, y, down); }
    fn touch_at(&mut self, cycle: VirtualCycle, x: u16, y: u16, down: bool) { self.queue_touch(cycle, x, y, down); }
    fn next_deadline(&self) -> Option<VirtualCycle> {
        match (self.next_te_cycle, self.pending_touch_irq) {
            (Some(te), Some((touch, _))) => Some(te.min(touch)),
            (Some(te), None) => Some(te),
            (None, Some((touch, _))) => Some(touch),
            (None, None) => None,
        }
    }
    fn advance_to(&mut self, cycle: VirtualCycle) {
        assert!(cycle >= self.cycle, "board time moved backwards from {} to {}", self.cycle, cycle);
        while self.next_deadline().is_some_and(|deadline| deadline <= cycle) {
            let deadline = self.next_deadline().expect("due board transition must have a deadline");
            if self.next_te_cycle == Some(deadline) {
                self.te_level = !self.te_level;
                self.edges.push(BoardEdge { cycle: deadline, pin: PIN_AMOLED_TE, level: self.te_level });
                self.next_te_cycle = deadline.checked_add(Self::APPROXIMATE_TE_HALF_PERIOD);
            }
            if self.pending_touch_irq.is_some_and(|(touch_cycle, _)| touch_cycle == deadline) {
                let (_, level) = self.pending_touch_irq.take().expect("due touch interrupt must remain pending");
                self.touch_irq_level = level;
                self.edges.push(BoardEdge { cycle: deadline, pin: PIN_AMOLED_TOUCH_INT, level });
                if self.pending_touch_final.is_some_and(|final_level| final_level != level) {
                    let final_level = self.pending_touch_final.take().expect("different final touch level must remain pending");
                    self.pending_touch_irq = Some((deadline.saturating_add(1), final_level));
                } else {
                    self.pending_touch_final = None;
                }
            }
        }
        self.cycle = cycle;
    }
    fn take_edges(&mut self) -> Vec<BoardEdge> { std::mem::take(&mut self.edges) }
}

#[cfg(test)]
mod amoled_tests {
    use super::*;

    fn cpu_transfer(command: u8, data: &[u8]) -> Vec<u8> {
        let mut spi = esp_periph::gpspi::GpSpi::new();
        spi.write(0x10, (1 << 31) | (1 << 30) | (1 << 27));
        spi.write(0x14, 23 << 27);
        spi.write(0x18, (7 << 28) | 0x02);
        spi.write(0x04, (command as u32) << 16);
        spi.write(0x1c, data.len() as u32 * 8 - 1);
        for (index, &byte) in data.iter().enumerate() {
            spi.w[index / 4] |= u32::from(byte) << (8 * (index % 4));
        }
        spi.write(0, 1 << 24);
        spi.take_transfer().expect("CPU GP-SPI transfer must be ready").tx
    }

    fn dma_transfer(command: u8, data: &[u8]) -> Vec<u8> {
        let mut spi = esp_periph::gpspi::GpSpi::new();
        spi.write(0x30, 1 << 28);
        spi.write(0x10, (1 << 31) | (1 << 30) | (1 << 27));
        spi.write(0x14, 23 << 27);
        spi.write(0x18, (7 << 28) | 0x32);
        spi.write(0x04, (command as u32) << 16);
        spi.write(0x1c, data.len() as u32 * 8 - 1);
        spi.write(0, 1 << 24);
        spi.complete_dma_tx(data);
        spi.take_transfer().expect("DMA GP-SPI transfer must be ready").tx
    }

    #[test]
    fn combined_gpspi_parameter_and_color_phases_update_panel_pixels() {
        let mut panel = Co5300::new();
        panel.transaction(&cpu_transfer(0x2a, &[0, 0x11, 0, 0x12]));
        panel.transaction(&cpu_transfer(0x2b, &[0, 3, 0, 3]));
        panel.transaction(&dma_transfer(0x2c, &[0xf8, 0, 0x07, 0xe0]));
        assert_eq!(panel.frame[3 * Co5300::WIDTH + 1], 0xf800);
        assert_eq!(panel.frame[3 * Co5300::WIDTH + 2], 0x07e0);
        assert_eq!(panel.pixels_written, 2);
        assert_eq!(panel.frames, 1);
    }

    #[test]
    fn split_gpspi_commands_and_parameters_place_partial_updates_in_the_window() {
        let mut panel = Co5300::new();
        // The IDF LCD SPI driver sends these headers separately from the parameters.
        panel.transaction(&[0x02, 0, 0x2a, 0]);
        panel.transaction(&[0, 0x60, 0, 0x61]);
        panel.transaction(&[0x02, 0, 0x2b, 0]);
        panel.transaction(&[0, 140, 0, 141]);
        panel.transaction(&[0x32, 0, 0x2c, 0]);
        panel.transaction(&[0xf8, 0, 0x07, 0xe0]);
        panel.transaction(&[0x32, 0, 0x3c, 0]);
        panel.transaction(&[0, 0x1f, 0xff, 0xff]);
        assert_eq!(&panel.frame[140 * Co5300::WIDTH + 80..140 * Co5300::WIDTH + 82], &[0xf800, 0x07e0]);
        assert_eq!(&panel.frame[141 * Co5300::WIDTH + 80..141 * Co5300::WIDTH + 82], &[0x001f, 0xffff]);
        assert_eq!(panel.pixels_written, 4);
        assert_eq!(panel.frame[0], 0);
    }

    #[test]
    fn physical_x_offset_maps_the_full_panel_width_to_the_framebuffer() {
        let mut panel = Co5300::new();
        let x1 = Co5300::X_OFFSET + Co5300::WIDTH as u16 - 1;
        panel.transaction(&cpu_transfer(0x2a, &[0, Co5300::X_OFFSET as u8, (x1 >> 8) as u8, x1 as u8]));
        panel.transaction(&cpu_transfer(0x2b, &[0, 0, 0, 0]));
        let mut pixels = Vec::with_capacity(Co5300::WIDTH * 2);
        for pixel in 1..=Co5300::WIDTH as u16 { pixels.extend_from_slice(&pixel.to_be_bytes()); }
        panel.transaction(&dma_transfer(0x2c, &pixels));
        assert_eq!(panel.pixels_written, Co5300::WIDTH as u64);
        assert_eq!(panel.frame[0], 1);
        assert_eq!(panel.frame[Co5300::WIDTH - 1], Co5300::WIDTH as u16);
        assert_eq!(&panel.frame[..Co5300::WIDTH], &(1..=Co5300::WIDTH as u16).collect::<Vec<_>>());
    }

    #[test]
    fn software_reset_clears_the_panel_parser_window_and_framebuffer() {
        let mut panel = Co5300::new();
        panel.transaction(&cpu_transfer(0x2a, &[0, 0x11, 0, 0x11]));
        panel.transaction(&cpu_transfer(0x2b, &[0, 3, 0, 3]));
        panel.transaction(&dma_transfer(0x2c, &[0xf8, 0]));
        assert_eq!(panel.frame[3 * Co5300::WIDTH + 1], 0xf800);

        panel.transaction(&[0x02, 0, 0x01, 0]);
        assert!(panel.frame.iter().all(|&pixel| pixel == 0));
        assert_eq!((panel.x0, panel.x1, panel.y0, panel.y1),
                   (Co5300::X_OFFSET, Co5300::X_OFFSET + Co5300::WIDTH as u16 - 1, 0, Co5300::HEIGHT as u16 - 1));
        assert_eq!((panel.pending, panel.pixel_hi, panel.frames, panel.pixels_written), (None, None, 0, 0));
    }

    #[test]
    fn board_exposes_the_v2_device_addresses_on_i2c0() {
        let mut board = WaveshareAmoled18V2::new();
        let addresses: Vec<_> = board.i2c_devices().into_iter().map(|(bus, address, _)| (bus, address)).collect();
        assert_eq!(addresses, [(0, 0x15), (0, 0x20), (0, 0x34), (0, 0x51), (0, 0x6b)]);
    }

    #[test]
    fn board_name_selects_only_the_v2_model() {
        assert_eq!(make_board("waveshare-amoled18-v2").unwrap().name(), "waveshare-amoled18-v2");
        assert_eq!(make_board("amoled18-v2").unwrap().name(), "waveshare-amoled18-v2");
        assert!(make_board("waveshare-amoled18").is_none());
    }

    #[test]
    fn approximate_te_signal_preserves_edge_timestamps() {
        let mut board = WaveshareAmoled18V2::new();
        let half_period = WaveshareAmoled18V2::APPROXIMATE_TE_HALF_PERIOD;
        assert_eq!(board.next_deadline(), Some(half_period));
        board.advance_to(half_period - 1);
        assert!(board.take_edges().is_empty());
        board.advance_to(half_period);
        assert_eq!(board.take_edges(), [BoardEdge { cycle: half_period, pin: PIN_AMOLED_TE, level: false }]);
        board.advance_to(half_period * 2);
        assert_eq!(board.take_edges(), [BoardEdge { cycle: half_period * 2, pin: PIN_AMOLED_TE, level: true }]);
    }

    #[test]
    fn touch_signal_preserves_active_low_edge_timestamps() {
        let mut board = WaveshareAmoled18V2::new();
        board.touch(100, 200, true);
        board.touch(100, 200, false);
        board.advance_to(2);
        assert_eq!(board.take_edges(), [
            BoardEdge { cycle: 1, pin: PIN_AMOLED_TOUCH_INT, level: false },
            BoardEdge { cycle: 2, pin: PIN_AMOLED_TOUCH_INT, level: true },
        ]);
    }

    #[test]
    fn touch_burst_keeps_the_first_edge_and_coalesces_to_the_final_level() {
        let mut board = WaveshareAmoled18V2::new();
        for index in 0..1000 { board.touch(100, 200, index % 2 == 0); }
        assert_eq!(board.pending_touch_irq, Some((1, false)));
        assert_eq!(board.pending_touch_final, Some(true));
        board.advance_to(2);
        assert_eq!(board.take_edges(), [
            BoardEdge { cycle: 1, pin: PIN_AMOLED_TOUCH_INT, level: false },
            BoardEdge { cycle: 2, pin: PIN_AMOLED_TOUCH_INT, level: true },
        ]);
        assert_eq!(board.input_levels(), [(PIN_AMOLED_TE, true), (PIN_AMOLED_TOUCH_INT, true)]);
    }

    #[test]
    fn te_deadline_overflow_disables_future_transitions() {
        let mut board = WaveshareAmoled18V2::new();
        board.cycle = VirtualCycle::MAX - 1;
        board.next_te_cycle = Some(VirtualCycle::MAX);
        board.advance_to(VirtualCycle::MAX);
        assert_eq!(board.take_edges(), [BoardEdge { cycle: VirtualCycle::MAX, pin: PIN_AMOLED_TE, level: false }]);
        assert_eq!(board.next_deadline(), None);
    }

}
