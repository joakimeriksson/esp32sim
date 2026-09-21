#include <limits.h>
#include <string.h>
#include "sdkconfig.h"
#include "ST7789.h"
#include "nvs_flash.h"
#include "radio.h"

static lv_obj_t *title, *status, *body, *chart;
static lv_chart_series_t *series;
static lv_obj_t *bars[16], *values[16];
static int page;
static unsigned last_sample;

static lv_obj_t *label(int x, int y, int width, const char *text)
{
    lv_obj_t *l = lv_label_create(lv_scr_act());
    lv_obj_set_pos(l, x, y);
    lv_obj_set_width(l, width);
    lv_obj_set_style_text_line_space(l, 0, 0);
    lv_label_set_text(l, text);
    return l;
}

static void create_page(void)
{
    lv_obj_clean(lv_scr_act());
    lv_obj_set_style_bg_color(lv_scr_act(), lv_color_hex(0x101824), 0);
    lv_obj_set_style_text_color(lv_scr_act(), lv_color_hex(0xe8f0ff), 0);
    lv_obj_clear_flag(lv_scr_act(), LV_OBJ_FLAG_SCROLLABLE);
    title = label(8, 8, 156, page == PAGE_SCAN ? "1/3 WiFi networks" :
                  page == PAGE_STATUS ? "2/3 Connection" : "3/3 Radio energy");
    lv_obj_set_style_text_color(title, lv_color_hex(0x56d6c9), 0);
    status = label(8, 32, 156, "Switching radio...");
    label(8, 300, 156, "BOOT: next page");
    body = NULL;
    chart = NULL;
    last_sample = UINT_MAX;
    if (page == PAGE_SPECTRUM) {
        for (int i = 0; i < 16; ++i) {
            lv_obj_t *ch = label(8, 77 + i * 13, 22, "");
            lv_label_set_text_fmt(ch, "%d", 11 + i);
            bars[i] = lv_bar_create(lv_scr_act());
            lv_obj_set_pos(bars[i], 34, 80 + i * 13);
            lv_obj_set_size(bars[i], 85, 7);
            lv_bar_set_range(bars[i], -110, -20);
            lv_bar_set_value(bars[i], -110, LV_ANIM_OFF);
            values[i] = label(125, 77 + i * 13, 44, "--");
        }
    } else {
        body = label(8, 78, 156, "");
        if (page == PAGE_STATUS) {
            chart = lv_chart_create(lv_scr_act());
            lv_obj_set_pos(chart, 8, 188);
            lv_obj_set_size(chart, 156, 85);
            lv_chart_set_range(chart, LV_CHART_AXIS_PRIMARY_Y, -100, -20);
            lv_chart_set_point_count(chart, 40);
            series = lv_chart_add_series(chart, lv_color_hex(0x56d6c9), LV_CHART_AXIS_PRIMARY_Y);
            label(8, 277, 156, "RSSI: -100 to -20 dBm");
        }
    }
}

void app_main(void)
{
    esp_err_t err = nvs_flash_init();
    if (err == ESP_ERR_NVS_NO_FREE_PAGES || err == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_ERROR_CHECK(nvs_flash_erase());
        err = nvs_flash_init();
    }
    ESP_ERROR_CHECK(err);
    gpio_config_t button = { .pin_bit_mask = 1ULL << 9, .mode = GPIO_MODE_INPUT,
                             .pull_up_en = true, .intr_type = GPIO_INTR_DISABLE };
    ESP_ERROR_CHECK(gpio_config(&button));
    LCD_Init();
    BK_Light(50);
    LVGL_Init();
    #ifdef CONFIG_DASH_START_SPECTRUM
    page = PAGE_SPECTRUM;
#else
    page = PAGE_SCAN;
#endif
    create_page();
    radio_start(page);
    int stable = 1, candidate = 1;
    TickType_t changed = 0;
    for (;;) {
        int level = gpio_get_level(9);
        TickType_t now = xTaskGetTickCount();
        if (level != candidate) { candidate = level; changed = now; }
        if (candidate != stable && now - changed >= pdMS_TO_TICKS(40)) {
            stable = candidate;
            if (!stable) {
                page = (page + 1) % 3;
                create_page();
                xQueueOverwrite(radio_pages, &page);
            }
        }
        radio_snapshot_t s;
        if (xQueueReceive(radio_results, &s, 0) && s.page == page) {
            lv_label_set_text(status, s.status);
            if (page == PAGE_SCAN) {
                char text[420] = "";
                for (int i = 0; i < s.count; ++i) {
                    strlcat(text, s.networks[i], sizeof(text));
                    strlcat(text, "\n\n", sizeof(text));
                }
                // Six entries use 216 px with a 12 px font.
                lv_label_set_text(body, text);
            } else if (page == PAGE_STATUS) {
                char ping[40];
                if (s.ping_ms == -2) snprintf(ping, sizeof(ping), "Gateway ping: waiting");
                else if (s.ping_ms < 0) snprintf(ping, sizeof(ping), "Gateway ping: timeout");
                else snprintf(ping, sizeof(ping), "Gateway ping: %d ms", s.ping_ms);
                lv_label_set_text_fmt(body, "IP\n%s\n\n%s", s.ip[0] ? s.ip : "--", ping);
                if (s.rssi && s.sample != last_sample)
                    lv_chart_set_next_value(chart, series, s.rssi);
            } else {
                for (int i = 0; i < 16; ++i) {
                    bool valid = s.energy[i] != INT8_MIN;
                    lv_bar_set_value(bars[i], valid ? s.energy[i] : -110, LV_ANIM_OFF);
                    if (valid) lv_label_set_text_fmt(values[i], "%d", s.energy[i]);
                    else lv_label_set_text(values[i], "--");
                }
            }
            last_sample = s.sample;
        }
        // Only this task calls LVGL; the display driver supplies the tick timer.
        lv_timer_handler();
        vTaskDelay(pdMS_TO_TICKS(10));
    }
}
