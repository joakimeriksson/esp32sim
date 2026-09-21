// A WiFi station and nothing else: scan, join, take a lease, ping the gateway, then report the
// signal. Every step is one console line starting with a fixed word (SCAN, CONNECT, CONNECTED,
// DISCONNECTED, GOT_IP, PING, STATUS), so a run on the board and a run in the emulator can be
// compared as text. The same steps go to the board's screen.
#include <string.h>
#include "sdkconfig.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/event_groups.h"
#include "esp_event.h"
#include "esp_log.h"
#include "esp_mac.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#include "nvs_flash.h"
#include "ping/ping_sock.h"
#include "lcd_text.h"

#define TAG "station"
#define SCAN_MAX 10

enum { ROW_TITLE, ROW_AP, ROW_STATE, ROW_IP, ROW_GW, ROW_RSSI, ROW_PING, ROW_SCAN, ROW_AP1, ROW_AP2 };
#define GOT_IP    BIT0
#define RETRY     BIT1
#define PING_DONE BIT2

static EventGroupHandle_t events;
static esp_netif_t *netif;
static int attempts;

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
    default: return "OTHER";
    }
}

// The reasons a bring-up actually meets; the number is always printed as well.
static const char *reason_name(int reason)
{
    switch (reason) {
    case WIFI_REASON_AUTH_EXPIRE: return "AUTH_EXPIRE";
    case WIFI_REASON_ASSOC_LEAVE: return "ASSOC_LEAVE";
    case WIFI_REASON_4WAY_HANDSHAKE_TIMEOUT: return "4WAY_TIMEOUT";
    case WIFI_REASON_BEACON_TIMEOUT: return "BEACON_TIMEOUT";
    case WIFI_REASON_NO_AP_FOUND: return "NO_AP_FOUND";
    case WIFI_REASON_AUTH_FAIL: return "AUTH_FAIL";
    case WIFI_REASON_ASSOC_FAIL: return "ASSOC_FAIL";
    case WIFI_REASON_HANDSHAKE_TIMEOUT: return "HANDSHAKE_TIMEOUT";
    case WIFI_REASON_CONNECTION_FAIL: return "CONNECTION_FAIL";
    case WIFI_REASON_NO_AP_FOUND_W_COMPATIBLE_SECURITY: return "NO_AP_SECURITY";
    default: return "";
    }
}

static void on_event(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    if (base == WIFI_EVENT && id == WIFI_EVENT_STA_CONNECTED) {
        wifi_event_sta_connected_t *e = data;
        ESP_LOGI(TAG, "CONNECTED channel=%u bssid=" MACSTR, e->channel, MAC2STR(e->bssid));
        lcd_line(ROW_STATE, LCD_AMBER, "ASSOCIATED CH %u", e->channel);
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) {
        wifi_event_sta_disconnected_t *e = data;
        ESP_LOGI(TAG, "DISCONNECTED reason=%u %s", e->reason, reason_name(e->reason));
        lcd_line(ROW_STATE, LCD_RED, "DISC %u %s", e->reason, reason_name(e->reason));
        lcd_line(ROW_IP, LCD_GREY, "IP --");
        lcd_line(ROW_GW, LCD_GREY, "GW --");
        xEventGroupClearBits(events, GOT_IP);
        xEventGroupSetBits(events, RETRY);
    } else if (base == IP_EVENT && id == IP_EVENT_STA_GOT_IP) {
        ip_event_got_ip_t *e = data;
        ESP_LOGI(TAG, "GOT_IP ip=" IPSTR " mask=" IPSTR " gw=" IPSTR,
                 IP2STR(&e->ip_info.ip), IP2STR(&e->ip_info.netmask), IP2STR(&e->ip_info.gw));
        lcd_line(ROW_STATE, LCD_GREEN, "CONNECTED");
        lcd_line(ROW_IP, LCD_WHITE, "IP " IPSTR, IP2STR(&e->ip_info.ip));
        lcd_line(ROW_GW, LCD_WHITE, "GW " IPSTR, IP2STR(&e->ip_info.gw));
        xEventGroupSetBits(events, GOT_IP);
    }
}

static void scan(void)
{
    lcd_line(ROW_STATE, LCD_AMBER, "SCANNING...");
    wifi_scan_config_t config = { .show_hidden = true };
    esp_err_t err = esp_wifi_scan_start(&config, true);
    uint16_t total = 0, count = SCAN_MAX;
    static wifi_ap_record_t found[SCAN_MAX];
    if (err == ESP_OK) err = esp_wifi_scan_get_ap_num(&total);
    if (err == ESP_OK) err = esp_wifi_scan_get_ap_records(&count, found);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "SCAN failed: %s", esp_err_to_name(err));
        lcd_line(ROW_SCAN, LCD_RED, "SCAN %s", esp_err_to_name(err));
        return;
    }
    ESP_LOGI(TAG, "SCAN found=%u", total);
    for (int i = 0; i < count; ++i) {                                // strongest first
        const char *name = found[i].ssid[0] ? (const char *)found[i].ssid : "(hidden)";
        ESP_LOGI(TAG, "SCAN %d ssid=\"%s\" channel=%u rssi=%d auth=%s bssid=" MACSTR, i + 1, name,
                 found[i].primary, found[i].rssi, security(found[i].authmode), MAC2STR(found[i].bssid));
        if (i < 2) lcd_line(ROW_AP1 + i, LCD_GREY, "%d %2u %.12s", found[i].rssi, found[i].primary, name);
    }
    lcd_line(ROW_SCAN, LCD_WHITE, "SCAN %u APs", total);
}

static void on_ping_reply(esp_ping_handle_t h, void *arg)
{
    uint16_t seq = 0;
    uint32_t ms = 0;
    esp_ping_get_profile(h, ESP_PING_PROF_SEQNO, &seq, sizeof(seq));
    esp_ping_get_profile(h, ESP_PING_PROF_TIMEGAP, &ms, sizeof(ms));
    ESP_LOGI(TAG, "PING seq=%u time=%u ms", seq, (unsigned)ms);
    lcd_line(ROW_PING, LCD_GREEN, "PING %u ms  %u/%d", (unsigned)ms, seq, CONFIG_STATION_PING_COUNT);
}

static void on_ping_timeout(esp_ping_handle_t h, void *arg)
{
    uint16_t seq = 0;
    esp_ping_get_profile(h, ESP_PING_PROF_SEQNO, &seq, sizeof(seq));
    ESP_LOGI(TAG, "PING seq=%u timeout", seq);
    lcd_line(ROW_PING, LCD_RED, "PING timeout %u/%d", seq, CONFIG_STATION_PING_COUNT);
}

static void on_ping_end(esp_ping_handle_t h, void *arg)
{
    uint32_t sent = 0, received = 0;
    esp_ping_get_profile(h, ESP_PING_PROF_REQUEST, &sent, sizeof(sent));
    esp_ping_get_profile(h, ESP_PING_PROF_REPLY, &received, sizeof(received));
    ESP_LOGI(TAG, "PING done sent=%u received=%u", (unsigned)sent, (unsigned)received);
    esp_ping_delete_session(h);
    xEventGroupSetBits(events, PING_DONE);
}

static void ping_gateway(void)
{
    esp_netif_ip_info_t ip;
    if (CONFIG_STATION_PING_COUNT == 0 || esp_netif_get_ip_info(netif, &ip) != ESP_OK) return;
    esp_ping_config_t config = ESP_PING_DEFAULT_CONFIG();
    config.count = CONFIG_STATION_PING_COUNT;
    config.interval_ms = 1000;
    config.timeout_ms = 1000;
    IP_ADDR4(&config.target_addr, esp_ip4_addr1(&ip.gw), esp_ip4_addr2(&ip.gw),
             esp_ip4_addr3(&ip.gw), esp_ip4_addr4(&ip.gw));
    esp_ping_callbacks_t callbacks = { .on_ping_success = on_ping_reply,
                                       .on_ping_timeout = on_ping_timeout, .on_ping_end = on_ping_end };
    esp_ping_handle_t session;
    esp_err_t err = esp_ping_new_session(&config, &callbacks, &session);
    if (err == ESP_OK) err = esp_ping_start(session);
    if (err != ESP_OK) ESP_LOGE(TAG, "PING could not start: %s", esp_err_to_name(err));
}

static void connect(void)
{
    ++attempts;
    esp_err_t err = esp_wifi_connect();
    ESP_LOGI(TAG, "CONNECT attempt=%d ssid=\"%s\" %s", attempts, CONFIG_STATION_SSID, esp_err_to_name(err));
    lcd_line(ROW_STATE, LCD_AMBER, "CONNECTING #%d", attempts);
}

void app_main(void)
{
    esp_err_t err = nvs_flash_init();
    if (err == ESP_ERR_NVS_NO_FREE_PAGES || err == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_ERROR_CHECK(nvs_flash_erase());
        err = nvs_flash_init();
    }
    ESP_ERROR_CHECK(err);

    lcd_text_init();                                                 // the screen is up before the radio
    lcd_line(ROW_TITLE, LCD_CYAN, "C6 WIFI STATION");
    lcd_line(ROW_AP, LCD_WHITE, "AP %.17s", strlen(CONFIG_STATION_SSID) ? CONFIG_STATION_SSID : "(scan only)");
    lcd_line(ROW_STATE, LCD_AMBER, "WIFI INIT...");

    events = xEventGroupCreate();
    assert(events);
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    netif = esp_netif_create_default_wifi_sta();
    assert(netif);
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&init));
    ESP_ERROR_CHECK(esp_wifi_set_storage(WIFI_STORAGE_RAM));
    ESP_ERROR_CHECK(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, on_event, NULL));
    ESP_ERROR_CHECK(esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP, on_event, NULL));

    wifi_config_t config = { 0 };
    _Static_assert(sizeof(CONFIG_STATION_SSID) <= 33, "an SSID is at most 32 bytes");
    _Static_assert(sizeof(CONFIG_STATION_PASSWORD) <= 64, "a passphrase is at most 63 characters");
    memcpy(config.sta.ssid, CONFIG_STATION_SSID, strlen(CONFIG_STATION_SSID));
    memcpy(config.sta.password, CONFIG_STATION_PASSWORD, strlen(CONFIG_STATION_PASSWORD));
    config.sta.threshold.authmode = strlen(CONFIG_STATION_PASSWORD) ? WIFI_AUTH_WPA2_PSK : WIFI_AUTH_OPEN;
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &config));
    ESP_ERROR_CHECK(esp_wifi_start());
    ESP_LOGI(TAG, "STARTED");

    scan();
    if (!strlen(CONFIG_STATION_SSID)) {
        lcd_line(ROW_STATE, LCD_GREY, "NO SSID SET");
        for (;;) { vTaskDelay(pdMS_TO_TICKS(10000)); scan(); }
    }

    connect();
    bool pinged = false;
    for (;;) {
        EventBits_t bits = xEventGroupWaitBits(events, RETRY, pdTRUE, pdFALSE, pdMS_TO_TICKS(5000));
        if (bits & RETRY) {
            pinged = false;
            vTaskDelay(pdMS_TO_TICKS(2000));
            connect();
            continue;
        }
        if (!(bits & GOT_IP)) continue;
        if (!pinged) { pinged = true; ping_gateway(); }
        wifi_ap_record_t ap;
        if (esp_wifi_sta_get_ap_info(&ap) == ESP_OK) {
            ESP_LOGI(TAG, "STATUS rssi=%d channel=%u", ap.rssi, ap.primary);
            lcd_line(ROW_RSSI, LCD_WHITE, "RSSI %d dBm CH %u", ap.rssi, ap.primary);
        }
    }
}
