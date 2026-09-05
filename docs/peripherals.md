# Peripheral coverage

Legend: **full** = everything the IDF/Arduino drivers use; **partial** = the paths exercised
so far; **stub** = accepts writes, returns plausible reads; **—** = not modelled (unknown
registers are logged with `--log-periph`).

ESP32-S3 peripheral MMIO reads and writes must be aligned 32-bit accesses. Byte and halfword accesses raise a prohibited-access fault before any device side effect; unaligned word accesses raise an alignment fault. This is the emulator policy for unsupported accesses, not a claim about every silicon PMS configuration.

| Block | Base | Status | Modelled |
| --- | --- | --- | --- |
| Interrupt matrix (per core) | 0x600C2000 | full | source→line mapping for both cores, level lookup |
| System / sensitive / APB_CTRL | 0x600C0000… | partial | core-1 release/reset, cache enables, clock regs as stubs |
| Cache MMU | 0x600C5000 | full | 512 entries, flash and PSRAM pages, invalid entries fault |
| SPI0/SPI1 (flash controller) | 0x60002000/3000 | full | user commands, JEDEC (size follows `--flash-mb`), read/program/erase, status/QE |
| Octal PSRAM (on SPI1 CS1) | — | full | mode registers MR0–MR8, sync read/write, `--psram-mb` |
| efuse | 0x60007000 | partial | MAC, chip revision, defaults; `--efuse-regs` loads a dump |
| RTC_CNTL | 0x60008000 | partial | reset cause, slow-clock time, SW resets, RTC watchdog (stages, feed, wprotect) |
| systimer | 0x60023000 | full | 2 units, 3 targets, one-shot/periodic |
| Timer groups 0/1 | 0x6001F000/20000 | partial | timer 0 with alarm/auto-reload; WDT registers as stubs |
| GPIO / IO_MUX | 0x60004000/9000 | full | out/enable/input, pin matrix in/out selects, edge/level interrupts, strap |
| UART0/1/2 | 0x60000000… | partial | TX FIFO to console, RX from scripts, TX-done/empty interrupts |
| USB Serial/JTAG | 0x60038000 | full | TX/RX FIFOs, interrupts (IDF console and Arduino `Serial`) |
| I2C0/I2C1 | 0x60013000/27000 | full | IDF `i2c_master` command list, FIFOs, NACK/END/COMPLETE interrupts |
| GDMA | 0x6003F000 | partial | out-channels (I2S0/I2S1) and in-channels (CAM); descriptor walk, DONE/EOF/TOTAL_EOF |
| I2S0 / I2S1 | 0x6000F000/2D000 | partial | TX: frame rate derived from the clock tree (source, integer + fractional MCLK divider, BCK divider, slot width and count), 16-bit stereo capture to PCM; RX — |
| RMT | 0x60016000 | partial | TX channels: symbol RAM, clock divider, end marker, done interrupt; RX — |
| LCD_CAM | 0x60041000 | partial | camera engine (start/reset, VSYNC, frame pump from GDMA RX) and the LCD RGB/DPI engine (timing/clock registers, frame pump into GDMA TX, LCD_VSYNC); i8080 LCD mode — |
| SHA | 0x6003B000 | full | SHA-1/224/256/384/512, block and GDMA modes (bootloader image verification, TLS certificate digests) |
| AES | 0x6003A000 | partial | block and DMA modes, ECB/CBC/CTR/OFB, all key lengths (mbedTLS, WPA2 group-key unwrap); hardware GCM — |
| RSA/MPI | 0x6003C000 | full | large-number multiply, modular multiply and modular exponentiation up to 4096 bits, polled or interrupt-driven (every mbedTLS public-key operation) |
| WiFi MAC | 0x60033000 | partial | TX queues, RX descriptor ring, interrupt events, TSF, filters — enough for scan/auth/assoc/data with the unmodified blob (docs/wifi-plan.md) |
| RNG | 0x6003B000 | full | random words |
| regi2c / I2C_MST (PLL, RF analog) | 0x6000E000 | stub | reads back what was written; BBPLL and pkdet calibration-done bits set |
| GP-SPI2 master | 0x60024000 | partial | CPU-driven command/address/data phases; board MISO responses; bounded GDMA TX descriptor completion; RX DMA is not modeled |
| PCNT | 0x60017000 | full | 4 units × 2 channels, pos/neg/ctrl modes via the GPIO matrix, limits/thresholds/zero events, counter reset/pause |
| LEDC, ADC, SPI3, TWAI, SDMMC, USB-OTG | — | — | |
| WiFi baseband/PHY/RF, BT | — | — | radio registers are faked, not modelled; see wifi-plan.md |

CPU-side: full base ISA, FPU (single precision), MAC16, booleans, PIE (all esp-dl/esp-dsp
ops; FFT/GPIO/s32 corners decode but are not executed).
