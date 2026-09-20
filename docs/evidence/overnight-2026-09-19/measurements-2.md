# Measurement record: September 19, experiments 14–26

## Step 14: generality checks and two more negatives

- **Golden regression bar with virtual quanta, natively.** With `ESP32SIM_VQ_NATIVE=1 ESP32SIM_VQ=1000 --no-jit` (the interpreter path has the deferral guard; the AArch64 JIT does not, so the native default stays off), the three committed golden scenarios give the committed hashes: Atech synth WAV `c64c46c5…`, SID jukebox WAV `ca76a497…`, LCD-4B panel WAV `89880538…`, identical consoles and identical per-core instruction totals against `ESP32SIM_VQ=1` (319,618,108 / 338,474,266 / 396,469,561, equal to `tests/golden/*.insns`). These firmwares use I2S, RMT and LCD_CAM, i.e. the devices the EX134 cadence guard exists for.
- **EX144 any single busy core + backoff** (`night/fast-entry-0919` 1 commit): the Atech firmware runs on core 1 with core 0 idle, which EX133 as first written ignored; and it touches device registers every few instructions, so runs were cut short 3.17M times out of 3.59M (0.48 quanta per run, slower than not trying). Now whichever single core is busy qualifies, and a run cut short inside two quanta doubles a skip counter (max 255 rounds). Atech: 462K runs, 3.6 quanta each, no slowdown, still exact. TinyDraw wasm pair vs combined3: 43.20 → 43.29 s, exact. No gain on TinyDraw, removes a pathology elsewhere.
- **Cache parameter sensitivity (not adopted):** requested-data readiness 96 with 160-cycle service (EX054's split) fixes ring-scalar staging (1.217 → 1.036) but worsens present (1.022 → 0.952), HARD (0.985 → 0.942) and export (1.03 → 0.935). Left at 160/96.
- **EX145 instruction-fetch cache at dispatch granularity: no effect.** 64 sets × 8 ways × 32-byte lines for flash-mapped code, 140 cycles per missing line, touched for the entered block only: every ratio unchanged to three decimals, load_us stays 0.639. Removed again. Either the working set fits or the misses happen inside regions where this coarse model cannot see them.
- **Document load (peer's scoped native profile, `doc-load-native/`)**: the timed window is 57.5M instructions of eager rasterization (raster helpers 48%, MaterializedCanvas 22%, memmove/memcpy 6.5%, `__divsf3` 5.6%, `lroundf` 3.3%), 37% of it fetched from flash. The missing 200 ms is about 0.84 cycles per instruction, far more than the unpriced FP divide assist (100K divides) or LSI→use (3.4M) can supply, so the data side (cache geometry, PSRAM misses and dirty writebacks under scattered access) is the leading remaining hypothesis. Instruction counts alone do not rule out unknown per-operation latencies or path differences, and the coarse fetch-cache null result does not rule out fetch misses inside regions. Needs a hardware probe, not another parameter guess.

## Step 15: timed-model host cost, merge fix, Tier-B hardware session started

- Profile of the timed + priced model (`prof-timed`, 69 s sampled): generated 36.8%, `jit::run` 13.3%, `run_block_inner` 10.9%, and 1.4 s in `decode` — my alignment check decoded the target instruction on every redirected fetch through the helper (51M RETWs). Replaced by a length test on the first byte, skipped when `pc & 3 < 2`. Same ratios; TinyDraw timed + priced, both-busy quantum 512: **72.0 guest s in 60.6 s wall = 1.19× realtime** (`check-timed-priced-r4`, single untimed run).
- Merge fix: `pie::timing_tests::selective_pie_costs…` (from the timing branch) failed after the merge because main's packed PIE executor bypasses the table executor where the optional PIE cost hypotheses are charged. Packed execution is now skipped when such a hypothesis is selected (none is in our configuration). `cargo test --release --workspace`: no failures on `night/timed-0919` f16d4daf.
- **Tier-B hardware session** (tinydraw `calibration/esp32s3-tier-b` at 7a157d4, the probe whose README says its decomposition controls were never captured; `~/Archives/esp32s3/tier-b/` did not exist): normal image built and ELF-verified under IDF v6.1, then flash + `tools/tier-b-capture.py --cells all` for two boots, then the XIP-PSRAM image the same way. Runs in the background from `~/Archives/esp32s3/tier-b/logs/session.sh`; the tinydraw checkout was dirty (`.gitignore`, untracked site files) and the verifier records that. This is the data-side probe the document-load misfit needs; analysis is for the morning.

## Step 16: Tier-B cohort captured; measured cache prices replace the fitted ones; PIE Q readiness (EX146)

- **Tier-B hardware cohort** (`~/Archives/esp32s3/tier-b/`): normal image boots 1 and 2: 43/43 cells, 360 samples, 0 refusals each; XIP-PSRAM image boots 1 and 2: 44/44 cells, 373 samples, 0 refusals each; receipts and the archived ELFs written by `tools/tier-b-capture.py`. Both boots of the normal image agree exactly on these window totals: `first_line_d_psram` **96 cycles** and `first_line_d_flash` **128** (each times the probe's whole one-line read window, with no matched hit control subtracted, so they are not isolated miss penalties); `first_line_i_flash` 404 (one 724 outlier per boot); explicit dirty `msync` writeback 1016 / 1170 / 1506 / 2154 / 3442 cycles for 1 / 2 / 4 / 8 / 16 lines (**≈162 cycles per additional dirty 64-byte line** on top of 862 for the clean call; an explicit flush cost, not a proven price for automatic victim eviction); `store_hit_psram` 272 for its whole window of 256 stores (baseline 276). The firmware's data cache is 32 KB, 8-way, 64-byte lines (`sdkconfig`); the model has 4 ways (the inline wasm path is built for that geometry).
- **Measurement-informed cache prices** (fill 96, writeback 160, taken from the window totals above with the caveats stated there; one shared fill price also covers flash, whose window is 128) instead of EX054's fitted 160/96, on top of the instruction prices: ring-scalar staging 1.217 → **1.033**; every other timer moves down a little and the medians cluster near 0.96 (individual cases vary): compute 0.956 (0.883–0.969), present 0.981, wall 0.958, HARD 0.955, export 0.964, document load 0.630; whole battery 70.4 guest s (0.91). The fitted 160 had been absorbing CPU cost that was not priced yet. Ring-PIE staging drops to 0.71: it was only right before because the fill was inflated (EX059 already saw it 17% short).
- **EX146 static readiness of loaded PIE Q registers**: operand masks from the EX058 prototype (810f38c5), only the measured results delayed (VLD, LD.USAR, loaded Qu of SRC.Q.LD usable at issue + 2, EX080), folded into the same per-run table as load-use and FP readiness. Linear PIE staging 0.953 → **0.997**; ring PIE 0.704 → 0.714 (its shortfall is memory-side).
- Host cost unchanged: 70.4 guest s in 60.2 s wall = 1.17× realtime. `?timing=hw` on the page now uses the measured cache prices.

State of the timed model against the erased-start board: a measurement-informed model with remaining assumptions (CALLX = JX, SUB.S/MSUB.S = ADD.S, one-cycle latency for every unmeasured FP/PIE writer, round-robin replacement, explicit-`msync` cost applied to automatic eviction, one fill price for PSRAM and flash). Reported medians cluster near 0.96 on compute, wall, HARD and export, with individual cases from 0.88 to 0.98; document load 0.63 and ring-PIE staging 0.71 are the two open misfits, for which the data side is the leading hypothesis, not an established cause.

## Step 17: combined4, EX111, cache geometry

- **combined4** (1b28b34e on `night/combined-0919`: combined3 + EX144 + the 1024-quanta wasm default, built with no environment knobs; peer: 43,261 differential cases incl. new legacy-vs-virtual-quanta machine tests with only core 1 busy, native suites, CI clippy): TinyDraw 83.00 → 44.43 s in its own pair and 42.48 s as the control of the next pair (so equal to combined3 within noise), exact; pocket-tank 80.91 → **65.41 s (−19.2%)**, exact — EX144 helps there because one of its cores does idle at times.
- **EX111 tiny tails** (peer, 8850d6c6, 43,262 cases; only budget-1 continuations inside a decoded block stay interpreted): pocket-tank 65.54 → 65.41 s, TinyDraw 42.48 → 43.12 s against combined4. No gain. Not adopted.
- **Timed model cache geometry**: the model and the inline wasm probe now use the firmware's 8-way data cache (64 sets) instead of 4 ways. Ratios move by less than 0.015 (present 0.981 → 0.995, HARD 0.955 → 0.959); kept because it is the real geometry.
- **combined4 confirmed with three alternating pairs** (`runs/combined4-x3`, default build, no knobs): base 83.27 / 84.16 / 83.57 s, candidate 42.50 / 42.45 / 42.75 s → **−49.2%, 1.12× modeled realtime**, bit-exact in all six runs. `tools/wasm-test.mjs` manifests hello, c3-hello, c6-hello, c6-energy-scan, panel, atech and atech-sid pass on the same artifact.
- Peer's design note for calls inside regions: [EX164 constraints](../../experiments.md#ex164) (direct CALL8 → ENTRY-headed chunk as an internal edge, RETW and CALLX still leaving; optimistic ceiling 45.6M dispatches ≈ 1.7 s ≈ 4%).

## Step 18: one head with everything

`night/timed-0919` now contains combined4 as well (merge, 2 conflict files). On that head:
- wasm differential suite: 43,261 cases with default features, 43,279 with `cache-inline` (the inline-cache test now follows the 8-way geometry); `cargo test --release --workspace` has no failures.
- Exact mode (plain build, no timing exports): TinyDraw bit-exact, 43.70 / 43.67 s against combined4's 42.69 / 42.53 s in two pairs, i.e. **the timing hooks cost about 2.5% when unused** (1.09× instead of 1.12×). `night/combined-0919` stays the branch to take for the exact path alone.
- Timed mode (`cache-inline` build + exports): identical to the pre-merge run (10,101,385,928 instructions, same ratios).

## Step 19: one artifact for both modes; page checks on the unified head

- A `cache-inline` build used to emit the inline data-cache probe into every generated memory access, whether or not the timing model was on: exact mode 48.33 s against 43.52 s, 28% more generated code. The probe is now emitted only after `esp32sim_set_approximate_jit_cache(…, 2)` switches the model on, and `cache-inline` is a default feature of the wasm crate on `night/timed-0919`. Same artifact: exact mode 43.79 s (control 43.69 s, identical generated bytes and console hash), timed mode identical to before (10,101,385,928 instructions, 62.6 s wall for 70.6 guest s). 43,279 differential cases and the workspace tests pass.
- Production page with that default artifact (`resp-unified-*`): exact mode boots the battery firmware to READY in 56.5 s wall, `?timing=hw` in 78.9 s; both commit 3/3 strokes and answer 24/24 movement points (medians 36.2 and 38.6 ms). The hardware figure next to the 78.9 s is 77.4 s from the startup serial line to the verdict; the endpoints differ (the page time runs from worker start to the READY line), so read it as "same order", not as a matched comparison.
- Board state at the end of the night: last flashed with the Tier-B XIP-PSRAM calibration image (`flash-xip-psram-2.log`), not TinyDraw.

## Step 20: readiness vs service, from the same Tier-B cohort (peer's finding)

The peer noticed that the cohort separates first-word readiness from sustained line service: `first_line_d_psram` is a 96-cycle window, but the uncontended baseline of `arbitration_psram_victim_internal_aggressor` is 702,762 cycles for 4,096 cold 64-byte lines = **171.6 cycles per line** (loop overhead included), and `flash_bandwidth_cross_core` 1,946,208 / 4,096 = 475.1 per line. The model charged fill = service = 96. With EX054's split (requested word ready after 96, the line occupies the memory service for 160, writeback 160):

| Firmware timer ÷ hardware | fill = service = 96 | ready 96 / service 160 |
| --- | ---: | ---: |
| ring PIE staging | 0.714 | **0.933** |
| ring scalar staging | 1.032 | 1.039 |
| linear PIE / scalar staging | 0.997 / 0.998 | 0.998 / 0.998 |
| paced cold compute_us (15) | 0.957 | 0.962 |
| paced cold present_us / wall_us | 0.995 / 0.961 | 1.007 / 0.971 |
| HARD total_us (4) | 0.959 | 0.979 |
| export | 0.969 | 0.975 |
| document load | 0.627 | 0.628 |
| whole battery guest s (hardware 77.4) | 70.6 | 71.0 |

Single untimed run, 36/36 (`check-tp-r96s160w160`, 71.0 guest s in 62.4 s wall = 1.14× realtime). Adopted for `?timing=hw`. The 160 is the 171.6-cycle total less an unmeasured loop overhead, so it is measurement-informed, not measured; flash lines (475 per line sustained) still share the PSRAM price, which pocket-tank's flash-resident weights will expose. Document load does not move: whatever is missing there is not line fill cost.
- **Linux on the S3 in the wasm module** (`tools/wasm-test.mjs linux`, image fetched by `tools/fetch-demo-assets.sh`, pinned by SHA-256): reaches `buildroot login:` on both builds with the same 440.8 M instructions and 170 console lines; base 18.1 s wall, combined4 15.3 s. A second dual-core system (network adapter on core 0, Linux XIP on core 1, MMU remaps, a function stub) that virtual quanta leave intact.

## Step 21: the document-load misfit is the instruction-fetch cache (EX147), not the data side

EX145's null result came from its granularity: it touched only the entered block's lines, and with 512-instruction regions almost no fetch line was ever touched, so the working set looked tiny. EX147 touches, per region entry, the fetch lines of the region (64 sets × 8 ways × 32-byte lines as in the firmware's `sdkconfig`, flash-mapped code only), and charges the Tier-B `first_line_i_flash` window of **404 cycles** per missing line. Same configuration otherwise (ready 96 / service 160 / writeback 160), single untimed runs, 36/36:

| Firmware timer ÷ hardware | no fetch cache | whole region span touched | the region's chunk lines touched |
| --- | ---: | ---: | ---: |
| document load_us | 0.628 | **0.956** | 0.737 |
| paced cold compute_us (15) | 0.962 | 0.981 | 0.976 |
| HARD total_us (4) | 0.979 | 0.998 | 0.999 |
| paced cold wall_us | 0.971 | 1.007 | 1.003 |
| paced cold present_us | 1.007 | 1.076 | 1.063 |
| export | 0.975 | 0.985 | 0.978 |
| whole battery guest s (hardware 77.4) | 71.0 | 73.3 | 72.4 |

The load window fetches 37% of its instructions from flash (peer's scoped profile), and a fetch-line miss costs four times a data-line miss, so this is where its missing 200 ms are. Both variants are approximations in opposite directions (the span also fetches gaps and unexecuted chunks; the chunk list misses entries through the slow path and treats every chunk as executed), and both overshoot presentation by 6–8%, so EX147 stays a diagnostic switch (`esp32sim_set_icache_fill`, off in `?timing=hw`). A faithful version needs the touch where a chunk is actually entered. My earlier note that the data side was the leading hypothesis is withdrawn.

Refinement the same hour: generated regions now record the chunks they actually enter (a 64-entry ring in `Cpu`, one store and one increment per chunk head of flash-mapped regions, emitted only when the fetch cache is on), and `jit::run` replays them into the fetch cache after the call. With per-core caches that gave load_us 0.709. Making the cache **one for both cores, as on the chip**, gives:

| Firmware timer ÷ hardware | no fetch cache | EX147, shared cache, chunk replay, 404 per line |
| --- | ---: | ---: |
| document load_us | 0.628 | **0.933** |
| paced cold compute_us (15) | 0.962 | 0.983 |
| HARD total_us (4) | 0.979 | 1.004 |
| paced cold wall_us | 0.971 | 1.013 |
| export | 0.975 | 0.992 |
| paced cold present_us | 1.007 | 1.072 |
| whole battery guest s (hardware 77.4) | 71.0 | 73.8 (0.954) |

Ranges over the individual cases: compute 0.977–0.989 (all 15 within 1.2 points of each other), HARD 1.002–1.019, wall 0.986–1.042, present 1.025–1.106. 36/36, single untimed run (`check-tp-icache404-shared`), 73.8 guest s in 70.4 s wall = 1.05× realtime. Core 1's flash-resident code (the touch sampler and FreeRTOS paths) evicts core 0's lines: a cross-core effect through the shared fetch cache that no per-core model can show. Adopted for `?timing=hw`; presentation is now the one timer more than 2% off in the median (7% slow).

- **pocket-tank under the final `timing=hw` set** (`check-pt-hw`): 8.0–8.4 tok/s and 32–33 fps (board: 12 and 25–30), 30 guest s in 50.2 s wall. Inference got slower than with the fetch cache off (9.5 tok/s), i.e. further from the board. Two known reasons not to read much into it yet: the cache geometry in the model is TinyDraw's `sdkconfig` (64-byte data lines, 16 KB fetch cache), while pocket-tank is a different build whose bootloader programs its own geometry, which the model should take from the cache configuration registers instead of constants; and the peer found that packed PIE loads go through `Bus::read_bulk`, which bypasses the data-cache pricing (fix in progress).
- Peer's fix f12d9837 (packed PIE loads no longer bypass data-cache pricing through `read_bulk`; native regression went from 0 fills / 0 hits to 1 / 3) and lint cleanup a2e8b5af are on `night/timed-0919`. TinyDraw ratios unchanged (`check-hw-final2`: compute 0.982, HARD 1.004, wall 1.013, load 0.933, present 1.074; 74.0 guest s in 71.6 s wall). pocket-tank now 7.4–8.0 tok/s, 32–33 fps (`check-pt-hw2`): its weight loads are priced at last, and at TinyDraw's cache geometry and the shared PSRAM/flash price they are priced too high; the board does 12 tok/s.
- Production page with the final `timing=hw` set (`resp-hw-final`): READY after 88.1 s wall, 3/3 strokes, 24/24 movement points, 42.8 ms median (max 59.9).
- The timed model is deterministic: two runs of the final head and export set give the same 10,152,693,296 instructions, 73.975 guest seconds and console hash c8ec9334… (`check-hw-final2`, `check-hw-final2-repeat`; wall 71.6 and 75.4 s, so about 1.0× realtime with the fetch cache on).
- Instruction-side numbers still unused from the cohort: XIP-PSRAM `instruction_psram_cold` 993 against `instruction_psram_hot` 133 cycles for 256 bytes, i.e. about 107 cycles per sequential fetch line from PSRAM; for flash only the isolated first-line window (404) exists. A sequential flash fetch-line probe would settle whether the 7% presentation overshoot is a too-high sequential fetch price.

## Step 22: pocket-tank's cache configuration, flash price knob (peer), what the board must be doing

- The firmware's own cache geometry is readable from the emulated EXTMEM registers after boot (`--peek 0x600c4000,4 --peek 0x600c4060,4`): TinyDraw programs `DCACHE_CTRL = 0x11` (64-byte lines, 32 KB) and `ICACHE_CTRL = 0x0b` (32-byte lines, 16 KB, 8 ways); **pocket-tank programs `DCACHE_CTRL = 0x15`: a 64 KB data cache**. The model's geometry is a constant; it should follow these registers. Added a 64 KB option for now (`esp32sim_set_approximate_jit_cache(…, 3)`, 128 sets in the inline probe).
- pocket-tank, final export set (single untimed runs): 32 KB cache 7.4–8.0 tok/s; 64 KB cache 7.9–8.3; 64 KB plus the peer's separate flash price (26e82aa1, `esp32sim_set_approximate_flash_timing(128, 160)`) 7.4–8.0; fps 32–33 throughout; the board does 12 tok/s and 25–30 fps. Neither knob closes the inference gap, and a bigger flash price moves away from the board. The weights stream sequentially from flash, so the likely missing piece is the S3's data-cache autoload (sequential preload), which ESP-IDF enables and the model does not have: a line-at-a-time miss price cannot be right for a sequential stream. Not attempted tonight.

## Step 23: EX147 review fixes and the price's uncertainty

Peer review of EX147 found: the ring was replayed only on the hot region-entry path (regions entered through the slow path, e.g. with an active in-region hardware loop, were never priced); the fetch cache was a module-global that a second emulator in the same instance would inherit; more than 64 chunk entries per call lose their order (left as a labeled approximation); and the 404-cycle window's raw records show two fetch misses, so 404 is not a proven per-line price. Fixed the first two (both call paths replay; the cache resets when the price is set for a new machine). Ratios with 404 are unchanged. Sensitivity to the price, same head (`check-hw-final5*`, single runs, 36/36):

| Firmware timer ÷ hardware | 404 per line | 200 per line |
| --- | ---: | ---: |
| document load_us | 0.934 | 0.757 |
| paced cold compute_us | 0.982 | 0.971 |
| HARD total_us | 1.007 | 0.995 |
| paced cold wall_us | 1.013 | 0.990 |
| paced cold present_us | 1.074 | 1.030 |
| export | 0.992 | 0.983 |

Document load wants at least 404, presentation at most 200, so presentation's overshoot is not only a fetch price. `?timing=hw` keeps 404 as a provisional value; a sequential flash fetch-line probe is the measurement that would replace it.

## Step 24: two more nulls

- **Data-cache autoload is not the pocket-tank explanation** (peer, `autoload/*-peek.log`): after boot both firmwares leave `DCACHE_AUTOLOAD_CTRL` (0x600c404c) at 0x8, enable and section bits clear, section registers zero; IDF 6.1's `cache_hal_init` preserves that state and no S3 call site enables autoload. The 8-versus-12 tok/s gap stays unexplained.
- **EX149 solo memory stalls spend budget instead of ending the batch** (timed model): 69.8 s against 70.6 s wall, same ratios. No gain; reverted.
- Exact path, two saturation checks against combined4 (one pair each, exact): regions up to 128 chunks / 1,024 instructions 42.53 → 42.78 s; compile threshold HOT 32 → 8: 43.12 → 44.01 s with 32% more generated code. With 16 code pages on top of 128 / 1,024: 42.89 → 44.56 s. None adopted; the 64 / 512 / 8 limits and HOT 32 stay.

## Step 25: pricing parity in the differential suite (peer), final head

The peer added a priced pass to the wasm differential suite (5a2476b9, cherry-picked onto `night/timed-0919`): with pricing on, the interpreter and the generated code must accrue the same `Cpu::timing_extra`; 35,545 priced cases on top of the existing ones (78,824 with default features, 78,806 without the inline cache), plus three expected-cost goldens for the FP readiness table, because a bug in the shared table would pass a parity check. It found two more real divergences, both fixed: a false conditional branch sitting at LEND was charged as taken by the interpreter (its pc changes through the loop backedge), as was a taken branch to the next instruction (now decided from the branch operands, only inside the pricing gates); and a static wait emitted before an instruction whose helper then faulted was not refunded. Known and documented outside the equality contract: alignment of inline JX/CALLX targets.

Final head `night/timed-0919`: TinyDraw under `timing=hw` 73.975 guest s, 36/36, compute 0.983, HARD 1.007, wall 1.009, export 0.992, document load 0.934, present 1.081 of the erased-start board, 71.7 s wall (`check-hw-final7`). The same artifact in exact mode: bit-exact, 44.92 s against combined4's 42.92 s in one pair, so the timing hooks cost 3–5% when unused.
- Peer's closing validation on 5a2476b9: the CI workspace command (`--include-ignored`, external tests filtered, local ROMs) 253 tests pass, including the golden firmware tests; worker pacing and integration tests, 16 Node and 12 Python harness tests pass; the default wasm build passes all 8 CI manifests. Sequential model reset (a7ba45ee): `esp32sim_new` resets the emit-time timing switches and the fetch cache, with an ABI regression; two simultaneously live emulators with different timing settings in one module instance remain unsupported (documented destroy-before-create).
- **Final head `night/timed-0919` = e79d5259**: `timing=hw` run identical to the previous one (10,153,176,125 instructions, console 1e0b2d02…), 73.975 guest s in 68.9 s wall (`check-hw-final8`).

## Step 26 (morning, with the user): the page

- **Setup for trying it**: `work/night-timed/web/wasm/fw/tinydraw.json` + `local/tinydraw/` (the September 17 product build), served by `bin/serve-web.py` (`python3 -m http.server` reset Chrome's parallel connections with its listen backlog of 5; `tools/fetch-web-vendor.sh` was needed for xterm).
- **Choppy drawing**: three settings each held the page near 25 Hz — touch moves every 40 ms (`web/touch-input.js`), the AMOLED board's quiet-interval frame deferral, and a 50 Hz push cap. Now 8 ms, no deferral on that board, and a per-board push rate (`BoardModel::display_push_hz`, AMOLED 120, default 50). Synthetic 3 s drag in headless Chrome (`bin/drawcheck.mjs`): 23.4 → 55.9 changed frames per second, median gap 40.5 → 14.1 ms. Battery totals unchanged.
- **Speed dropping while drawing**: the worker yielded with a nested `setTimeout(0)` (clamped to 4 ms) after every 8 ms interactive turn. It now yields through a `MessageChannel` when not ahead, with display-frame backpressure (two unpainted frames at most). A 10 s drag holds 56 fps at real time.
- **Author's firmware, 20 guest s under Node, boot included, equal instruction totals**: `atech-sid` 20.7 → 13.8 s (0.97× → 1.45×), `panel-sid` 24.8 → 21.6 s (0.81× → 0.93×).
- **pocket-tank on the page**: 36–46% of real time during boot, 23–29% once inference starts; `&timing=hw` 34–58%. An opt-in `?quantum=512` (`esp32sim_set_quantum`, EX047's knob, deterministic but not bit-identical) takes the harness run from 65.4 to 54.2 s while the instruction total rises to 10.83 G. The rest is raw throughput with both cores busy: about 336 M instructions per emulated second wanted, 150–200 M delivered.
- **Pull requests** (drafts, native GitHub stack #90 on joakimeriksson/esp32sim, base main): #85 dispatch diet, #86 virtual quanta, #87 special-register block boundaries, #88 region entry cache and larger regions, #89 smooth drawing on the page. The tip of #88 has the same tree as the validated `night/combined-0919` head 1b28b34e; the tip of #89 passes the workspace tests, clippy with `-D warnings`, 43,261 differential cases, and both batteries bit-exact (`runs/prstack-s1`, `runs/prstack-pt`).
- Two more draft pull requests at the user's request: the timing model as a sixth layer of stack #90 (`timing/approximate-model`: two commits on top of #89, the tree of `night/timed-0919` without its bulky evidence directories; exact mode bit-exact and the timed battery identical to `check-hw-final8` on that tree, `runs/prtiming-exact`, `check-pr-timing-hw`; workspace tests, clippy and 78,825 differential cases pass), and the FlatRam bounds fix on its own against main.
