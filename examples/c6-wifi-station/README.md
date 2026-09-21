# C6 WiFi station

The smallest firmware that exercises WiFi end to end on an ESP32-C6: scan, join, take a DHCP
lease, ping the gateway, then report the signal. It is the bring-up specimen for C6 WiFi in the
emulator (`docs/wifi-c6-plan.md`): the same binary runs on the board and in `esp32sim-c6`, and
the two console logs are compared.

Every step is one console line that starts with a fixed word (the values here are examples, not a captured run):

    station: STARTED
    station: SCAN found=7
    station: SCAN 1 ssid="home" channel=6 rssi=-48 auth=WPA2 bssid=aa:bb:cc:dd:ee:ff
    station: CONNECT attempt=1 ssid="home" ESP_OK
    station: CONNECTED channel=6 bssid=aa:bb:cc:dd:ee:ff
    station: GOT_IP ip=192.168.1.23 mask=255.255.255.0 gw=192.168.1.1
    station: PING seq=1 time=4 ms
    station: PING done sent=5 received=5
    station: STATUS state=connected rssi=-49 channel=6
    station: DISCONNECTED reason=201 NO_AP_FOUND

A disconnect is retried after two seconds, forever. `STATUS` repeats every five seconds whatever
the state (`connecting`, `associated_no_ip`, `connected`, `disconnected`), so the log is never
silent: a network that associates but withholds DHCP shows as `associated_no_ip`.

## The screen

On the Waveshare ESP32-C6-LCD-1.47 the same state is shown as 20 x 10 characters in landscape:
the network, the state (with the disconnect reason and its number), IP, gateway, signal, the last
ping, and the two strongest networks of the scan. There is no graphics library: a doubled 8 x 8
font, one SPI transfer per changed row, nothing at all while the text is unchanged. The panel is up
before the radio starts, so a failure in WiFi bring-up still leaves `WIFI INIT...` readable.

`CONFIG_STATION_LCD_FLIP` turns the text 180 degrees. `CONFIG_STATION_LCD=n` removes the display
code from the run entirely, which is what a register trace wants.

## Build

ESP-IDF **5.5.4**:

```sh
. ~/esp/esp-idf-v5.5.4/export.sh
cd examples/c6-wifi-station
idf.py menuconfig        # "C6 WiFi station": your network's SSID and passphrase
idf.py build
idf.py -p /dev/cu.usbmodem101 flash monitor
```

The defaults (`esp32sim` / `esp32sim-pass`) are the emulator's virtual access point, the same as
`examples/wifi-station` uses on the S3. Your own credentials go into the ignored `sdkconfig`, and
into the binary: do not share a build made with real ones. An empty SSID scans every ten seconds
and never joins; an empty passphrase joins an open network.

The trace build has no display traffic and debug logging from the WiFi and WPA libraries:

```sh
idf.py -B build-trace -DSDKCONFIG=sdkconfig.trace \
  '-DSDKCONFIG_DEFAULTS=sdkconfig.defaults;sdkconfig.trace.defaults' build
```

## In the emulator

```sh
B=examples/c6-wifi-station/build
target/release/esp32sim-c6 --boot rom --flash-mb 4 --board waveshare-c6-lcd147 --console usb \
  --bootloader $B/bootloader/bootloader.bin --ptable $B/partition_table/partition-table.bin \
  --app $B/c6_wifi_station.bin --elf $B/c6_wifi_station.elf --max-seconds 6 --tft-png /tmp/station.png
```

The C6 has no WiFi model yet. Today the screen comes up, `esp_wifi_init` succeeds, and
`esp_wifi_start` stops in the PHY's full calibration:

    phy_init: failed to load RF calibration data (0x1102), falling back to full calibration
    phy: error: chgp_cal no done!!!

with the core spinning in `txdc_cal_new`. That is where the emulator work starts. (The PNG is the
panel's native portrait scan, so the landscape text is sideways in it.)

## Provenance

`Vernon_ST7789T/` is Espressif's ST7789T panel driver (Apache-2.0), as shipped in Waveshare's demo
for this board. `font8x8_basic.h` is Daniel Hepper's public-domain 8 x 8 font
(github.com/dhepper/font8x8). The pin numbers are the board's schematic. Everything else is new.
