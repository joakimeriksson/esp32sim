# EX237: networks around the station (`--wifi-nearby`)

Base: upstream/main `51a91540`; the firmware runs and the CPU comparison below were first made on
`9890ac55` and the checks repeated on `51a91540` ("Checks on main `51a91540`"). The final source
is the commit containing this receipt.
Inputs, hashes and the exact commands are in [inputs.json](inputs.json); the checks that ran
and their results in [checks.json](checks.json).

The virtual air carried one network, the access point of `--wifi`, so a scan always found
exactly one. `--wifi-nearby ssid=…,chan=…,rssi=…,secure=…[,bssid=…]` (repeatable, at most 32,
needs `--wifi`) adds beacon-only networks around the station:

- `BeaconAp` on the `VirtualAp`. A network around answers a wildcard probe request, or one for
  its SSID, with a probe response from its own BSSID after the access point's own response, 400 µs
  apart: timestamp, beacon interval, capability (ESS; Privacy when secured; Short Slot Time), then
  the SSID, Supported Rates, DSSS Parameter Set (its channel), Extended Supported Rates and, when
  secured, the RSN element. It answers nothing else, so an authentication to its BSSID goes
  unanswered and a join fails the way a network out of reach does. A network with the access
  point's own SSID is left out (`StationLink::set_beacons`).
- `VirtualAp::rx_meta`: a frame from a network around reaches the chip's RX descriptor with that
  network's channel and RSSI; the access point's frames keep its channel and -40 dBm. Applied on
  the S3, C3, C6 (RSSI; the library writes the C6's channel byte itself, as before) and the
  classic ESP32 (with its +96 RSSI offset).
- The spec parser `BeaconAp::parse` / `parse_all` and `ApConfig::parse` share one `parse_bssid`;
  a BSSID not given is derived from the SSID (FNV-1a, locally administered unicast), the same
  every run.

Nothing per tick: the networks matter only when a probe request arrives and when a frame is
delivered. Without the option nothing changes; the Wi-Fi goldens pass byte-identical.

## Public definitions

- IEEE 802.11-2020: 9.3.3.10 (Probe Response frame body: Timestamp, Beacon Interval, Capability
  Information, then elements), 9.4.1.4 (Capability Information: ESS bit 0, Privacy bit 4, Short
  Slot Time bit 10), Table 9-92 (element ids: SSID 0, Supported Rates 1, DSSS Parameter Set 3,
  RSN 48, Extended Supported Rates 50).
- ESP-IDF v5.5.5 `components/esp_wifi/include/local/esp_wifi_types_native.h` line 36 (`rssi`,
  signed, dBm) and lines 59-60 (`channel`, `secondary_channel`) of `wifi_pkt_rx_ctrl_t`; v4.4.8
  `components/esp_wifi/include/esp_wifi_types.h` lines 387 and 408-409: the fields the chips' RX
  descriptors carry and the scan reads.
- v5.5.5 `components/esp_wifi/include/esp_wifi_types_generic.h` lines 320-349 (`wifi_ap_record_t`:
  `bssid`, `ssid`, `primary`, `rssi`, `authmode`), v4.4.8 `esp_wifi_types.h` lines 191-210: what
  `WiFi.scanNetworks()` reports per network; `WIFI_AUTH_OPEN` / `WIFI_AUTH_WPA2_PSK` (v5.5.5 lines
  87 and 90) are what the station derives from the Privacy bit and the RSN element.
- The RX descriptor layouts themselves are unchanged from main (the S3 words are silicon-noted
  there; the C3 and C6 are inherited from the S3 model, the classic from its own driver notes).

No silicon measurement: a network around is a probe responder, not a modelled radio; nothing
about its frames was compared with hardware beyond what the existing access point already is.

## Tests

`esp-soc/src/wifi/tests.rs`: a wildcard probe gets the access point's response first and then
one per network around, each from its own BSSID with its channel, Privacy bit and RSN element,
spaced in time; a directed probe gets only the named network and an unknown SSID gets nothing;
`rx_meta` gives a network's channel and signal for its frames and the access point's otherwise;
authentication to a network around is unanswered and leaves the station Idle; spec parsing
(defaults, the derived BSSID stable across calls and distinct per SSID, every invalid key and
value, the 32 limit) and `set_beacons` dropping the access point's own SSID.

## Firmware

Arduino-ESP32 3.3.12 sketches (in this directory) on the ESP32-S3, from the mask ROM,
`--wifi ssid=Jumperly --net none` (commands in inputs.json):

| Run | Options | Console |
| --- | --- | --- |
| scan, 3 around | `--wifi-nearby ssid=Home,chan=1,rssi=-60 --wifi-nearby ssid=Cafe,chan=11,rssi=-75,secure=0 --wifi-nearby ssid=Office,chan=6,rssi=-82,bssid=02:00:00:00:00:42` | `scan: 4 network(s)`, `Jumperly rssi=-40 ch=6 open`, `Home rssi=-60 ch=1 secured`, `Cafe rssi=-75 ch=11 open`, `Office rssi=-82 ch=6 secured`, then DHCP on Jumperly as before |
| scan, control | none | `scan: 1 network(s)`, `Jumperly rssi=-40 ch=6 open` |
| join Home | `--wifi-nearby ssid=Home,chan=1 --debug wifi-frames`, 30 s | `not connected: status=6` after the sketch's 20 s. Frame log: 24 probe requests for `Home`, 30 probe responses from `02:4e:11:e0:7f:d8` (Home's derived BSSID) delivered to the station, 25 authentication frames from the station to that BSSID, no authentication response, the access point's own beacons throughout |

The emulator's own log names each network as configured:
`[emu] network around: 'Home' bssid 02:4e:11:e0:7f:d8 channel 1 -60 dBm (WPA2-PSK)`.

## CPU comparison

Against main `9890ac55` (the same tree, the same Rust 1.99.0 release build in a separate
worktree), the hello demos as CONTRIBUTING.md asks: S3 hello 3000 emulated seconds (board
none), C3 and C6 hello 30 seconds; child user CPU seconds via `GetProcessTimes` (Windows,
15.625 ms timer resolution); one warmup pair B→M, then seven measured pairs alternating M→B,
B→M; change is the ratio of medians. Every run of a workload retired the same instruction
count and produced the same console SHA-256 on both builds (`identical_work_and_console` in
[cpu.json](cpu.json)). No load gate, idle desktop. Pocket Tank was not run: neither the S3 core
nor its bus is touched, and none of this change's code runs without `--wifi`.

Nothing is flagged by the receipt rules (slower in at least 6/7 pairs, or non-overlapping
ranges): S3 hello −2.02% (slower 1/7), C3 hello −15.38% (1/7), C6 hello −2.29% (3/7).

### run 1, started 2026-10-15T18:51:09

| Workload | Main median (range) | PR median (range) | Change | PR slower in N/7 | Instructions | Console SHA-256 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| s3-hello | 33.2969 (32.0469–34.7812) | 32.6250 (31.1562–32.7344) | -2.02% | 1/7 | 1134713226 + 655106431 | `98de90e19cbd…` |
| c3-hello | 2.0312 (1.5156–2.1875) | 1.7188 (1.5000–1.9062) | -15.38% | 1/7 | 4800000000 | `3b9d8d3eb053…` |
| c6-hello | 2.7344 (2.4062–3.2812) | 2.6719 (2.0625–2.9844) | -2.29% | 3/7 | 4800000000 | `bd69fbb9ac5c…` |

#### s3-hello

| Pair | Order | Main s | PR s |
| --- | --- | ---: | ---: |
| warmup | B→M | 34.438 | 32.734 |
| pair1 | M→B | 34.469 | 32.734 |
| pair2 | B→M | 34.781 | 32.562 |
| pair3 | M→B | 33.297 | 31.156 |
| pair4 | B→M | 32.047 | 32.688 |
| pair5 | M→B | 32.422 | 31.859 |
| pair6 | B→M | 33.250 | 32.672 |
| pair7 | M→B | 33.703 | 32.625 |

#### c3-hello

| Pair | Order | Main s | PR s |
| --- | --- | ---: | ---: |
| warmup | B→M | 2.125 | 1.844 |
| pair1 | M→B | 1.891 | 1.641 |
| pair2 | B→M | 2.094 | 1.500 |
| pair3 | M→B | 2.172 | 1.906 |
| pair4 | B→M | 1.516 | 1.625 |
| pair5 | M→B | 2.188 | 1.859 |
| pair6 | B→M | 1.906 | 1.719 |
| pair7 | M→B | 2.031 | 1.750 |

#### c6-hello

| Pair | Order | Main s | PR s |
| --- | --- | ---: | ---: |
| warmup | B→M | 2.500 | 2.469 |
| pair1 | M→B | 3.281 | 2.359 |
| pair2 | B→M | 2.516 | 2.062 |
| pair3 | M→B | 2.734 | 2.781 |
| pair4 | B→M | 2.844 | 2.422 |
| pair5 | M→B | 2.688 | 2.984 |
| pair6 | B→M | 2.406 | 2.703 |
| pair7 | M→B | 2.750 | 2.672 |

## Limits

- A network around has no beacons of its own on the air, no data path and no authentication:
  it exists for scans and for the failure of a join. Hidden SSIDs, 5 GHz channels and WPA3 are
  not modelled; the RSN element is the access point's WPA2-PSK one.
- The firmware images are not committed and no golden pins them.

## Checks on main `51a91540`

The same change on main `51a91540`. Rust 1.99.1 (58cf8f9c3 2026-10-13), `cargo +1.99.1`,
Windows 11, ROMs from `tools/fetch-rom-elfs.sh roms`; entries marked `on_main` in
[checks.json](checks.json):

- Clippy native and wasm32 jit-tests, warnings denied: pass.
- `ESP32SIM_ROM_DIR=roms cargo test --release --workspace --no-fail-fast -- --include-ignored
  --skip external_`: 1020 passed, 2 failed, 0 ignored over 109 binaries; the failures are the two
  `decoder_matches_corpus` tests (Windows drive-letter path split), unrelated. The Wi-Fi tests
  above pass; no golden changed. Virtual-quantum scheduler (`ESP32SIM_VQ_NATIVE=1`, `machine`,
  `virtual_stops`): 43 passed.
- The wasm build and the eight standard demos, the evidence privacy check: pass.
- Firmware, the same images (`wifi-net.bin` `16e7673a…`, `wifi-home.bin` `4439d502…`) and
  commands: the scan prints `scan: 4 network(s)` with the same four lines, the control
  `scan: 1 network(s)`; the join prints `not connected: status=6`, and its frame log
  (`--debug wifi-frames`, 30 s) has 30 probe requests for `Home`, 30 probe responses from Home's
  derived BSSID delivered to the station, 30 authentication frames from the station to that
  BSSID and no authentication response, with the access point's beacons throughout.

## Review follow-up

- **One BSSID per network.** `BeaconAp::parse_all` refuses two networks around on one BSSID
  (one SSID given twice without `bssid`, or one `bssid` given twice): both would answer from that
  address. `StationLink::set_beacons` leaves out a network with the access point's SSID or BSSID,
  which would otherwise be that access point (with its BSSID, `rx_meta` would give every frame of
  the access point the network's channel and RSSI), and returns the ones left out with the reason;
  the CLI prints `[emu] --wifi-nearby '…' left out: it has the access point's BSSID` for those and
  `network around: …` only for the ones kept.
- **Probe addressing.** A network around answers a probe request sent to the broadcast address or
  its own, with a wildcard BSSID or its own (IEEE 802.11-2020 11.1.4.3.4), and no other.
- **Chip-side tests.** `wifi_rx_ctrl_carries_a_nearby_networks_channel_and_rssi` (S3, C3) and
  `classic_rx_ctrl_carries_a_nearby_networks_channel_and_rssi` deliver a frame from a network
  around and one from the access point and read the RX descriptor: the RSSI byte (the classic
  with its +96 offset) and the channel word. The C6 writes the RSSI only and is covered by
  firmware, as before.
- **Citations.** `wifi_ap_record_t` is v5.5.5 `esp_wifi_types_generic.h` lines 320-349 and v4.4.8
  `esp_wifi_types.h` lines 191-210.

Tests: `nearby_networks_need_a_bssid_each`, `a_nearby_network_answers_only_probes_addressed_to_it`
and the access point's BSSID case in the set_beacons test; each fails with its rule removed.
Workspace (`--include-ignored --skip external_`, main `20117b08` merged): 1044 passed, the same
two `decoder_matches_corpus` failures; virtual-quantum tests 43; Clippy native and wasm, the wasm
build and demos, the privacy check: pass. The firmware runs above give the same scan, control and
join results (30 / 30 / 30 / 0 frames).
