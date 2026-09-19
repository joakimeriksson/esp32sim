# Overnight experiments, September 19, 2026

Two sessions worked through the night on the TinyDraw browser battery: a coordinator (Claude) that owned benchmarking, the board and this log, and a peer (GPT-6 Astra, launched separately by the user) that owned isolated patches, differential tests and code review. No subagents were spawned. In the morning, at the user's request, the exact-path work was opened as five draft pull requests (native stack #90, #85–#89 on joakimeriksson/esp32sim). The experiment branches are in this repository as `night/*-0919`; the adopted work is proposed as draft pull requests #85–#89 and #91 (one native stack) and #92.

## Results

The reference column gives the revision or configuration each figure comes from; rows marked historical are earlier receipts, not runs of main d5446b4a made tonight.

| | Reference | This night | Where |
| --- | --- | --- | --- |
| TinyDraw battery, exact instruction-count clock | main d5446b4a: 83.6 s wall, 0.57× | **42.5 s wall, 1.12×**; three alternating pairs on 1b28b34e and three on the previous head 75382bcc; identical 9,819,885,134 instructions and console hash | `night/combined-0919` at 1b28b34e, default build, pushed |
| pocket-tank, 30 guest s (both cores busy) | main d5446b4a: 80.9 s, 0.37× | 65.4 s, 0.46×, exact 10,073,833,775 (one pair) | same |
| Atech synth, SID jukebox, LCD-4B panel goldens | committed hashes | identical WAV hashes, consoles and per-core instruction totals with virtual quanta on (native, `--no-jit`, one run each) | `night/fast-entry-0919` |
| Production page, boot to READY | historical: 120.5 s (Sep 5 receipt, 8df2f0ad) | 54.7–56.5 s; strokes correct, movement→canvas median 36–38 ms (single runs) | `night/fast-entry-0919`, `night/timed-0919` |
| Approximate-timing model, EX066 configuration | historical: 0.478× (Sep 7, d13b7f93) | 0.98–1.05× with measurement-informed instruction, alignment, data-cache and fetch-cache prices; 1.14× without the fetch cache (single untimed runs) | `night/timed-0919` at e79d5259 |
| Timed model vs an erased-start board, 15 paced cold tests | EX066 configuration on tonight's head: compute 0.749, wall 0.787, document load 0.47 | medians compute 0.983 (cases 0.977–0.989 in the run that listed them), HARD 1.007, wall 1.009, export 0.992, document load 0.934, present 1.081; staging microbenchmarks 0.93–1.04; whole battery 74.0 guest s vs 77.4 s; deterministic (two runs, same instruction total and console hash) | same |
| pocket-tank under the timed model | instruction clock: 24.3 tok/s, 62.5 fps | 7.4–8.0 tok/s, 32–33 fps with the final export set (9.5 and 35 before the fetch cache and the packed-PIE pricing fix); `docs/speed-plan.md` gives the board as 12 and 25–30. The model uses TinyDraw's cache geometry and one price for PSRAM and flash lines, so inference is now priced too high | same |
| Production page under the timed model (`?timing=hw`) | strokes dead (board-swap bug, found tonight) | boots to READY in 79–88 s wall (88 s with the fetch cache on), strokes correct, 39–43 ms median. The board needs 77.4 s from its startup serial line to the verdict; the endpoints differ | same |

What made the difference, in order of size:

1. **EX133 virtual quanta** (new). While the other core idles, core 0 runs many 64-instruction quanta in one budget. The run is bounded so that nothing can fall due inside it, a device-register access stops in front of its instruction, and the spanned rounds are closed exactly as before, so results are bit-identical. EX133 alone (K≤4) took 85.3 s to 70.5 s; with **EX134** in its first, unsafe constant form 82.2 s to 53.9 s. The guarded form of EX134 (tick deferral raised only while no cadence-driven device is active) costs about 1.4 s of that and is the one that is kept.
2. **EX137 larger regions.** The widening part of EX043, isolated, is worth 15% under virtual quanta. EX043 itself was a flat multi-change bundle, so longer budgets are the plausible enabler, not a proved sole cause.
3. **Dispatch diet**: EX038 boxed cache, EX065 negative coverage cache, EX106 direct cut index (5–10%), plus EX135 and EX136 (2–3% each).
4. **EX139 + EX138/EX140/EX141/EX146** for the timed model: solo batches that yield in front of device registers, then measured prices folded into generated code as constants. More cycles per instruction means less guest work per simulated second, so the priced model still runs at realtime; that ratio is under a different clock and is not an equal-work speedup.

What did not work: EX109 guarded RETW (inside noise), EX121 wasm-opt (about 1%), EX043 last-mapping TLB reuse isolated (4.2% slower), EX111 tiny tails, the EX136 hot-loop extension, larger regions or an earlier compile threshold beyond EX137's limits, EX145 and EX149 (no effect), EX134 as a plain constant (breaks pocket-tank's pinned total), my first derived call/return prices (overcharged by 4.5 cycles per call; replaced by EX141's measured ones), my data-side and the autoload hypotheses (the document load was the shared instruction-fetch cache, EX147; pocket-tank's inference gap is unexplained).

## What to trust, and how far

- The exact-clock numbers rest on the harness's own contract: equal instruction totals, equal console SHA-256, 36/36 firmware checks, zero JIT failures, alternating pairs. combined2, combined3 and combined4 each have three pairs, the mapping-cache screen two; the other exact-clock rows are single screening pairs, and the timed-model rows are single unpaired diagnostic runs. Each entry in the log states its own count.
- The peer ran the wasm differential suite (43,261 cases on combined4; 78,824 on the timed head including 35,545 priced interpreter-versus-JIT parity cases), selected native suites (Xtensa semantics, S3, shared SoC) and the CI clippy command on the integrated commits. The coordinator ran `cargo test --release --workspace` on `night/fast-entry-0919` and `night/timed-0919` and both differential configurations on the merged timed head.
- The timed model is approximate by design: a measurement-informed model with remaining assumptions. Its instruction prices come from earlier hardware ladders (EX067, EX068, EX079, EX080) and from EX081's control cells, captured on September 7 and analysed only tonight; its cache prices from window totals of tonight's Tier-B cohort, which are not isolated miss penalties. Assumed: CALLX = JX, SUB.S/MSUB.S = ADD.S, one-cycle latency for unmeasured FP/PIE writers, round-robin replacement, explicit-`msync` cost for automatic eviction, one fill price for PSRAM and flash. The fit is to one firmware on one board revision.
- Hardware: the board was fully erased (authorized), ran the frozen gate-1 images for the two reference captures, then the Tier-B calibration images. It was not restored; the last image flashed is the Tier-B XIP-PSRAM one.

## Open items, most valuable first

1. Review and upstream `night/combined-0919` in pieces: EX133 + EX134 guard, the diet, EX135, EX136/137, EX144. EX133's exactness rests on several invariants together (the run-length bounds on device flush, script, page push, peer wake-up and limits; deferral of device registers; the cadence guard) and on the tested contract (pinned totals, console hashes, goldens). `vq_violations` is only a backstop for register accesses that escape deferral, not a correctness certificate. It deserves a reviewer who did not write it.
2. Timed model: presentation is 7% slow with the fetch cache on and the fetch-line price (404, a window with two misses in its raw records) is provisional; ring-PIE staging 0.93; pocket-tank's inference is 35% slow and neither its 64 KB data cache nor a separate flash price fixes that (sequential data-cache autoload was checked and is off in both firmwares, so that gap is unexplained); the cache geometry should come from the EXTMEM registers; CALLX assumed; 96/160 are window totals, not isolated penalties. The Tier-B cohort captured tonight (`~/Archives/esp32s3/tier-b/`) has the msync and SPI2 decomposition cells still to analyse.
3. Calls and returns inside regions: 82% of region exits, about 4 s of the remaining 42 s; the peer's design note (`design-calls-in-regions.md`) puts the first step (direct CALL8 → ENTRY as an internal edge) at no more than 4%.
4. pocket-tank stays at 0.44×: both cores busy, so it needs raw throughput or the timed clock, not scheduling.

## Files

- `summaries/*.json`: run-pairs summaries and single-run results named in the log.
- `hardware/`: gzipped serial logs of both erased-start captures, compact summaries, firmware-timer ratio tables.
- `tools/`: the small wrappers used (`pair.sh`, `battery.sh`, `prof.sh`, `resp.sh`, `cmp-hw.py`, `hash.py`).
- Raw captures, wasm artifacts, CPU profiles and module dumps stay local in `work/night-run/` (1.4 GB, ignored).

The step-by-step record, with every run and the file it came from, is in [LOG.md](LOG.md) (steps 1–13) and [LOG-2.md](LOG-2.md) (steps 14–26). Step numbers cited in pull requests and in the catalog refer to it.
