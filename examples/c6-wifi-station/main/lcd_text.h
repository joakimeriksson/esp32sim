// A 20 x 10 character text screen on the Waveshare ESP32-C6-LCD-1.47, in landscape.
#pragma once
#include <stdint.h>

#define LCD_COLS 20
#define LCD_ROWS 10

#define LCD_WHITE 0xffff
#define LCD_CYAN  0x07ff
#define LCD_GREEN 0x07e0
#define LCD_AMBER 0xfd20
#define LCD_RED   0xf800
#define LCD_GREY  0x8410

/// Bring the panel up and clear it. Without CONFIG_STATION_LCD both calls do nothing.
void lcd_text_init(void);
/// Replace one row (printf style, cut at LCD_COLS). Any task may call it; a row whose text
/// and colour did not change costs no SPI transfer.
void lcd_line(int row, uint16_t rgb565, const char *fmt, ...) __attribute__((format(printf, 3, 4)));
