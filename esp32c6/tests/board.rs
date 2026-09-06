//! The Waveshare ESP32-C6-LCD-1.47 board model on its own: the ST7789 fed command and pixel bytes
//! with the D/C line, and the WS2812 decoded from an RMT frame.
use esp32c6::board::{WaveshareC6Lcd147, LCD_COL_OFFSET, LCD_VISIBLE_COLS, PIN_LCD_DC};
use esp_soc::BoardModel;

fn cmd(b: &mut WaveshareC6Lcd147, c: u8, params: &[u8]) {
    b.gpio_changes(&[(PIN_LCD_DC, false)]); b.spi_tx(2, &[c]);
    if !params.is_empty() { b.gpio_changes(&[(PIN_LCD_DC, true)]); b.spi_tx(2, params); }
}

#[test]
fn st7789_window_madctl_and_visible_columns() {
    let mut b = WaveshareC6Lcd147::new();
    // the D/C pin idles low: the very first command must be seen as a command
    b.spi_tx(2, &[0x11]);                                              // SLPOUT
    assert!(!b.panel.sleeping, "SLPOUT before any D/C edge must still be a command");
    cmd(&mut b, 0x36, &[0x48]);                                        // MADCTL: MX | BGR, as the firmware sets it
    cmd(&mut b, 0x3a, &[0x55]);
    cmd(&mut b, 0x21, &[]); cmd(&mut b, 0x29, &[]);
    // a 2x2 window at logical x=0..1 (RAM column 34+x), y=10..11, red pixels in BGR order
    cmd(&mut b, 0x2a, &[0, 34, 0, 35]); cmd(&mut b, 0x2b, &[0, 10, 0, 11]);
    b.gpio_changes(&[(PIN_LCD_DC, false)]); b.spi_tx(2, &[0x2c]);
    b.gpio_changes(&[(PIN_LCD_DC, true)]); b.spi_tx(2, &[0x00, 0x1f, 0x00, 0x1f, 0x00, 0x1f, 0x00, 0x1f]);
    assert_eq!(b.panel.pixels_written, 4);
    let (w, h, px, _) = b.display().unwrap();
    assert_eq!((w, h), (172, 320));
    // MX mirrors RAM columns and the glass mirrors them back: logical x = 0 is visible column 0;
    // BGR order swaps the channels back, so the 0x001f we sent shows as red 0xf800
    assert_eq!(px[10 * 172], 0xf800, "top-left of the window at visible (0, 10)");
    assert_eq!(px[11 * 172 + 1], 0xf800);
    assert_eq!(px[10 * 172 + 2], 0, "outside the window stays black");
    assert_eq!(b.panel.bbox(), Some((LCD_COL_OFFSET + LCD_VISIBLE_COLS - 2, 10, LCD_COL_OFFSET + LCD_VISIBLE_COLS - 1, 11)));
    // the reset pin low clears the panel state (RAM survives, as on silicon)
    b.gpio_changes(&[(esp32c6::board::PIN_LCD_RST, false)]);
    assert!(b.panel.sleeping && b.panel.resets == 1);
}

#[test]
fn ws2812_from_rmt_bits() {
    let mut b = WaveshareC6Lcd147::new();
    let bits = |bytes: [u8; 3]| -> Vec<bool> { bytes.iter().flat_map(|&v| (0..8).map(move |i| v & (0x80 >> i) != 0)).collect() };
    b.rmt_frame(0, &bits([0x10, 0xab, 0x03]));                          // G, R, B on the wire
    let (leds, updates) = b.leds().unwrap();
    assert_eq!((leds[0], updates), ([0xab, 0x10, 0x03], 1));
    b.rmt_frame(0, &bits([0, 0, 0])[..20]);                              // a short frame changes nothing
    assert_eq!(b.leds().unwrap().1, 1);
    assert_eq!(b.named_pin("boot"), Some(esp32c6::board::PIN_BOOT));
}
