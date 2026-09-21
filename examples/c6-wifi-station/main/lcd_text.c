// Text on the board's ST7789T without a graphics library: the 172 x 320 panel is used in
// landscape as 20 x 10 cells of a doubled 8 x 8 font. The panel stays in its native portrait
// scan; a text row is one 16 x 320 strip, rotated here, and goes out as a single transfer.
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include "sdkconfig.h"
#include "lcd_text.h"

#if CONFIG_STATION_LCD
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "driver/gpio.h"
#include "driver/spi_master.h"
#include "esp_heap_caps.h"
#include "esp_lcd_panel_io.h"
#include "esp_lcd_panel_ops.h"
#include "Vernon_ST7789T.h"
#include "font8x8_basic.h"

// The board's wiring (Waveshare ESP32-C6-LCD-1.47 schematic).
#define PIN_SCLK 7
#define PIN_MOSI 6
#define PIN_CS   14
#define PIN_DC   15
#define PIN_RST  21
#define PIN_BL   22
#define PANEL_W  172
#define PANEL_H  320
#define GAP_X    34              // the 172 columns sit at 34..205 of the controller's 240
#define CELL     16              // a glyph cell, 8 x 8 doubled
#define MARGIN   ((PANEL_W - LCD_ROWS * CELL) / 2)

static esp_lcd_panel_handle_t panel;
static SemaphoreHandle_t lock, sent;
static uint16_t *strip;          // CELL x PANEL_H pixels, RGB565
static char shown[LCD_ROWS][LCD_COLS + 1];
static uint16_t shown_colour[LCD_ROWS];

static bool on_sent(esp_lcd_panel_io_handle_t io, esp_lcd_panel_io_event_data_t *e, void *ctx)
{
    BaseType_t woke = pdFALSE;
    xSemaphoreGiveFromISR(sent, &woke);
    return woke == pdTRUE;
}

// One strip to the panel; the buffer is shared, so wait until the transfer has left it.
static void push(int x0, int width)
{
    esp_lcd_panel_draw_bitmap(panel, GAP_X + x0, 0, GAP_X + x0 + width, PANEL_H, strip);
    xSemaphoreTake(sent, pdMS_TO_TICKS(1000));
}

static void draw_row(int row, uint16_t colour, const char *text)
{
    size_t len = strlen(text);
    uint16_t ink = colour;                                          // RGB565 as it is in memory: the driver's RAMCTRL makes the panel little-endian
    int top = MARGIN + row * CELL;                                  // across the landscape view
#if CONFIG_STATION_LCD_FLIP
    int x0 = top;
#else
    int x0 = PANEL_W - top - CELL;
#endif
    for (int py = 0; py < PANEL_H; ++py) {
        for (int sx = 0; sx < CELL; ++sx) {
#if CONFIG_STATION_LCD_FLIP
            int lx = PANEL_H - 1 - py, ly = sx;
#else
            int lx = py, ly = CELL - 1 - sx;
#endif
            size_t col = (size_t)lx / CELL;
            unsigned char c = col < len ? (unsigned char)text[col] : ' ';
            if (c > 127) c = '?';
            bool set = font8x8_basic[c][ly / 2] >> ((lx % CELL) / 2) & 1;
            strip[py * CELL + sx] = set ? ink : 0;
        }
    }
    push(x0, CELL);
}

void lcd_text_init(void)
{
    lock = xSemaphoreCreateMutex();
    sent = xSemaphoreCreateBinary();
    strip = heap_caps_calloc(CELL * PANEL_H, sizeof(uint16_t), MALLOC_CAP_DMA);
    assert(lock && sent && strip);

    spi_bus_config_t bus = { .sclk_io_num = PIN_SCLK, .mosi_io_num = PIN_MOSI, .miso_io_num = -1,
                             .quadwp_io_num = -1, .quadhd_io_num = -1,
                             .max_transfer_sz = CELL * PANEL_H * sizeof(uint16_t) };
    ESP_ERROR_CHECK(spi_bus_initialize(SPI2_HOST, &bus, SPI_DMA_CH_AUTO));
    esp_lcd_panel_io_handle_t io = NULL;
    esp_lcd_panel_io_spi_config_t io_config = { .dc_gpio_num = PIN_DC, .cs_gpio_num = PIN_CS,
                                                .pclk_hz = 12 * 1000 * 1000, .lcd_cmd_bits = 8,
                                                .lcd_param_bits = 8, .spi_mode = 0,
                                                .trans_queue_depth = 4, .on_color_trans_done = on_sent };
    ESP_ERROR_CHECK(esp_lcd_new_panel_io_spi((esp_lcd_spi_bus_handle_t)SPI2_HOST, &io_config, &io));
    // BGR: this module's glass exchanges red and blue unless MADCTL's BGR bit is set (seen on the
    // board: with it clear, green text was blue).
    esp_lcd_panel_dev_st7789t_config_t config = { .reset_gpio_num = PIN_RST,
                                                  .rgb_endian = LCD_RGB_ENDIAN_BGR, .bits_per_pixel = 16 };
    ESP_ERROR_CHECK(esp_lcd_new_panel_st7789t(io, &config, &panel));
    ESP_ERROR_CHECK(esp_lcd_panel_reset(panel));
    ESP_ERROR_CHECK(esp_lcd_panel_init(panel));
    ESP_ERROR_CHECK(esp_lcd_panel_mirror(panel, true, false));

    for (int x = 0; x < PANEL_W; x += CELL)                          // black, before the light comes on
        push(x, PANEL_W - x < CELL ? PANEL_W - x : CELL);
    ESP_ERROR_CHECK(esp_lcd_panel_disp_on_off(panel, true));
    gpio_config_t light = { .mode = GPIO_MODE_OUTPUT, .pin_bit_mask = 1ULL << PIN_BL };
    ESP_ERROR_CHECK(gpio_config(&light));
    gpio_set_level(PIN_BL, 1);
}

void lcd_line(int row, uint16_t rgb565, const char *fmt, ...)
{
    if (!panel || row < 0 || row >= LCD_ROWS) return;
    char text[LCD_COLS + 1];
    va_list args;
    va_start(args, fmt);
    vsnprintf(text, sizeof(text), fmt, args);
    va_end(args);
    xSemaphoreTake(lock, portMAX_DELAY);
    if (strcmp(text, shown[row]) || rgb565 != shown_colour[row]) {
        strcpy(shown[row], text);
        shown_colour[row] = rgb565;
        draw_row(row, rgb565, text);
    }
    xSemaphoreGive(lock);
}
#else
void lcd_text_init(void) {}
void lcd_line(int row, uint16_t rgb565, const char *fmt, ...) { (void)row; (void)rgb565; (void)fmt; }
#endif
