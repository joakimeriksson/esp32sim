# Web UI and protocol

`--web PORT` serves `web/index.html` (no build step, no dependencies) and a WebSocket on the
same port. The page shows the board (Atech: 14-port drawing with knob, ring, buttons,
speaker VU, the TFT in its physical orientation plus a readable copy; Waveshare: camera
panel with picture upload / webcam and speaker meter; bare: console only), the USB-CDC and
UART0 consoles, an action box for the SDK's JSON protocol, and audio through WebAudio
(click 🔇 once — browsers require a user gesture).

The header shows emulated time, instructions, frames, and `real time` / `⚠ N s behind` /
resync count. The audio buffer is adaptive: it starts at 60 ms and grows on underrun (up to
400 ms) so a busy firmware phase (a full display redraw) does not produce gaps.

## Emulator → browser

Text frames (JSON):

| `t` | Fields | When |
| --- | --- | --- |
| `board` | `name` | on connect; the page switches layout |
| `serial` | `src` (`usb`/`uart0`), `data` | console output (ANSI colours stripped by the page) |
| `ring` | `leds` `[[r,g,b]…]` | ring changed |
| `stat` | `time`, `insns`, `frames`, `behind`, `resyncs`, `cam`, `gpio_in` | every 20 ms emulated |

Binary frames (first byte = type):

| Type | Payload |
| --- | --- |
| 1 | TFT frame: `w u16le, h u16le`, RGB565 pixels (160×80) — quiet-push boards defer at most one push interval; other boards send when changed |
| — | `{"t":"emu","msg":…}` — a line from the emulator itself (wasm build: stubs, chip resets, load errors), shown in the console |
| 2 | audio: `[rate u32 le]` then int16le mono samples; the rate is what the firmware programmed the I2S clock to (44.1 kHz Atech, 24 kHz autopling, 22.05 kHz the panel's SID player) and can change between chunks |
| 4 | camera preview: `w u16le, h u16le`, RGB888 (320×240) |

On quiet-push boards, continuous redraws are published on every other push opportunity rather
than waiting indefinitely for silence. This keeps drawing visible but can expose a partially
updated frame. It is a publication policy, not a model of physical panel scanout.

A late-joining tab gets a snapshot (console backlog, last frame, ring, preview) on connect.

## Browser → emulator

Text frames:

| `t` | Fields |
| --- | --- |
| `btn` | `pin`, `v` (1 = pressed) |
| `knobpress` | `v` |
| `knob` | `d` (+1 cw / −1 ccw per detent); the emulator queues the quadrature edges 2 ms apart |
| `serial` | `line` — sent to the USB-CDC RX with a newline |
| `gpio` | `pin`, `level` |
| `touch` | `x`, `y`, `down` (1 = touching) |

Binary frame type 3: camera picture, `w u16le, h u16le`, RGBA8888 — used by the picture
upload and the webcam (4 fps). Frames up to 8 MB are accepted.

## Sending is never blocking

Each client has a writer thread with a bounded queue; when a tab is frozen or slow, frames
are dropped for that client and the emulator keeps running at real time.
