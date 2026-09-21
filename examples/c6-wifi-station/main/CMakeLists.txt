idf_component_register(
    SRCS "station.c" "lcd_text.c" "Vernon_ST7789T/Vernon_ST7789T.c"
    INCLUDE_DIRS "." "Vernon_ST7789T"
    PRIV_REQUIRES esp_wifi esp_event esp_netif nvs_flash lwip esp_lcd driver)
