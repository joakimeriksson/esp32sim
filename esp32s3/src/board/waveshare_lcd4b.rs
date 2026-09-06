use esp_soc::board::BoardModel;

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
