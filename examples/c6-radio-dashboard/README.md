# C6 radio dashboard

Firmware for the Waveshare ESP32-C6-LCD-1.47 (172×320 ST7789, BOOT on GPIO9).
Press BOOT to cycle between three pages:

1. **WiFi networks:** strongest six APs, SSID, channel, RSSI, and security.
   Refreshes five seconds after each scan. Long SSIDs are shortened on screen.
2. **Connection:** joins the configured network, displays IP/channel/RSSI, graphs RSSI,
   and pings the DHCP gateway every three seconds. Some gateways do not answer ICMP.
3. **Radio energy:** measures IEEE 802.15.4 channels 11–26, with dBm bars.

WiFi stops before energy measurements begin. Leaving spectrum stops 802.15.4 before
starting WiFi. Entering scan disconnects the station; returning to connection reconnects.
Radio changes can take a few seconds while the current scan finishes.

## Build and configure

Use ESP-IDF **5.5.4**, target **esp32c6**, and LVGL **8.4.0** (pinned in the manifest).

```sh
. ~/esp/esp-idf-v5.5.4/export.sh
cd examples/c6-radio-dashboard
idf.py set-target esp32c6
idf.py menuconfig
idf.py build
```

Under **Radio dashboard**, enter the station SSID and passphrase. Empty SSID means scan only;
empty passphrase selects an open network. Credentials live in the ignored sdkconfig and compiled
firmware; use dedicated test credentials for shared HIL artifacts.

Once the intended board and serial port are identified:

```sh
idf.py -p /dev/cu.usbmodem101 flash monitor
```

Flashing replaces the board's application. Use the actual port for your board.

## Spectrum-first build and emulator

The default starts with WiFi scanning, which the emulator runs when given an access point
(`--wifi ssid=esp32sim,psk=esp32sim-pass`): the scan page logs `dashboard: SCAN count=1`. For
display/energy regression checks that do not depend on WiFi, use a separate spectrum-first build:

```sh
idf.py -B build-spectrum -DSDKCONFIG=sdkconfig.spectrum \
  '-DSDKCONFIG_DEFAULTS=sdkconfig.defaults;sdkconfig.spectrum.defaults' build
```

From the repository root:

```sh
B=examples/c6-radio-dashboard/build-spectrum
target/release/esp32sim-c6 --boot rom --flash-mb 4 --board waveshare-c6-lcd147 \
  --bootloader "$B/bootloader/bootloader.bin" \
  --ptable "$B/partition_table/partition-table.bin" \
  --app "$B/radio_dashboard.bin" --elf "$B/radio_dashboard.elf" \
  --stub bb_init=0 --max-seconds 8 --tft-png /tmp/dashboard.png
```

The existing spectrum path requires the calibration stub above; this is a display/energy
smoke test, not unmodified WiFi validation. `examples/c6-wifi-station` is the specimen for that
(scan, WPA2 join, lease and pings in the emulator). The connection page here has not been run in
the emulator; it needs a build configured for the emulator's network.

For an offline build using an existing LVGL checkout, disable the component manager and supply
that component directory explicitly (normal builds download the pinned dependency):

```sh
IDF_COMPONENT_MANAGER=0 idf.py \
  -DEXTRA_COMPONENT_DIRS="$HOME/work/esp32/energy_scan/managed_components/lvgl__lvgl" build
```

## Hardware validation

Use the same ELF/bin and configuration hashes for paired hardware/emulator runs.

- Verify display colors, all 16 channel rows, BOOT debounce, and repeated page cycling.
- Check scan SSIDs/channel/security/RSSI against the controlled AP.
- Join open and WPA2 test networks; verify GOT_IP, ping and RSSI history.
- Restart the AP and verify reconnection; try incorrect credentials and an absent AP.
- Cycle scan → connection → spectrum repeatedly; check crashes, stale data and heap loss.
- Capture USB console logs (MODE, SCAN, CONNECT, GOT_IP, DISCONNECTED), AP logs and packets.

Hardware WiFi and radio switching require real-board testing before claiming HIL success.

## Source provenance

LCD_Driver and LVGL_Driver were copied from the existing local
`~/work/esp32/energy_scan/main` project (joakimeriksson/esp32, energy_scan), retaining its board
initialization and pin mapping. Dashboard UI and radio control are new. The original scanner
project is unchanged. Only the UI task calls LVGL; ISR results and radio snapshots pass through
FreeRTOS queues. No esp_lvgl_port task or second LVGL tick source is started.
