#pragma once
#include <stdint.h>
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
enum { PAGE_SCAN, PAGE_STATUS, PAGE_SPECTRUM };
typedef struct {
    int page, count, rssi, ping_ms;
    unsigned sample;
    char status[80], ip[20], networks[6][64];
    int8_t energy[16];
} radio_snapshot_t;
extern QueueHandle_t radio_pages, radio_results;
void radio_start(int initial_page);
