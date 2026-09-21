# ESP32-C6 WiFi implementation plan

Status (2026-09-21): the specimen runs on the board (joined a WPA2 network, five of five gateway
pings) and, in the emulator, the unmodified WiFi library initialises, scans all 14 channels and
reports `DISCONNECTED reason=201 NO_AP_FOUND` — what the board says when the network is absent.
That took three handshakes in `esp32c6/src/wifi.rs` (baseband channel switch and IQ estimate, the
MAC core's ready flag) and the existing `--stub bb_init=0`. Next is the MAC's receive path, so a
virtual access point's beacons reach the scan: the RX descriptor ring, the event and clear
registers and the interrupt source, found the same way — from what the library waits on.

The C6 emulator currently models the IEEE 802.15.4 MAC in `esp32c6/src/radio.rs`. Its WiFi
support is explicitly rejected by the CLI, and the C6 documentation says that WiFi 6 is not
modelled. The S3 implementation in `esp32s3/src/wifi.rs`, `esp32s3/src/periph.rs`, and
`esp32s3/src/bus.rs` is a useful protocol and virtual-network reference, but its register map
must not be copied to the C6: the C6 has a different RISC-V SoC and a different WiFi 6 MAC/PHY
integration.

## Target and scope

The target should be an unmodified C6 ESP-IDF station application that can:

1. scan and associate with an emulator-provided open access point;
2. complete WPA2-PSK association;
3. receive a DHCP lease and exchange ARP, ICMP, DNS, and NTP traffic;
4. optionally reach the host network through the existing user-mode NAT; and
5. run in the CLI and browser builds without disturbing the existing 802.15.4/Cooja path.

The first release should be a deterministic virtual AP, not an RF or PHY simulation. WiFi and
802.15.4 should initially use independent virtual media. Later work can connect their channel
occupancy and energy-detect models, since the current C6 radio already documents the overlap
between WiFi channels and 802.15.4 channels.

There is a useful fallback if the closed C6 WiFi firmware cannot be obtained or its hardware
contract is too large to recover: add an explicitly named application-level network shim or
virtual Ethernet interface. That can provide IP connectivity, but must not be presented as
unmodified `esp_wifi` support.

## Phase 0: obtain a C6 specimen and the hardware contract

Bring-up specimen: `examples/c6-wifi-station`. It scans, joins, takes a lease, pings the gateway
and reports the signal, one fixed-word console line per step, with the same state on the board's
screen as plain text (no LVGL) and a trace configuration without any display traffic.

Later demo: `examples/c6-radio-dashboard`, for the Waveshare C6-LCD-1.47.
It provides WiFi scan, station status with gateway ping/RSSI history, and a separate 802.15.4
energy page. ESP-IDF 5.5.4 and LVGL 8.4.0 are the initial build baseline. Default credentials
are empty; use a controlled AP and configure test credentials for HIL. The firmware build and
spectrum emulator smoke test do not constitute WiFi hardware validation. See its README for
build instructions and the board validation checklist.

Before implementing registers, collect a reproducible C6 WiFi station specimen:

- an ESP-IDF `wifi_station` image for the exact C6 IDF version and chip revision;
- the matching ROM/closed WiFi libraries and build configuration;
- UART logs for open and WPA2 networks;
- a real C6 board trace using JTAG/OpenOCD or GDB, including MMIO reads/writes, interrupt
  delivery, DMA descriptors, and reset/clock transitions; and
- a small `--regstat`/trace artifact checked into `docs` so future changes can be compared.

Set up the hardware-in-the-loop (HIL) rig at the same time: a C6 development board with
USB-JTAG and UART, a dedicated controllable access point (for example hostapd on a second WiFi
adapter), packet capture in monitor mode, and scripts that can reset the board, flash the test
image, collect logs, and save timestamps, channel, RSSI, and association results. Keep the AP
configuration fixed so an open-network run and a WPA2 run are reproducible.

Inventory the C6-specific MAC, WDEV, PHY/RF, PCR, clock, analog-I2C, coexistence, and interrupt
blocks. Record reset values, write-one-to-clear behavior, descriptor ownership bits, address
masking, RX metadata, TSF/timestamp semantics, and the exact polling loops used by the blobs.
Do not assume the S3 addresses at `0x60033000`, its descriptor layout, or its fake calibration
bits apply to the C6.

**Gate:** no C6 WiFi MMIO implementation should begin until a trace identifies the minimum
bring-up register set and the firmware image is pinned.

## Hardware-in-the-loop validation track

HIL is required for each protocol milestone, not just for the final release. Use the real C6 as
the behavioral oracle and the emulator as the deterministic implementation under test:

1. **Board baseline.** Run the exact station image on hardware against the controlled AP. Save
   UART logs, firmware hashes, AP configuration, packet captures, and reset-to-connected timing.
2. **Hardware trace.** During boot, scan, association, WPA2, DHCP, and a short ping/HTTPS flow,
   collect JTAG register/DMA snapshots at selected synchronization points. Capture interrupt
   causes, descriptor ownership, RX metadata, TSF values, and calibration polls. Avoid relying on
   continuous single-stepping, which changes WiFi timing.
3. **Replay fixtures.** Convert packet captures and selected register observations into compact
   fixtures used by unit and integration tests. A fixture must include the chip revision, IDF
   version, image hash, AP channel, and security mode.
4. **Emulator comparison.** Run the same unmodified image in the emulator with the corresponding
   virtual AP settings. Compare state transitions and externally visible behavior first (scan,
   auth, association, WPA2, DHCP, ping); compare register and descriptor traces where timing is
   expected to be equivalent. Allow documented timing differences caused by virtual scheduling.
5. **Fault-injection pass.** On hardware and in the emulator, vary beacon loss, delayed RX, bad
   MIC, replay counters, full RX rings, and AP restart. Confirm both implementations recover or
   fail in the same way at the station API level.

The first HIL setup should use the controlled physical AP as the network oracle. A host process
cannot make an in-process virtual AP appear as RF to a real board without a radio bridge, so a
direct “board connected to emulator AP over the air” loop is a later optional experiment. If that
is required, use a dedicated second radio or SDR/packet-injection bridge and keep it outside the
normal deterministic test path.

## Phase 1: isolate the SoC adapter

Add a C6-specific WiFi device, likely `esp32c6/src/wifi.rs`, and mount it from
`esp32c6/src/periph.rs`. Keep the existing `Ieee802154` device and its `ZB_MAC` interrupt
unchanged. Add only the C6 WiFi interrupt source(s), peripheral clock/reset behavior, and bus
tick/deadline hooks shown by the specimen.

The device should own:

- reset and calibration/status state;
- station MAC/BSSID/filter state;
- TX descriptor queues and completion/error events;
- RX descriptor-ring state and frame metadata;
- TSF/timing state; and
- debug counters and frame tracing.

Use a separate adapter for C6 register semantics even if protocol helpers are shared with S3.
Create a small common module only for representation-independent operations such as 802.11
frame parsing, FCS, frame construction, WPA2/CCMP, and Ethernet conversion. Do not make the C6
peripheral depend on S3 types or addresses.

## Phase 2: minimal open-network bring-up

Implement the smallest path that lets an unmodified station reach `CONNECTED`:

- reset, clock enable, and the PHY/RF calibration completion state that the C6 blob polls;
- MAC address and BSSID programming;
- scan results, beacons, probe responses, open-system authentication, and association;
- TX descriptor fetch and TX-done/error interrupts;
- RX ring traversal, ownership transitions, RX status metadata, and RX-data interrupts;
- frame filtering for the station address and BSSID; and
- TSF reads/latching and the timing needed for beacon and ACK handling.

Initially use one virtual AP and one station. Complete frames at scheduled deadlines rather than
polling the AP on every emulated CPU cycle. Preserve deterministic ordering when a beacon, TX
completion, RX frame, and 802.15.4 event become due together.

Add `--wifi` parsing for the C6 only after this phase passes. Keep the option rejected on C3 and
other unsupported targets. The C6 option should accept the same basic `ssid`, `chan`, `bssid`,
and `psk` syntax as S3 so test scripts can switch targets without changing their network setup.

## Phase 3: reuse the virtual AP and WPA2 implementation

Refactor the S3 virtual AP so its protocol logic is reusable without importing S3 peripheral
state. The shared layer should cover beacons, probe/auth/association frames, sequence numbers,
WPA2 four-way handshake, PTK/GTK derivation, MIC verification, and CCMP framing. The C6 adapter
will translate between those frames and its own RX/TX metadata.

Bring up an open network first, then WPA2-PSK with known test vectors. Keep WPA3/SAE, PMF,
roaming, power save, 802.11n/ax rates, and multiple stations out of the first milestone.

## Phase 4: IP networking and NAT

Reuse the S3 virtual-network behavior behind a chip-neutral Ethernet boundary:

- DHCP lease allocation;
- ARP and ICMP echo;
- DNS and SNTP responses; and
- optional user-mode NAT for TCP/UDP host access.

The C6 bus code should convert completed WiFi data frames to Ethernet payloads and feed them into
that common network module. NAT remains unavailable in browser builds where host sockets are not
available; the browser can still exercise association and the isolated virtual subnet.

Inbound port forwarding, multicast/mDNS, roaming, and real LAN bridging are follow-up features.

## Phase 5: coexistence and performance

Once station networking is correct, add optional shared-medium behavior:

- map WiFi channel occupancy into the C6 802.15.4 energy-detect scene;
- model busy/CCA and collisions only when both radios are enabled; and
- keep the default isolated AP mode for fast, deterministic tests.

Avoid adding work to ordinary C6 emulation when WiFi is disabled. Use event deadlines, bounded
descriptor walks, and zero-copy frame buffers where the guest descriptor permits it. Reuse parsed
frames and crypto buffers during the WPA2 exchange instead of rebuilding them in each bus tick.

## Tests and acceptance gates

Add tests in layers:

- register reset/read/write and interrupt-clear semantics;
- descriptor encode/decode, ownership, pointer masking, and RX metadata;
- TSF/timing and station-address/BSSID filters;
- frame/FCS and WPA2/CCMP vectors shared by S3 and C6;
- open-network and WPA2 integration with an unmodified C6 station image;
- HIL runs on a real C6 board with captured UART/JTAG/802.11 evidence;
- replay of hardware packet captures and selected descriptor/register observations;
- DHCP, ARP, ICMP, DNS, NTP, and NAT/HTTPS integration where supported; and
- regression tests proving existing C6 802.15.4/Cooja and S3 WiFi behavior is unchanged.

Milestones:

| Milestone | Exit condition |
| --- | --- |
| M0 | C6 specimen, firmware version, register trace, and minimum map documented |
| M1 | HIL harness runs the real C6 against a controlled AP and stores reproducible traces |
| M2 | C6 blob boots through WiFi reset/calibration without an interrupt storm in emulator and HIL |
| M3 | Unmodified firmware scans, authenticates, associates, and receives beacons on open AP |
| M4 | WPA2-PSK completes and encrypted data is accepted; hardware and emulator results agree |
| M5 | DHCP/DNS/ICMP/NTP pass in the virtual subnet and on the HIL board |
| M6 | NAT/HTTPS passes on native hosts; browser mode passes without host sockets |
| M7 | Optional 802.15.4 coexistence, performance counters, and user documentation |

The main technical risk is not the virtual AP; it is recovering the C6 WiFi 6 MAC/PHY firmware
contract. If M0 cannot produce a stable specimen and trace, stop before adding guessed registers
and implement the clearly labelled network shim fallback instead.
