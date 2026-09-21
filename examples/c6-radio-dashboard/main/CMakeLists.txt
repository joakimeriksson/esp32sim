idf_component_register(
    SRCS "dashboard.c" "radio.c"
         "LCD_Driver/Vernon_ST7789T/Vernon_ST7789T.c"
         "LCD_Driver/ST7789.c" "LVGL_Driver/LVGL_Driver.c"
    INCLUDE_DIRS "." "LCD_Driver" "LCD_Driver/Vernon_ST7789T" "LVGL_Driver"
    PRIV_REQUIRES lvgl__lvgl esp_wifi esp_event esp_netif nvs_flash ieee802154 lwip esp_timer esp_lcd driver)
