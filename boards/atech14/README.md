# The Atech 14-port board: the Pocket Synth firmware

The firmware the Atech Hardware Platform generated for the "Pocket Synth" (`firmware/src/main.cpp`)
plus the audio work below, built with PlatformIO against the real SDK drivers. It runs unmodified
in the esp32sim emulator (the `atech14` board model: TFT, knob + LED ring, buttons, USB console,
audio to WAV or the browser) and on the board.

```sh
make hw         # build firmware/.pio/build/hw/firmware.bin
make flash      # flash the real board
make test       # the emulator's golden tests for this firmware (console, audio, instruction count)
```

## Board (Atech 14-port, read from the device + SDK catalog)

| | |
|---|---|
| MCU | ESP32-S3R2 (2 MB PSRAM), 8 MB flash, native USB-CDC console → `/dev/cu.usbmodem1101` |
| Speaker MAX98357A (I2S) | ports 5+6 → BCLK 12, LRCLK 13, DIN 10 |
| ST7735 TFT 160×80 | ports 9+10 → SCLK 2, CS 41, MOSI 1, DC 40 |
| Rotary encoder + 12-LED ring | ports 1+2 → CLK 5, DT 4, SW 9, ring 8 |
| Button 1 (play) / Button 2 (waveform) | port 3 → GPIO 17 / port 4 → GPIO 16 (active low) |

## Layout

```
firmware/
  src/main.cpp.generated    what the Atech Hardware Platform emitted — verbatim, never edited
  src/main.cpp              that file plus the audio work below (SID chip engine, SID jukebox)
  lib/atech_*/              REAL drivers copied from the atech SDK (tools/sync-sdk-modules.sh)
  lib/sid/                  3-voice 6581-style SID chip core (ADSR, filter) driving the synth
  lib/crsid/                cRSID by Hermit (WTFPL): a whole C64 — 6502, SID, CIA, VIC — so the
                            board can play real .sid tunes; same engine the esp32-screen panel uses
  lib/sidtunes/             four HVSC tunes embedded as C arrays (PlatformIO has no EMBED_FILES)
  src/modules/…             glue the hosted platform includes but the SDK doesn't ship:
                            AtechSerial (SDK wire protocol), atech_helpers.h, forwarding headers
  platformio.ini            env:hw (the board)
script1.txt                 the scripted scenario the emulator's golden test runs (buttons, knob, serial)
regression.wav              that scenario's audio, bit-exact per run — the regression fixture
tools/sync-sdk-modules.sh   refresh drivers after `uv pip install -U atech`
examples/idf-minimal/       bare ESP-IDF sample (pre-Atech)
```

## Setup

The Atech SDK hardware modules (`firmware/lib/atech_*`) are not part of this repository; fetch
them from the `atech` package with `make sync-sdk` before building the firmware.

```sh
make sdk                     # .venv with the atech SDK (drivers + `atech` CLI)
make hw
```

## The protocol

Events use the SDK envelope, e.g.
`{"type":"event","payload":{"event_type":"state","key":"note_triggered","value":"C4",…}}`,
and actions are sent as `{"action":"set_note","value":"5"}` — what `atech send` / `atech monitor`
speak to the real board, what the emulator's `serial` script command and the web page's console
send, and what the goldens compare.

## Hardware

```sh
make flash                   # env:hw — real I2S + ST7735
make monitor
make check                   # atech check: reboot + module health report
make send KEY=set_note VALUE=5
```

"Resource busy" on the port → a browser tab (Web Serial) or monitor holds it:
`lsof /dev/cu.usbmodem1101`, or `.venv/bin/atech free-port`.

## SID jukebox (real .sid tunes)

The Pocket Synth drives a SID *chip* model (`lib/sid`). The jukebox goes a step further and runs
**cRSID** (`lib/crsid`) — a complete emulated C64: the 6502 executes the tune's own machine code,
which writes the SID registers 50 times a second, exactly as it did in 1985. Four tunes are
embedded (Commando · Rob Hubbard, Wizball · Martin Galway, Irish Dream and On the Edge), and the
titles and authors on screen are read from each file's PSID header at run time.

| Control | Action |
| --- | --- |
| **knob press** (in the synth) | start the player |
| hold the **knob** 1.5 s | stop, back to the synth |
| **button 1** | next tune |
| **button 2** | next subtune (a one-subtune file restarts) |
| **encoder** | volume, 5 % per detent; the LED ring is the dial, cyan |
| **knob press** | mute / unmute (ring turns red; the tune keeps running underneath) |
| serial JSON | `{"action":"play_sid","value":"0"}`, `{"action":"next_sid"}`, `{"action":"sid_subtune","value":"3"}` (empty = next), `{"action":"set_volume","value":"0.5"}`, `{"action":"mute","value":"1"}` (empty = toggle), `{"action":"stop_sid"}`; diagnostics: `{"action":"pin_probe","value":"9"}` samples a GPIO 2000× over 100 ms and reports the lows |

The screen shows title and author from the PSID header, `tune/count  sub n/m`, and the volume bar
(or a MUTE badge). The header reads SID PLAYER while a tune plays. Changes are posted as state events
(`sid_tune`, `volume`, `mute`) on the serial protocol, so the emulator's console shows them.

Try it in the emulator (the script drives the serial protocol, so no clicking):

```sh
printf '3.0 serial {"action":"play_sid","value":"0"}\n16.0 stop\n' > /tmp/sid.txt
./target/release/esp32sim --boot rom --bootloader $B/bootloader.bin --ptable $B/partitions.bin \
    --app $B/firmware.bin --elf $B/firmware.elf --script /tmp/sid.txt --wav commando.wav --max-seconds 16
```

Two things this board forced that the panel did not:

- **No PSRAM.** The emulated C64 is ~270 KB (64 KB RAM, both 64 KB IO banks, 64 KB ROM banks) and
  this board has 360 KB of internal heap and no PSRAM at all, so `cRSID_init` now falls back to
  internal RAM — and the firmware allocates the C64 when playback starts and frees it on stop
  (`cRSID_free`), so the synth and the WiFi stack get the memory back. Free heap goes 349 KB →
  37 KB while a tune plays and straight back afterwards.
- **44.1 kHz, not 22.05.** It feeds the same `Speaker` the synth uses, so no resampling. Rendering
  costs about a quarter of one core, and `Speaker::writeSamples` blocking on the I2S DMA is what
  paces playback.

## Testing

The emulator two directories up runs this firmware from the mask ROM through the bootloader and
holds it to golden outputs (`tests/golden/atech-*`): `script1.txt` presses the buttons, turns the
knob and sends a serial command; the audio it produces must match `regression.wav` byte for byte,
the console text and the instruction count must match too, and the same run through the block
interpreter without the JIT must agree. `make test` runs those. After an intentional firmware
change, re-baseline with `UPDATE_GOLDENS=1` and copy the new WAV over `regression.wav`.

Any input can be scripted (`docs/cli.md`, action scripts) and any run can capture the TFT
(`--tft-png`), so a new behaviour is verified the way the SID player's controls were: a script
that exercises every control, the console events it prints, and a screenshot.
