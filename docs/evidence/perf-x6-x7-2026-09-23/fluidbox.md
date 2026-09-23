# Fluidbox workload (EX194)

Fluidbox is the third browser benchmark workload, added in x6. Both cores stay busy: core 1 runs a 900-particle SPH solver in scalar floating point and core 0 renders to the CO5300 panel over GP-SPI2. From x7 on, candidates were adopted by fluidbox first, then Pocket Tank; TinyDraw became a regression screen.

## Source and build

| Item | Value |
| --- | --- |
| Firmware source | [V4C38/esp32-fluidbox](https://github.com/V4C38/esp32-fluidbox) at `21d7516d143da25bab9176257131d01a66a2573d` (August 6, 2026), unmodified checkout |
| License | MIT, "Copyright (c) 2026 V4C38" (repository `LICENSE`) |
| Board | Waveshare ESP32-S3-Touch-AMOLED-1.8; simulator board `waveshare-amoled18-v2`, 16 MB flash, 8 MB PSRAM |
| Toolchain | ESP-IDF 6.1.0 (`idf (6.1.0)` in the build log), GNU 15.2.0 Xtensa compilers; target `esp32s3` taken from `fluidbox/sdkconfig.defaults` |
| Build | ESP-IDF project build in `fluidbox/`; the log ends in `Project build complete`. The built `build/fluidbox.bin`, `build/bootloader/bootloader.bin` and `build/partition_table/partition-table.bin` are byte-identical to the assets below |

Asset SHA-256 (identical in every fluidbox arm's provenance, for example `arms/x7.json` job `x7int-a-fl` and the per-arm captures hashed in [sources.json](sources.json)):

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| `fluidbox.bin` (app) | 300,176 | `9277d40a1eb1d0227178cdabad588a8473438d2731210ec0480b2d246f2fe61c` |
| `fluidbox-bootloader.bin` | 22,560 | `3b0d1ab7f9b8825629c4de1cb884b8787683fe2be1bcb66674605d7a25b18638` |
| `fluidbox-ptable.bin` | 3,072 | `5295e8c42d6fb5f95e4e8a185932c6191d27cfc5078322475347e40e71af815a` |
| `fluidbox.elf` (symbols only, not loaded) | 5,181,772 | `d86cd62b4141ddce5e0a9c2e8961d26f8589465db5345ea6c18a713dce180fda` |
| `esp32s3_rev0_rom.elf` | | `c0ce0f338d1de1bdc6efbef1591779a2a42c1ab7d759d3c6ae8ae63a7dd34cfd` |

The firmware is not committed; the harness reads it from the ignored `web/wasm/fw/local/fluidbox/`. Rebuild it from the commit above and compare these hashes.

## Harness and scripted IMU

- `949e28fe` adds the `fluidbox` entry to `tools/browser-benchmark/workloads.json`: 30 guest seconds, serial source UART0, check `sim_reports` (pattern `fluidbox: [0-9.]+ fps \| [0-9.]+ steps/s`, at least 10). Timed arms reported 14.
- `1d16f5a2` adds `esp32sim_set_imu_motion(mode)`. Mode 1 plays a fixed 30 s tilt, flip and three-shake script as a function of guest time through the QMI8658 model, so the fluid moves while the run stays exact. Mode 0 keeps the register stub that every earlier workload was pinned against. The setter rejects modes other than 0/1 and rejects once any instruction has executed.
- `067d0e94` makes the manifest apply `["esp32sim_set_imu_motion", 1]` before boot.
- `4d7b5eb5` (review fix W1/W2, see [reviews.md](reviews.md)): samples on a 4 ms cadence, so one timestamp names one sample; STATUS0 reports new data until the outputs are read; `esp32sim_set_measured_te` carries the selected motion over to the replacement board.
- `1f4c247d` re-pins the manifest to the fixed cadence.

## Pinned contract

All three are Node 30-guest-second references with zero panics and zero JIT failures ([exact harness](../perf-x5-2026-09-22/exact-harness.json) method). Node and browser console hashes differ because they capture different output representations.

| Contract | Source | Instructions | Frames | Frame-content SHA-256 | Node console SHA-256 | Reference artifact |
| --- | --- | ---: | ---: | --- | --- | --- |
| `fluidbox-still` (no motion) | main `aa353cf3` + `949e28fe` | 13,247,635,412 | 744 | `d5baec5b9690fd29338e89e16917fda191c6d1ab7bffbd89ad701dc65c7462fe` | `f60924092fbcbfca92e81c88c7eb6563e5c7d2ae77eca386c89f2cd3e4fdf509` | base `fbb189ac` |
| `fluidbox-motion-v1` (before the 4 ms fix) | `067d0e94` | 13,329,744,922 | 744 | `21792dd5b42f4c3441557e12f17ec5c596494da77b1cfa5f2aeae7ae17d6cf32` | `5227c09dc553fab2a8e0bf1bb475080f9c0b40b206ff8652609c7f6a303b6b72` | base-motion `f774ef8b` |
| `fluidbox-motion-v2` (after) | `1f4c247d` | 13,335,545,195 | 744 | `9b196782663ce72e04d3c51a59c6ee055619f002770884d0582cd353e43ce7a6` | `247ec1b7c72fb981c938b420ab074e8ccd09e9576516da4a65bf2d7150a31eec` | base-motion2 `4d9f8fdb` |

**Every fluidbox stage screen in this directory used `fluidbox-motion-v1`, except `control-aa-fluid` (still).** The review fix changes guest work by 5,800,273 instructions (0.04%) and the sample timing the solver sees, so the stage screens do not measure the v2 contract. The final confirmation does: the final stack `2f3ae625` against base-motion2 took 39.783→35.764 s (**10.10%** less wall time, four pairs, 0.75×→0.84× realtime), and the base-motion2 A/A `control-aa-fl2` measured +0.39% (two pairs). The superseded first final `ce4587f2` measured 9.40%. [Final samples](arms/final.json)

M3 Chrome base time on v2: base-motion2 39.887 / 39.733 s (`control-aa-fl2`), 0.75×. On v1: base-motion 39.956 / 39.903 s in the A/A control (`control-aa-fluid-motion`), 0.75× realtime. Base7 38.364 / 38.521 s (`control-aa-fl7`), 0.78×. [Samples](arms/x6.json), [x7 samples](arms/x7.json).

## Where the time goes (base-motion, before x6)

Coordinator profile and census; counters are Node jit-profile runs of the pinned 30 guest seconds, the profile is an M1 Chrome 153 cpu-profile build under load (shares only, not timing):

- Generated code 49.6% of samples, the Rust dispatch shell about 40% (`run_inner` 13.0 s, `run_block_inner` 5.0 s, `jit::run` 4.7 s, `find_block` 2.1 s).
- Hot guest code: core 1 `compute_densities_and_viscosity`, `relax_pair`, `relax_positions`, libgcc `__ieee754_sqrtf` (about 10% of core 1 generated samples) and ROM `__divsf3` (about 9%); core 0 `render_frame` (a three-instruction pixel loop at `0x4200c55c` with 33.9M retained backedges), `project_all`, `draw_disc`, ROM `memset`.
- Census: about 750M generated-module calls for 13.3G instructions (17.7 per call; Pocket 27), 498M EX153 chain hops, 139M region budget exits (Pocket 71M), 28M core-0 peripheral accesses through `h_exec` (GP-SPI2/GDMA transaction setup), interpreted blockers led by S32C1I 11.4M, S32E 10.5M, L32E 8.7M and core-1 RSR CCOUNT 4.1M.
- x7 float census (base7): about 23M `sqrtf` and 26M `__divsf3` calls per 30 s, about 144M module entries, half of core 1's. The divide/square-root assist opcodes are no-ops in the reference interpreter and emit no WASM; the leaves cost call and return handling.

These counts set the round-2 levers: tails/resume (EX182), MMIO helpers (EX190), interpreter blockers (EX159, EX191, EX192), call edges (EX164) and FP locals (EX193). Lab receipts: `notes/fluidbox-profile.md`, `results/census-fluidbox-motion-30s.txt`, `notes/r2-float.md` (hashes in [sources.json](sources.json)).

## Limits

The firmware driver's exact QMI8658 transaction sequence was not traced, so the reviews could not show whether the pre-fix W2 behavior affected the pinned v1 runs; it changed the v2 pin. The workload checks serial reports and instruction totals in the browser; frame content is checked only by the Node gate. Fluidbox was not run on hardware for comparison.
