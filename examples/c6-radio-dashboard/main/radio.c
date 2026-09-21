#include <stdio.h>
#include <string.h>
#include <limits.h>
#include "sdkconfig.h"
#include "radio.h"
#include "freertos/task.h"
#include "freertos/event_groups.h"
#include "esp_wifi.h"
#include "esp_event.h"
#include "esp_netif.h"
#include "esp_ieee802154.h"
#include "esp_log.h"
#include "ping/ping_sock.h"

QueueHandle_t radio_pages, radio_results;
static QueueHandle_t energy_results, ping_results;
static EventGroupHandle_t events;
static esp_netif_t *netif;
static void wifi_setup(void);
#define HAS_IP BIT0
#define RETRY BIT1

static void wifi_event(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    if (base == IP_EVENT && id == IP_EVENT_STA_GOT_IP) {
        xEventGroupSetBits(events, HAS_IP);
        ESP_LOGI("dashboard", "GOT_IP");
    }
    if (base == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) {
        xEventGroupClearBits(events, HAS_IP);
        xEventGroupSetBits(events, RETRY);
        ESP_LOGI("dashboard", "DISCONNECTED reason=%u",
                 ((wifi_event_sta_disconnected_t *)data)->reason);
    }
}

void IRAM_ATTR esp_ieee802154_energy_detect_done(int8_t power)
{
    BaseType_t wake = pdFALSE;
    xQueueSendFromISR(energy_results, &power, &wake);
    if (wake) portYIELD_FROM_ISR();
}

static void ping_end(esp_ping_handle_t h, void *arg)
{
    uint32_t received = 0, ms = 0;
    esp_ping_get_profile(h, ESP_PING_PROF_REPLY, &received, sizeof(received));
    esp_ping_get_profile(h, ESP_PING_PROF_TIMEGAP, &ms, sizeof(ms));
    int result = received ? (int)ms : -1;
    esp_ping_delete_session(h);
    xQueueOverwrite(ping_results, &result);
}

static const char *security(wifi_auth_mode_t auth)
{
    switch (auth) {
    case WIFI_AUTH_OPEN: return "OPEN";
    case WIFI_AUTH_WEP: return "WEP";
    case WIFI_AUTH_WPA_PSK: return "WPA";
    case WIFI_AUTH_WPA2_PSK: return "WPA2";
    case WIFI_AUTH_WPA_WPA2_PSK: return "WPA/2";
    case WIFI_AUTH_WPA3_PSK: return "WPA3";
    case WIFI_AUTH_WPA2_WPA3_PSK: return "WPA2/3";
    default: return "SECURE";
    }
}

static void worker(void *arg)
{
    int page = (int)(intptr_t)arg, previous = -1;
    bool wifi_initialized = false;
    bool wifi_started = false, ieee_started = false, ping_pending = false;
    TickType_t next_scan = 0, next_connect = 0, next_ping = 0;
    radio_snapshot_t s = { .ping_ms = -2 };
    memset(s.energy, INT8_MIN, sizeof(s.energy));

    for (;;) {
        xQueueReceive(radio_pages, &page, 0);
        if (page != previous) {
            // The worker is the sole owner of radio lifecycle and scan operations.
            if (page == PAGE_SPECTRUM) {
                if (wifi_started) {
                    ESP_ERROR_CHECK(esp_wifi_stop());
                    wifi_started = false;
                }
                xEventGroupClearBits(events, HAS_IP | RETRY);
                ESP_ERROR_CHECK(esp_ieee802154_enable());
                ieee_started = true;
            } else {
                if (ieee_started) {
                    ESP_ERROR_CHECK(esp_ieee802154_disable());
                    ieee_started = false;
                }
                if (!wifi_initialized) { wifi_setup(); wifi_initialized = true; }
                // Disconnect before scanning; retry events are ignored outside status mode.
                if (wifi_started) {
                    ESP_ERROR_CHECK(esp_wifi_stop());
                    wifi_started = false;
                }
                xEventGroupClearBits(events, HAS_IP | RETRY);
                ESP_ERROR_CHECK(esp_wifi_start());
                wifi_started = true;
                next_scan = next_connect = next_ping = xTaskGetTickCount();
            }
            previous = page;
            s.page = page;
            s.ip[0] = 0;
            s.rssi = 0;
            s.ping_ms = -2;
            snprintf(s.status, sizeof(s.status), "%s", page == PAGE_SPECTRUM ?
                     "Measuring channels 11-26" : page == PAGE_SCAN ? "Scanning..." : "Connecting...");
            ESP_LOGI("dashboard", "MODE %d", page);
            xQueueOverwrite(radio_results, &s);
        }
        TickType_t now = xTaskGetTickCount();
        int ping;
        if (xQueueReceive(ping_results, &ping, 0)) {
            ping_pending = false;
            if (page == PAGE_STATUS) s.ping_ms = ping;
        }
        if (page == PAGE_SPECTRUM) {
            bool ok = true;
            for (int i = 0; i < 16; ++i) {
                int8_t power;
                xQueueReset(energy_results);
                esp_err_t err = esp_ieee802154_set_channel(11 + i);
                if (err == ESP_OK) err = esp_ieee802154_energy_detect(40);
                if (err == ESP_OK && xQueueReceive(energy_results, &power, pdMS_TO_TICKS(100))) {
                    s.energy[i] = power;
                } else {
                    s.energy[i] = INT8_MIN;
                    ok = false;
                    // Do not start another measurement while a timed-out one is active.
                    ESP_ERROR_CHECK(esp_ieee802154_disable());
                    ESP_ERROR_CHECK(esp_ieee802154_enable());
                }
            }
            snprintf(s.status, sizeof(s.status), "%s", ok ? "Energy in dBm / ch 11-26" : "Measurement timeout");
            s.sample++;
        } else if (page == PAGE_SCAN && (int32_t)(now - next_scan) >= 0) {
            wifi_scan_config_t cfg = { .show_hidden = true };
            esp_err_t err = esp_wifi_scan_start(&cfg, true);
            uint16_t count = 6, total = 0;
            wifi_ap_record_t records[6];
            if (err == ESP_OK) {
                esp_wifi_scan_get_ap_num(&total);
                err = esp_wifi_scan_get_ap_records(&count, records);
            }
            s.count = err == ESP_OK ? count : 0;
            for (int i = 0; i < s.count; ++i) {
                snprintf(s.networks[i], sizeof(s.networks[i]), "%.20s\nch %u  %d dBm  %s",
                         records[i].ssid[0] ? (char *)records[i].ssid : "(hidden)",
                         records[i].primary, records[i].rssi, security(records[i].authmode));
            }
            if (err == ESP_OK) snprintf(s.status, sizeof(s.status), "%u APs / strongest six", total);
            else snprintf(s.status, sizeof(s.status), "Scan: %s", esp_err_to_name(err));
            ESP_LOGI("dashboard", "SCAN count=%u result=%s", total, esp_err_to_name(err));
            next_scan = xTaskGetTickCount() + pdMS_TO_TICKS(5000);
        } else if (page == PAGE_STATUS) {
            if (!strlen(CONFIG_DASH_SSID)) {
                snprintf(s.status, sizeof(s.status), "No SSID configured\nUse menuconfig");
            } else if (xEventGroupGetBits(events) & HAS_IP) {
                wifi_ap_record_t ap;
                esp_netif_ip_info_t ip;
                if (esp_wifi_sta_get_ap_info(&ap) == ESP_OK &&
                    esp_netif_get_ip_info(netif, &ip) == ESP_OK) {
                    s.rssi = ap.rssi;
                    snprintf(s.status, sizeof(s.status), "%.32s\nch %u / %d dBm",
                             ap.ssid, ap.primary, ap.rssi);
                    snprintf(s.ip, sizeof(s.ip), IPSTR, IP2STR(&ip.ip));
                    if (!ping_pending && (int32_t)(now - next_ping) >= 0) {
                        esp_ping_config_t cfg = ESP_PING_DEFAULT_CONFIG();
                        cfg.count = 1;
                        cfg.timeout_ms = 800;
                        cfg.interval_ms = 1000;
                        IP_ADDR4(&cfg.target_addr, esp_ip4_addr1(&ip.gw), esp_ip4_addr2(&ip.gw),
                                 esp_ip4_addr3(&ip.gw), esp_ip4_addr4(&ip.gw));
                        esp_ping_callbacks_t callbacks = { .on_ping_end = ping_end };
                        esp_ping_handle_t h;
                        if (esp_ping_new_session(&cfg, &callbacks, &h) == ESP_OK) {
                            if (esp_ping_start(h) == ESP_OK) ping_pending = true;
                            else esp_ping_delete_session(h);
                        }
                        next_ping = now + pdMS_TO_TICKS(3000);
                    }
                    s.sample++;
                }
            } else {
                s.ip[0] = 0;
                s.rssi = 0;
                s.ping_ms = -2;
                snprintf(s.status, sizeof(s.status), "Connecting to\n%.32s", CONFIG_DASH_SSID);
                if ((int32_t)(now - next_connect) >= 0) {
                    esp_err_t err = esp_wifi_connect();
                    ESP_LOGI("dashboard", "CONNECT %s", esp_err_to_name(err));
                    next_connect = now + pdMS_TO_TICKS(10000);
                }
            }
        }
        xQueueOverwrite(radio_results, &s);
        vTaskDelay(pdMS_TO_TICKS(250));
    }
}

void radio_start(int initial_page)
{
    radio_pages = xQueueCreate(1, sizeof(int));
    radio_results = xQueueCreate(1, sizeof(radio_snapshot_t));
    energy_results = xQueueCreate(1, sizeof(int8_t));
    ping_results = xQueueCreate(1, sizeof(int));
    events = xEventGroupCreate();
    assert(radio_pages && radio_results && energy_results && ping_results && events);
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    netif = esp_netif_create_default_wifi_sta();
    assert(netif);
    assert(xTaskCreate(worker, "radio", 6144, (void *)(intptr_t)initial_page, 4, NULL) == pdPASS);
}

static void wifi_setup(void)
{
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&init));
    ESP_ERROR_CHECK(esp_wifi_set_storage(WIFI_STORAGE_RAM));
    ESP_ERROR_CHECK(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, wifi_event, NULL));
    ESP_ERROR_CHECK(esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP, wifi_event, NULL));
    wifi_config_t cfg = {0};
    _Static_assert(sizeof(CONFIG_DASH_SSID) <= 33, "SSID too long");
    _Static_assert(sizeof(CONFIG_DASH_PASSWORD) <= 64, "Use a passphrase up to 63 characters");
    memcpy(cfg.sta.ssid, CONFIG_DASH_SSID, strlen(CONFIG_DASH_SSID));
    memcpy(cfg.sta.password, CONFIG_DASH_PASSWORD, strlen(CONFIG_DASH_PASSWORD));
    cfg.sta.threshold.authmode = strlen(CONFIG_DASH_PASSWORD) ? WIFI_AUTH_WPA2_PSK : WIFI_AUTH_OPEN;
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &cfg));
}
