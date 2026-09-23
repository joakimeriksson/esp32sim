# x6/x7 browser performance closeout — September 23, 2026

**Final stack confirmation (M3 Pro, Chrome 153).** The final code stack, tip `2f3ae625` (artifact `c2144b73362f2aa43ea45bdc7f57efddc36a7d5b99e50397a83a8ca3ead672df`), reduced median wall time on all three workloads, every pair faster:

| Workload | Baseline | Pairs | Baseline s | Final s | Reduction | Pair reductions | Realtime |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| Fluidbox (`fluidbox-motion-v2`) | base-motion2 `4d9f8fdb` | 4 | 39.783 | 35.764 | **10.10%** | 9.93, 9.81, 10.12, 10.53 | 0.75× → 0.84× |
| Pocket Tank | main `aa353cf3` (`fbb189ac`) | 4 | 23.955 | 22.662 | **5.40%** | 4.97, 5.48, 5.84, 5.02 | 1.25× → 1.32× |
| TinyDraw battery | main `aa353cf3` (`fbb189ac`) | 2 | 25.024 | 24.492 | **2.13%** | 2.16, 2.10 | |

The fluidbox A/A on base-motion2 (`control-aa-fl2`, two pairs) measured +0.39% (0.68, 0.09). All 24 arms of the final and its control pass pinned work, console and JIT checks. Base-motion2 is main plus the fluidbox workload; its Pocket and TinyDraw code is main's. These are whole-stack results against one baseline each, not sums of the stage screens below. Fluidbox is still below real time on the M3. [Final samples](arms/final.json), [index](results.json)

The stack is slices A–G: A fluidbox workload (`949e28fe`, `1d16f5a2`, `067d0e94`, `4d7b5eb5`, `1f4c247d`; branch `codex/x7-fluidbox`), B shell-s1 `0e363d53`, shell-s2 `9e24182f` and the B1 fix `a3de7247` (`codex/x7-dispatch-record`), C coverage-s1 `bbfe8364` and coverage-s2 `33bb9c1a` (`codex/x7-region-coverage`), D tails-s1 `3edf6a99`, tails-s2 `c7ee226e` and the T1/T2 fixes `d62b364e`/`ae81cb73` (`codex/x7-tail-copies`), E coverage-s3 `d7ba5aa9` (`codex/x7-mask-branches`), F edge-s1 `93d1b041` and edge-s1r `16c39902` (`codex/x7-copy-policy`), G mmio-s1 `3e02c50c` and the timed-SPI2 fix `2f3ae625` (`codex/x7-device-irq`). This evidence is on `codex/x7-closeout`. Gates on `2f3ae625`: 94,063 WASM differential cases; `cargo test --workspace --release` 457 passed, 0 failed, 22 ignored; native all-targets and wasm32 Clippy clean with warnings denied; 30 s Pocket exact (10,073,833,775 instructions, 737 frames, `71a5ccdb…`) and 30 s fluidbox exact against base-motion2 (13,335,545,195, 744 frames, `9b196782…`).

**Superseded first final.** The first final stack (`ce4587f2`, artifact `9ab33729`) also carried mmio-s2/s2b (the continuing device helper) and its null-version-table fix. It measured fluidbox **9.40%** (39.819→36.076 s), Pocket **3.12%** (23.921→23.176 s) and TinyDraw **0.00%** (25.029→25.030 s; pairs −0.40, +0.39), same baselines and pair counts. The helper's own Pocket screen then came in at −1.40% against mmio-s1's +1.65%, so it was dropped and the stack re-measured as above. The two finals ran at different times and are not a controlled isolation of the helper.

**Measured stages.** The x6 integrated base (`x7/base` `ff384f5e`, artifact `3f5f3870`: shell-s1/s2, coverage-s1/s2/s3, tails-s1/s2) against base-motion reduced median wall time **3.81% on fluidbox** (39.910→38.389 s) and **4.53% on Pocket Tank** (23.871→22.790 s), four pairs each, every pair faster; TinyDraw **1.30%** (2 pairs). These stage screens used the pre-fix `fluidbox-motion-v1` contract and are intermediate. The x7 integration `x7int-a` (`e7633eec`: base7 plus edge-s1/s1r and mmio-s1/s2/s2b, before review fixes) against base7 measured fluidbox **6.66%** but Pocket **−2.02%**, which is what led to re-examining the helper. [x6 samples](arms/x6.json), [x7 samples](arms/x7.json)

**Safari on the M3.** Safari 27.0 on the same M3 Pro, three alternating pairs each, runner paused: base-motion→base7 reduced fluidbox wall time **4.92%** (44.959→42.749 s; 0.70× realtime) and Pocket **3.27%** (29.077→28.126 s; 1.07×), every pair faster. The JSC OMG size cap (EX189) on base7 **added** 3.33% fluidbox and 1.97% Pocket wall time in Safari, every pair slower; it was rejected. The final stack was not run in Safari. [Safari samples](arms/safari-m3.json)

## Outcomes by candidate

Positive reduction means less wall time. Each job compares one frozen candidate with one frozen baseline; stacked candidates compare with that plain baseline, not with the stage below them. Single jobs of two to four pairs on one machine; no confidence intervals. Pair values and medians for every row are in the arm files.

**Adopted** (in the final code stack, with the review fixes in [reviews.md](reviews.md)). Stage screens; the final confirmation above is the result for the set:

| Candidate | ID | Fluidbox | Pocket | TinyDraw | Baseline |
| --- | --- | ---: | ---: | ---: | --- |
| shell-s1 flat dispatch record | EX186 | +0.37% (3) | +1.79% (4) | +0.30% (3) | base / base-motion |
| shell-s1+s2 flash-page epoch | EX168 | +1.19% (3) | +2.65% (4) | −0.06% (2) | base / base-motion |
| coverage-s1 adopted loops | EX187 | +1.15% (3) | −0.45% (4) | +0.82% (2) | base / base-motion |
| coverage-s1+s2 predicted JX edges | EX187 | +1.00% (3) | +1.87% (4) | pruned | base / base-motion |
| coverage-s1+s2+s3 MUL16, mask branches | EX187 | +2.31% (3) | +1.63% (4) | pruned | base / base-motion |
| tails-s1 selective guarded copies | EX182 | −0.71% (3) | −0.46% (4) | pruned | base / base-motion |
| tails-s1+s2 resume into copies | EX182 | +0.67% (3) | +0.82% (4) | pruned | base / base-motion |
| int-a (shell-s1+s2) | | +1.38% (4) | +3.29% (4) | pruned | base-motion |
| int-b (+coverage-s1) | | +1.48% (4) | +3.26% (4) | pruned | base-motion |
| int-c (+coverage-s2) | | +2.42% (4) | +4.68% (4) | pruned | base-motion |
| int-d (+tails-s1+s2) | | +2.72% (4) | +4.41% (4) | pruned | base-motion |
| base7 (int-d + coverage-s3) | | +3.81% (4) | +4.53% (4) | +1.30% (2) | base-motion |
| edge-s1 resumes choose copies | EX182 | +2.28% (4) | −0.16% (4) | pruned | base7 |
| edge-s1+s1r three selection windows | EX182 | +5.59% (4) | −1.12% (4) | +0.28% (2) | base7 |
| mmio-s1 precise SPI2/GDMA interrupt refresh | EX190 | +2.33% (4) | +1.65% (4) | pruned | base7 |

Round-1 fluidbox jobs (three pairs) merged each candidate onto the fluidbox branch and compared with base-motion; round-1 Pocket and TinyDraw jobs compared with main `aa353cf3` (base `fbb189ac`). Adoption followed fluidbox, then Pocket (no loss beyond about 0.5% unless the fluidbox gain was several times larger), then TinyDraw as a regression screen. tails-s1+s2 was adopted for the integrated result (int-c→int-d: fluidbox +2.42%→+2.72%, Pocket +4.68%→+4.41%, separate jobs) and because edge-s1/s1r build on its copies. edge-s1+s1r costs Pocket 1.12% for 5.59% on fluidbox, about five times larger, which the adoption rule allows; the Pocket cost is inside the final Pocket result.

**Not adopted** (negative, flat or superseded):

| Candidate | ID | Fluidbox | Pocket | Reason |
| --- | --- | ---: | ---: | --- |
| shell-s3r region formation outlined | EX186 | +0.52% (3) | +2.25% (4) | below shell-s2 (+1.19%, +2.65%), which it stacks on |
| codegen-s1 stack-window proof | EX188 | −0.04% (3) | +0.47% (4) | flat |
| codegen-s2 FR in i32 locals | EX105 | −1.60% (3) | −1.89% (4) | slower; exit spills, bytes +4.3% |
| codegen-s3 L32R literal folding | EX185 | −0.62% (3) | −0.22% (4) | flat to slower |
| host-s1 async module installation | EX119 | −1.67% (3) | −1.05% (4) | slower; TinyDraw −0.67% (2) |
| host-s1b async, dispatch paths unchanged | EX119 | −1.93% (3) | −1.54% (4) | slower under the stock harness |
| host-s3 JSC OMG size cap | EX189 | −1.83% (3) | −2.04% (4) | slower in Chrome; on base7 −0.97% (3); Safari −3.33% / −1.97% |
| tails-s1+s3 interior entries | EX182 | +0.44% (3) | −2.77% (4) | Pocket loss; int-d3 +1.86% / +0.84% versus int-d +2.72% / +4.41% |
| coverage-s2a standalone | EX187 | +0.38% (3) | +0.82% (4) | attribution only |
| call-s2 return edges | EX164 | −0.66% (4) | pruned | slower |
| call-s3 CALLX literal edges | EX164 | −2.79% (4) | pruned | slower; bytes +19.3% |
| call-s3t (s3 + wider copy choice) | EX164 | −0.72% (4) | pruned | slower |
| call-t wider tails copy choice alone | EX182 | +1.69% (4) | pruned | same policy as edge-s1/s1r, which measured more |
| edge-s1r64 copy room 64 | EX182 | +1.44% (4) | −2.08% (4) | below edge-s1r on fluidbox, worse on Pocket |
| mmio-s1+s2 direct MMIO helper | EX190 | +0.31% (4) | superseded | superseded by s2b |
| mmio-s1+s2+s2b one shared spill | EX190 | +1.42% (4) | −1.40% (4) | below mmio-s1 alone on both (+2.33%, +1.65%); TinyDraw −1.34% (2); first final with it: Pocket 3.12% vs 5.40% without |
| mmio-s3 lazy tick-budget refresh | EX190 | +0.94% (4) | −1.49% (4) | stacks on s2b; TinyDraw −0.75% (2) |
| x7int-a (base7 + edge-s1/s1r + mmio-s1/s2/s2b) | | +6.66% (4) | −2.02% (4) | integration stage; superseded by the final stack without s2/s2b |
| atom-s1 window handlers, S32C1I | EX159 | −0.04% (4) | failed/pruned | flat |
| atom-s1+s2 CCOUNT/INTERRUPT head reads | EX191 | +0.15% (4) | pruned | flat |
| atom-s1+s2+s3 interrupt-path blockers | EX192 | −0.11% (4) | pruned | flat; x7int-b (x7int-a + atomics) +6.53% versus x7int-a +6.66% |
| fp-s3 FR store-through locals | EX193 | +0.03% (4) | pruned | flat |
| fp-s3+s2 BR local | EX193 | +0.44% (4) | pruned | within control spread |
| fp-s3+s2+s1r first-read fill | EX193 | +0.44% (4) | pruned | within control spread |

Built but never timed: call-s1 (its fluidbox job was pruned after call-s3 lost), call-s3b (dropped on counters), coverage-s3b and coverage-s3a (TinyDraw jobs pruned). Not built, with reasons in the catalog: shell-s3 batch lane, host-s2 retention (census only, EX113), edge-s2 successor memo and edge-s3 entry epoch (bounded at about 0.5% and 0), fp-s1 assist-op inlining (the assist ops are no-ops in the reference). "Pruned" means the coordinator removed the queued job because it could not change a decision; those jobs have no timing.

## Controls

Two-pair A/A controls: Pocket −0.69%, +0.09% and −0.06% (the last through the candidate-tree path), TinyDraw −0.16%, fluidbox still −0.31%, fluidbox motion-v1 on base-motion +0.13% and on base7 −0.41%; the Safari warm-up (base7 against itself, one pair) +0.68%. Their pair values span −1.73% to +0.72%. Fluidbox motion-v2 on base-motion2: +0.39% (`control-aa-fl2`). Two pairs cannot define a noise floor; they justify treating effects under about 1% as unresolved.

## Method

**M3 Chrome.** A serial runner on the M3 Pro times one job at a time from a queue ([bench.sh freezes each artifact by SHA-256](sources.json) and pushes it). Before timing, each frozen artifact passes the runner's strict 30 s Pocket exactness gate once (four artifacts in parallel, `runner.sh` phase 1); a job whose candidate fails is skipped. Each arm starts a fresh headless Chrome 153.0.8010.53 (V8 15.3.76.13) process with `--disable-background-timer-throttling` and `--disable-renderer-backgrounding` through `tools/browser-benchmark/run-pairs.py`; pairs alternate baseline/candidate then candidate/baseline, and the harness refuses a campaign whose inputs change mid-run. Candidates that change `web/wasm` JavaScript (host-s1/s1b, `control-aa-tree`) ship their committed tree for the candidate arm. All artifacts use the production build policy (`tools/wasm-rustflags.sh`: LLVM inline threshold 2000, debug 0, stripped debug info, no wasm-opt). Pairs: Pocket 4, fluidbox 3 in round 1 and 4 from integration on, TinyDraw capped at 2 and run last. Timing uses no diagnostic features; counters come from separate jit-profile builds under Node. Load averages at each job's start are in the arm files.

**Safari on the M3.** Same M3 Pro, runner stopped for the session, `safari/server.py` from the lab (an ordinary-page fallback without WebDriver): Safari 27.0 loads a token URL, each arm runs in a fresh page and worker, the page must stay visible (a hidden page fails the arm) and the same browser process may keep caches. One warm-up pair, then fluidbox and Pocket, base-motion→base7 and base7→base7-jsc, three pairs each, in that order. The first session stopped at its first warm-up arm with an adapter error ("expected a headless Chrome timing capture") and was rerun ([exclusions](results.json)).

**No timing on the M1.** From September 22 the M1 Pro laptop ran no benchmark or timing screen of any kind: another agent ran heavy Rust builds there and its load fluctuated (the round-1 host worker recorded load averages 13–38). The laptop built artifacts, ran tests, Node exactness gates and census counters, which count work and do not depend on load. An earlier local Safari screen on the M1 had predicted about 10% less wall time for the JSC size cap; the M3 Safari measurement found the opposite sign. No M1 result in this directory is a timing result. M1 Chrome and Safari confirmations wait until the rule is lifted.

## Exactness contract

A candidate must reproduce the base's instruction total, serial-console bytes, frame count and frame-content hash. Two gates enforce it. (1) Node, per candidate, before queueing: 30 guest seconds of Pocket Tank (10,073,833,775 instructions, 737 frames, frames `71a5ccdb…`, console `b9d9966e…`) and, from round 2, fluidbox (see [fluidbox.md](fluidbox.md) for the three contracts), with zero panics and JIT failures. (2) Browser, every arm: completed status, the pinned instruction total, console hash equal within the job, zero JIT failures and the workload checks (Pocket `model_decisions`/`render_reports`, fluidbox `sim_reports`, TinyDraw firmware verdict `tinydraw-gate1-v1`); frames are recorded but browser captures carry no frame-content hash. All 644 Chrome arms and 26 Safari arms pass; within each job every arm has the same instructions, frames and console hash ([verifier](verify-results.mjs)). The WASM differential suite grew from 86,116 cases at main to 94,063 at the final tip `2f3ae625` (98,655 at the larger `x7/final` `091ce114`, which still carried mmio-s2/s2b and the atomics).

Every fluidbox stage screen used `fluidbox-motion-v1` (13,329,744,922 instructions), except `control-aa-fluid` (`fluidbox-still`). The review fix moved the pin to `fluidbox-motion-v2` (13,335,545,195); the two finals and `control-aa-fl2` measure v2. TinyDraw exactness was not rerun under Node.

## Workloads

Pocket Tank (both cores, PIE matmul kernels) and the TinyDraw battery (mostly one core, 9,819,885,134 instructions, 428 frames, 36 firmware checks) are unchanged from x5. Fluidbox (EX194) is new: V4C38/esp32-fluidbox (MIT) built with ESP-IDF 6.1.0, both cores busy, a scalar-FP particle solver on core 1 and the renderer on core 0, with a scripted 30 s IMU sequence. Source, build, asset hashes, contracts before and after the 4 ms IMU fix and the profile summary: [fluidbox.md](fluidbox.md).

## Lab names and catalog IDs

Lab notes proposed IDs that collided (three proposed EX186). Assigned IDs, in order after EX185:

| ID | Lab name(s) | Lab proposal |
| --- | --- | --- |
| EX186 | shell-s1, shell-s3r (shell-s3 batch lane unbuilt) | EXnnn |
| EX187 | coverage-s1, s2, s3, s3b, s2a, s3a | EXa |
| EX188 | codegen-s1 | EX186 |
| EX189 | host-s3, `x7/base-jsc` | EX186 (or next id) |
| EX190 | mmio-s1, s2, s2b, s3 | EX186 |
| EX191 | atom-s2 | coordinator assigns |
| EX192 | atom-s3 | coordinator assigns |
| EX193 | fp-s3, fp-s2, fp-s1r (fp-s1 unbuilt) | EXnnn |
| EX194 | fluidbox workload and scripted IMU | none |

Retries appended to existing rows: shell-s2 and edge-s2/s3 → EX168; tails-s1/s2/s3, edge-s1/s1r/s1r64 and call-t → EX182; codegen-s2 and fp-s3 → EX105; codegen-s3 → EX185; host-s1/s1b → EX119; host-s2 → EX113; call-s1/s2/s3/s3t → EX164; atom-s1 → EX159; coverage failure census → EX161; aggregate and Safari numbers → EX094.

## Files and curation

- [results.json](results.json): per job name, workload, work contract, source revision, baseline label, candidate SHA-256, pairs, medians, reduction, pair reductions, instructions, frames and pass state; baseline artifact hashes and exclusions.
- [arms/x6.json](arms/x6.json), [arms/x7.json](arms/x7.json), [arms/final.json](arms/final.json): every arm in execution order (pair, arm, wall seconds, frames, JIT bytes, JIT compile ms), console hash, queuing agent, start time and load at start. [arms/safari-m3.json](arms/safari-m3.json): Safari arms.
- [verify-results.mjs](verify-results.mjs): recomputes every median, reduction, pair order and work check (`node docs/evidence/perf-x6-x7-2026-09-23/verify-results.mjs`).
- [curate.py](curate.py): builds the five JSON files from the lab queue, the M3 summaries, per-arm `result.json` captures and the runner log.
- [sources.json](sources.json): SHA-256 and size of the 122 lab files (briefs, task files, worker notes, reviews, census receipts, the job queue) and 95 M3 files this was curated from, plus a digest over the 652 per-arm captures.
- [reviews.md](reviews.md), [fluidbox.md](fluidbox.md).

Omitted: raw event logs, Chrome logs, per-arm captures, CPU profiles, census text and worker notes. They hold host paths, scratch locations and debugger URLs; the committed files keep the numeric samples, hashes and checks needed to challenge each comparison. Host names, login names and home paths were removed; lab-relative paths remain. Percentages in `results.json` are rounded to four decimals; wall seconds are not rounded.

## Reproduction

Build a candidate with the production policy and time one job as in x5: create a virtual environment with `uv venv .venv` if absent, supply asset manifests matching the recorded input hashes (fluidbox: [fluidbox.md](fluidbox.md)) and run

```sh
uv run --offline --no-project --python .venv/bin/python python tools/browser-benchmark/run-pairs.py "$OUTPUT" \
  --baseline-tree "$BASE_TREE" --candidate-tree "$CANDIDATE_TREE" \
  --baseline-wasm "$BASE_WASM" --candidate-wasm "$CANDIDATE_WASM" \
  --assets "$ASSETS_JSON" --pairs 4
```

Source revisions in `results.json` are the lab commits that produced each artifact; round-1 fluidbox revisions are private merges of the candidate onto the fluidbox branch and are not published. The frozen artifact hash identifies what was timed. Published code: slices A–G on the branches named above.

## Limits

One M3 Pro, one Chrome and one Safari version, small nonrandomized samples, no confidence intervals. Jobs ran over about seven hours with other jobs in between; stages were never timed against each other directly except where a job says so. TinyDraw coverage is thin: 12 TinyDraw jobs finished (two pairs each apart from the three-pair `shell-s1-td`) and the rest were pruned. No M1, Firefox or iPhone timing, and no Safari run of the final stack. Counters (calls, hops, bytes) come from Node census runs and did not predict speed reliably (call-s3: 13.8% fewer fluidbox module calls, 2.79% more wall time). The fluidbox v1→v2 change means the stage screens and the final confirmation measure slightly different guest work.
