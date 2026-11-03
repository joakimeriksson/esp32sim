# EX235: GP-SPI3 (HSPI) on the ESP32-S3

Base: upstream/main `af9ebc79`; the firmware run and the CPU comparison below were first made on
`f5a038bf` and the checks repeated on `af9ebc79` ("Checks on main `af9ebc79`"). The final source
is the commit containing this receipt.
Inputs, hashes and the exact commands are in [inputs.json](inputs.json); the checks that ran
and their results in [checks.json](checks.json).

The S3 has two general-purpose SPI masters. Upstream models SPI2 (Arduino `SPI` / FSPI); SPI3,
which Arduino's `SPIClass(HSPI)` and ESP-IDF's `SPI3_HOST` use for a second bus (an SD card
next to a display is the usual case), had no device: its register block read as nothing, so
firmware waited for a USR bit that never cleared. This adds SPI3 as a second instance of the
same `GpSpi` model:

- `Peripherals::spi3` at DR_REG_SPI3_BASE 0x60025000 on ETS_SPI3_INTR_SOURCE 22, in the
  `device_set!` table next to SPI2.
- The SPI2 bus paths take the controller as `host` 2 or 3: CPU-FIFO transfers reach the board
  model as `spi_transfer(3, …)` / `spi_transfer_pins(3, …)`; the DMA data phase comes from the
  GDMA OUT channel bound to trigger 1 (SOC_GDMA_TRIG_PERIPH_SPI3), committed at once (the
  experimental wire timing stays SPI2's); descriptor faults land in `spi3_dma_fault`.
- Pin-aware boards get SPI3's routes through the GPIO matrix only (SPI3 has no IO_MUX pins on
  the S3): SPI3_CLK 66, SPI3_Q 67, SPI3_D 68, SPI3_CS0..2 71, 72, 127, through the existing
  `spi_pins` with explicit CS signals; the controller's CS3..5 enables select nothing.
- The mmio-s1 write shortcut covers 0x60025 like 0x60024; a pending SPI3 transfer holds the
  active tick cadence like SPI2's.
- `GpSpi` gains a `host` number for its log lines (`[spi3] transfer …` under `--debug spi`);
  the C3, C6 and classic controllers keep 2.

Cost on the idle path: `tick_impl` gains the SPI3 calls next to SPI2's (`complete_spi3_dma`,
`deliver_spi3_transfer`, and the second `deliver_spi3_transfer` after the board's GPIO edges),
`periph_write_inner` the same pair; each returns on `dma_tx_pending.is_none()` and an empty
transfer queue. `cadence_active` gains two field tests, the mmio-s1 write shortcut one more
match arm. No new per-tick state besides the two `Option` fields. The CPU comparison below
measures it; goldens are unchanged.

## Public definitions

- ESP-IDF v5.5.5 `components/soc/esp32s3/register/soc/reg_base.h` line 39: DR_REG_SPI3_BASE
  0x60025000 (v4.4.8 `components/soc/esp32s3/include/soc/soc.h` line 74; line 122
  `REG_SPI_BASE(i)`: GPSPI2 and GPSPI3 are 0x1000 apart, the same register layout).
- v5.5.5 `components/soc/esp32s3/include/soc/interrupts.h` line 41: ETS_SPI3_INTR_SOURCE = 22
  (v4.4.8 `components/soc/esp32s3/include/soc/periph_defs.h` line 81).
- v5.5.5 `components/soc/esp32s3/include/soc/gdma_channel.h` lines 11-12:
  SOC_GDMA_TRIG_PERIPH_SPI2 0, SOC_GDMA_TRIG_PERIPH_SPI3 1 (v4.4.8 line 20).
- v5.5.5 `components/soc/esp32s3/include/soc/gpio_sig_map.h` lines 136-147 and 226:
  SPI3_CLK_OUT 66, SPI3_Q_IN/OUT 67, SPI3_D_IN/OUT 68, SPI3_CS0_OUT 71, SPI3_CS1_OUT 72,
  SPI3_CS2_OUT 127; line 149 LEDC_LS_SIG_OUT0 73 (v4.4.8 lines 144-157, 234 the same).
  `components/soc/esp32s3/include/soc/spi_pins.h` line 48 (v4.4.8 line 49): "SPI3 have no
  iomux pins".
- Arduino-ESP32 3.3.12 `cores/esp32/esp32-hal-spi.h` line 36 (`HSPI 1`, "SPI 3 bus" on the S3)
  and `esp32-hal-spi.c` line 593 (`FSPI → SPI2_HOST`, otherwise `SPI3_HOST`).

No silicon measurement: SPI3 is modelled as register-compatible with SPI2 (the TRM and the
shared `spi_ll` driver treat GPSPI2 and GPSPI3 as one block type); the SPI2 model's limits
apply (no RX DMA, CPU-driven phases, bounded TX descriptor completion).

## Firmware

[spi3-probe.ino](spi3-probe.ino), Arduino-ESP32 3.3.12 (ESP-IDF v5.5.5): a microSD module on
`SPIClass(HSPI)` (SCK 18, MISO 16, MOSI 17, CS 15) through the core's SD library and an
Adafruit ILI9341 on the default `SPI` (FSPI: SCK 12, MOSI 11, MISO 13, CS 10) at once. It
prints `tft ready` after the display init, then retries `SD.begin` every second (`mount failed`)
until a card answers.

Run from the mask ROM with board `none` (every MISO byte 0xff, so no card ever answers) for
6 emulated seconds with `--debug spi` (command in inputs.json):

| Controller | Transfers in 6 s | What they are |
| --- | ---: | --- |
| SPI3 | 366 | the SD library's bring-up: 339 single 0xff bytes (the 80 idle clocks before each attempt) and 27 × CMD0 `40 00 00 00 00 95` |
| SPI2 | 88 | the ILI9341 init sequence, starting `ef 03 80 02 …` (write-only) |

CMD0 never appears on SPI2. Console: `tft ready`, then `mount failed` once a second. On main
(`f5a038bf`, the same command for 3 emulated seconds) the same image prints `tft ready` and then
spins in `SD.begin`: the first SPI3 transfer never completes, core 1 runs flat out
(550 084 899 instructions in 3 s), no SPI3 log line, no `mount failed`.

## CPU comparison

Against main `f5a038bf` (the same tree, the same Rust 1.99.0 release build in a separate
worktree), the hello demos and Pocket Tank as CONTRIBUTING.md asks: S3 hello 3000 emulated
seconds (board none), C3 and C6 hello 30 seconds, Pocket Tank 30 seconds (board
waveshare-amoled18-v2, the model at 0x290000 from `tools/fetch-pocket-tank.sh`); child user CPU
seconds via `GetProcessTimes` (Windows, 15.625 ms timer resolution); one warmup pair B→M, then
seven measured pairs alternating M→B, B→M; change is the ratio of medians. Every run of a workload
retired the same instruction count and produced the same console SHA-256 on both builds
(`identical_work_and_console` in [cpu.json](cpu.json)). No load gate; the desktop was otherwise
idle for this run (an earlier attempt, disturbed by a stray emulator process, was discarded).

By the receipt rules' flags (slower in at least 6/7 pairs, or non-overlapping ranges) nothing
is flagged: S3 hello −0.23% (slower 4/7), C3 hello −8.94% (1/7), C6 hello −1.20% (4/7),
Pocket Tank −0.19% (3/7; one pair on each side reached ~103 s against an 89–92 s floor, both
builds alike). Parity within noise: the per-tick addition is the early-returning SPI3 calls.
The static hot-function comparison (`docs/evidence/classic-core/hot.py`) was not run on this
MSVC build (no symbols in the release executable for `llvm-objdump`); `tick_impl` and
`periph_write_inner` are not identical to main's by construction.

### run 1, started 2026-10-15T15:53:37

| Workload | Main median (range) | PR median (range) | Change | PR slower in N/7 | Instructions | Console SHA-256 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| s3-hello | 27.6094 (26.9219–28.4375) | 27.5469 (26.6406–29.1562) | -0.23% | 4/7 | 1134713226 + 655106431 | `98de90e19cbd…` |
| c3-hello | 1.9219 (1.6562–2.1406) | 1.7500 (1.5312–2.0000) | -8.94% | 1/7 | 4800000000 | `3b9d8d3eb053…` |
| c6-hello | 2.5938 (2.1719–2.7656) | 2.5625 (2.3594–3.2031) | -1.20% | 4/7 | 4800000000 | `bd69fbb9ac5c…` |
| pocket-tank | 90.3281 (89.1250–102.6250) | 90.1562 (88.9375–102.5938) | -0.19% | 3/7 | 4548165341 + 6107970320 | `880ce5621dde…` |

#### s3-hello

| Pair | Order | Main s | PR s |
| --- | --- | ---: | ---: |
| warmup | B→M | 27.203 | 28.578 |
| pair1 | M→B | 27.188 | 27.422 |
| pair2 | B→M | 27.828 | 28.969 |
| pair3 | M→B | 26.922 | 28.750 |
| pair4 | B→M | 27.828 | 27.547 |
| pair5 | M→B | 28.438 | 26.656 |
| pair6 | B→M | 27.516 | 26.641 |
| pair7 | M→B | 27.609 | 29.156 |

#### c3-hello

| Pair | Order | Main s | PR s |
| --- | --- | ---: | ---: |
| warmup | B→M | 2.031 | 1.953 |
| pair1 | M→B | 1.859 | 1.750 |
| pair2 | B→M | 1.656 | 2.000 |
| pair3 | M→B | 2.141 | 1.766 |
| pair4 | B→M | 1.922 | 1.734 |
| pair5 | M→B | 2.125 | 1.531 |
| pair6 | B→M | 1.922 | 1.672 |
| pair7 | M→B | 2.062 | 1.984 |

#### c6-hello

| Pair | Order | Main s | PR s |
| --- | --- | ---: | ---: |
| warmup | B→M | 2.172 | 2.312 |
| pair1 | M→B | 2.594 | 2.562 |
| pair2 | B→M | 2.641 | 2.750 |
| pair3 | M→B | 2.766 | 2.359 |
| pair4 | B→M | 2.438 | 2.594 |
| pair5 | M→B | 2.172 | 2.484 |
| pair6 | B→M | 2.578 | 3.203 |
| pair7 | M→B | 2.734 | 2.375 |

#### pocket-tank

| Pair | Order | Main s | PR s |
| --- | --- | ---: | ---: |
| warmup | B→M | 89.328 | 74.641 |
| pair1 | M→B | 102.625 | 90.156 |
| pair2 | B→M | 90.547 | 92.453 |
| pair3 | M→B | 90.328 | 89.344 |
| pair4 | B→M | 89.125 | 90.312 |
| pair5 | M→B | 90.000 | 88.938 |
| pair6 | B→M | 89.953 | 102.594 |
| pair7 | M→B | 90.812 | 89.188 |

## Limits

- No hardware comparison; the model is SPI2's, instantiated twice.
- SPI3's DMA data phase is committed at once; the experimental SPI2 wire timing
  (`--spi2-timing`) is not extended to it.
- Board models that answer by `host` already receive 3; the shipped boards ignore the
  controller number, so an SPI3 device needs a board that routes by pins or by host.
- The firmware images are not committed and no golden pins them.

## Checks on main `af9ebc79`

The same change on main `af9ebc79`. Rust 1.99.1 (58cf8f9c3
2026-10-13), `cargo +1.99.1`, Windows 11, ROMs from `tools/fetch-rom-elfs.sh roms`; entries
marked `on_main` in [checks.json](checks.json):

- Clippy native and wasm32 jit-tests, warnings denied: pass.
- `ESP32SIM_ROM_DIR=roms cargo test --release --workspace --no-fail-fast -- --include-ignored
  --skip external_`: 1018 passed, 2 failed, 0 ignored over 109 binaries; the failures are the two
  `decoder_matches_corpus` tests (Windows drive-letter path split), unrelated. The three SPI3
  tests and main's `spi3_transfers_through_the_bus_reach_the_glass` (classic T-Display) pass;
  no golden changed. Virtual-quantum scheduler (`ESP32SIM_VQ_NATIVE=1`, `machine`,
  `virtual_stops`): 43 passed.
- The wasm build and the eight standard demos, the evidence privacy check: pass.
- Firmware, the same spi3.bin (`568b56f9…`) and command: 366 SPI3 transfers (339 × `ff`,
  27 × CMD0 `40 00 00 00 00 95`), 88 on SPI2 starting `ef`, CMD0 never on SPI2; `tft ready`, then
  `mount failed`. On main `af9ebc79` (3 emulated seconds): `tft ready`, no SPI3 line, core 1 at
  550 084 899 instructions, as on `f5a038bf`.

## Review follow-up

`spi3_dma_fault` is cleared on every USR write, as `spi2_dma_fault` is; it was cleared only
while no SPI3 transfer was pending. The two rules agree in every state the model reaches today
(SPI3's DMA data phase is committed at once, so no transfer is pending at a USR write); the change
keeps a retry after a descriptor fault reading the same on both hosts. New test
`spi3_dma_fault_is_cleared_by_the_next_transaction` (`esp32s3/src/bus/tests.rs`): a descriptor
without its owner bit records `NotOwned` on SPI3 and reaches no board; the next CPU-FIFO
transaction clears it and reaches the board as host 3. With the clear removed it fails. Workspace
(`--include-ignored --skip external_`): 1019 passed, the same two `decoder_matches_corpus`
failures; Clippy native and wasm, the wasm build and demos, the privacy check: pass.
