# x8 browser performance round and the 256-instruction quantum — September 23, 2026

**Shipped x8 stack (M3 Pro, Chrome 153).** The linear branch `x8/ship`, tip `9e8d3378` (artifact `38e599d00e93`), is the round's result. Against base8 with both arms at the 256-instruction quantum (base8 through the workload's `esp32sim_set_quantum 256` export), it reduced median wall time on all three workloads with every pair faster:

| Workload | Pairs | base8 s (q256) | x8/ship s (q256) | Reduction | Pair reductions | Real time |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| Fluidbox (`fluidbox-motion-v2`) | 4 | 24.243 | 19.787 | **18.38%** | 18.30, 18.63, 18.03, 18.58 | 1.24× → 1.52× |
| Pocket Tank | 4 | 17.700 | 15.501 | **12.42%** | 12.04, 12.45, 12.10, 12.40 | 1.69× → 1.94× |
| TinyDraw battery | 4 | 21.302 | 20.144 | **5.43%** | 5.26, 5.73, 5.19, 5.76 | |

Jobs `ship-fl`, `ship` and `ship-td` ([arms/ship.json](arms/ship.json)); the q256 fluidbox A/A run (`control-aa-fl256`, base8 against itself, 2 pairs) measured −0.67% (−1.29, −0.07). These jobs compare equal work under the q256 contract. What a user of the page sees is the combined effect of the code and the contract. Three alternating single arms per workload, with base8 at its own q64 default and x8/ship at q256, measured medians of fluidbox 35.491 → 19.657 s (**−44.6%**, 1.81×; real time 0.85× → 1.53×), Pocket Tank 22.527 → 15.493 s (−31.2%, 1.45×) and TinyDraw 24.390 → 20.126 s (−17.5%, 1.21×). Those arms run different guest work and are not pairs ([q256.json](q256.json) `headline`). All arms pass their work, console, JIT and workload checks.

base8 is `x8/base` = `codex/x7-closeout` `85c639db` (artifact `c2144b73362f`, byte-identical to the x6/x7 final that measured 35.764 s for fluidbox). The round's goal was fluidbox in real time on the M3 in Chrome, which needed at least 16.1% less wall time at q64. `x8/ship` is ten linear commits on `85c639db`:
- seven code slices: 1 shell `2d5b906c` (resume memo lane-s1/s1l/s1c, batch-s1), 2 lookup `eba3f049` (alias-s1, inner-s1, event-s1), 3 loops `e9979e2b` (loop-s1, store-s1), 4 calls `db7a3e50` (leaf-s1, leaf-s2, rename-s1, with the calls reviewer's two directed tests), 5 memo `6b071959` (memo-s2, lane-s2b, with the shell reviewer's prepared-entry test route), 6 hop `bcf94d29` (hop-s2, hop-s2b) and 7 quads `9c1b84ef` (gen-s1, gen-s2);
- the TIMG fix `937abffc`;
- the per-target default quantum `a66e16bf` (256 on wasm32, 64 native);
- the browser benchmark pins `9e8d3378`.

Slices 6 and 7 and the three q256 commits are patch-identical to the reviewed `x8/final` and `x8/q256-fix` commits (`git patch-id`). Slices 4 and 5 add only the reviewers' tests. watch-s1 (`x8/final` slice 8) is not shipped. Gates on the tip (lab coordinator log): 111,125 WASM differential cases, `cargo test --workspace --release` 459 passed and 0 failed, 14 native goldens at their q64 pins, 30 s Node exactness at q64 on slice 7 and at q256 on the tip.

**The q64 measurement of the code.** Before the contract changed, `x8/final` `69858e1c` (artifact `771c6a83839f`) was measured against base8 at q64 with equal work. It holds slices 1–7 plus watch-s1, so it is not the shipped tree. The shipped code alone was not timed at q64.

| Workload | Pairs | base8 s | x8/final s | Reduction | Pair reductions | Real time |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| Fluidbox | 4 | 35.676 | 29.755 | **16.60%** | 16.54, 16.93, 16.29, 16.94 | 0.84× → 1.01× |
| Pocket Tank | 4 | 22.525 | 19.992 | **11.24%** | 11.35, 10.84, 11.62, 12.83 | 1.33× → 1.50× |
| TinyDraw battery | 2 | 24.451 | 23.216 | **5.05%** | 5.14, 4.96 | |

Jobs `final8-fl`, `final8` and `final8-td` ([arms/final.json](arms/final.json)); the q64 fluidbox A/A run just before (`control-aa-fl8b`, 2 pairs) measured 0.00% (+0.41, −0.41). Gates on `69858e1c`:
- 110,955 differential cases, also with Rust overflow checks enabled;
- 460 native tests;
- `ESP32SIM_VQ_NATIVE=1` virtual_stops 7 passed;
- native and wasm32 Clippy clean with warnings denied;
- 30 s Node exactness on Pocket Tank (10,073,833,775 instructions, 737 frames, frames `71a5ccdb…`) and fluidbox (13,335,545,195, 744 frames, frames `9b196782…`, console `247ec1b7…`).

Base: 94,063 cases, 457 native tests.

**watch-s1 was dropped.** The per-4 KiB code watch ([EX110](../../experiments.md#ex110)) failed the TinyDraw control under both contracts:
- at q64, its four-pair TinyDraw recheck against int-c measured −0.68% with every pair slower;
- at q256, removing it from w4base (`w4-nowatch`) measured fluidbox −0.33% (every pair slower) and Pocket +0.16% (mixed), but TinyDraw **+0.93%** with every pair faster (1.02, 0.79, 0.85, 1.01).

A same-direction TinyDraw loss blocks adoption, so watch-s1 and its W1 resize fix are out. The watch-s1b variant (skip the bit test for all-watched entries) cannot help: a probe found that all-watched entries never occur, and `w4-watch-s1b` measured fluidbox −0.61%, Pocket −0.55% and TinyDraw −0.81% (4 pairs each, against w4base).

**The contract moved to a 256-instruction quantum in the browser.** With the user's approval (conditional on no performance or correctness regressions), the browser's exact instruction-clock scheduler runs 256 instructions per busy core per round instead of 64. That is a different work contract. It has new pinned totals, and device events and cross-core hand-offs land up to 255 cycles late instead of 63. On the w3-all artifact (three alternating single arms per workload and quantum), q256 took:
- fluidbox from 29.80 s to 19.87 s (−33.3%);
- Pocket Tank from 19.85 s to 15.61 s (−21.4%, with 14.8% more guest instructions);
- TinyDraw from 23.17 s to 20.36 s (−12.1%, 430 frames instead of 428).

Natively, q256 made Pocket 5.38% slower, so the default is per target: 256 on wasm32 and 64 natively, where the native goldens keep their pins (`a66e16bf`). The q256 review's blocker and major were fixed. Its follow-up review of the per-target branch says ready to merge, with minor items listed in [reviews.md](reviews.md). Details: [The q256 contract](#the-q256-contract).

**Wave 4 was stopped.** A fourth wave retried exact mechanisms under q256 against w4base (`x8/final` plus the first q256 commits). The user then stopped the round after q256, and its measured screens are recorded as not adopted ([Wave 4](#wave-4-under-q256-stopped-not-adopted)).

## How the stack was built

Five lanes (shell, quantum, calls, lookup, loops) each built up to three candidates on base8. Each candidate was queued as three jobs against base8: fluidbox with 4 pairs, Pocket Tank with 4 and TinyDraw with 2. Winners were merged in three integration stages. A second wave (w2) and a third (w3, one candidate) ran against integration stage c. The coordinator then trial-merged the waves, and the result was rebuilt as linear slices with the review fixes. Positive numbers mean less wall time; every job compares one frozen candidate with one frozen baseline, so **percentages from jobs against different baselines must not be added**.

**Integration and trial stages against base8** (fluidbox, Pocket 4 pairs; TinyDraw 2):

| Stage | Commit, artifact | Contents | Fluidbox | Pocket | TinyDraw |
| --- | --- | --- | ---: | ---: | ---: |
| int-a | `f8599f5f`, `220927818008` | base8 + lane-s1/s1l/s1c + batch-s1 + alias-s1 + inner-s1 + event-s1 | +4.75% | +3.84% | |
| int-b | `a6deda55`, `7a11bcc4956b` | + loop-s1 + tailcut-s1 + store-s1 | +8.07% | +5.95% | |
| int-c (= w2base) | `ad7c9853`, `249330578f4a` | + leaf-s1/s2 + rename-s1 | +9.68% | +4.86% | −0.44% |
| w2-all (trial) | `c567017c`, `97c686eb0ede` | int-c + memo-s2/lane-s2b + hop-s2/s2b + gen-s1/gen-s2 | +16.26% | +12.73% | +5.98% |
| w3-all (trial3) | `3b696862`, `79e0b66a8e24` | w2-all + watch-s1 | +16.42% | +12.59% | |
| x8/final | `69858e1c`, `771c6a83839f` | w3-all − tailcut-s1 + review fixes | +16.60% | +11.24% | +5.05% |

The shipped stack (x8/final without watch-s1, plus the q256 commits) was confirmed at q256 only, against base8 at q256 (table at the top). Its percentages are a different contract from the rows above and must not be compared with them as if the work were equal.

Pair values: [results.json](results.json). Within later baselines, w2-all against int-c measured fluidbox **+7.07%** (`w2-all-fl`) and w3-all against w2-all **+0.88%** (`w3-all-flw2`). int-c's fluidbox median, 32.22 s (0.93× real time), set the second wave's target of about 7% more. The Pocket difference between w3-all (12.59%) and x8/final (11.24%) comes from separate jobs. Removing tailcut-s1 measured −0.16% on Pocket inside int-c (`int8-cnt`), and the review fixes are not timing changes, so these jobs do not isolate a cause. base8's own fluidbox median ranged from 34.645 s to 35.824 s across jobs, so compare within a job only.

## Outcomes by candidate

Single jobs of 2–4 pairs on one machine ("screens"); no confidence intervals. The adoption rule: fluidbox gain first; Pocket (4 pairs) and TinyDraw (2) are controls, where ±2% is noise unless every pair points the same way and a same-direction loss blocks adoption unless fixed. Stacked candidates compare with the plain baseline, so increments are differences between separate jobs. "Pruned" means the coordinator removed the queued job untimed because it could not change a decision.

**Adopted** (shipped in `x8/ship`, with the fixes in [reviews.md](reviews.md)):

| Candidate | ID | Baseline | Fluidbox | Pocket | TinyDraw |
| --- | --- | --- | ---: | ---: | ---: |
| lane-s1 resume memo | [EX195](../../experiments.md#ex195) | base8 | +0.93% | +0.77% | −0.94% |
| lane-s1l (+ own loops) | EX195 | base8 | +1.21% | pruned | pruned |
| lane-s1c (+ own-module cuts) | EX195 | base8 | **+3.85%** | +1.95% | −0.61% |
| lane-s1c + batch-s1 | EX195, [EX177](../../experiments.md#ex177) | base8 | +4.32% | +2.21% | +0.30% |
| batch-s1 batch across device deferrals | EX177 | base8 | +0.90% | −0.29% | +0.40% |
| alias-s1 exits name their decoded resume | [EX172](../../experiments.md#ex172) | base8 | +1.99% | +2.10% | +0.45% |
| alias-s1 + inner-s1 `run_inner` diet | [EX168](../../experiments.md#ex168) | base8 | +2.90% | +3.23% | +1.48% |
| + event-s1 cached next CCOMPARE | EX168 | base8 | +3.83% | +3.20% | +1.41% |
| loop-s1 enter/resume inside own hardware loop | [EX196](../../experiments.md#ex196) | base8 | +0.71% | +2.19% | +0.27% |
| store-s1 store-run loops in bulk | [EX197](../../experiments.md#ex197) | base8 | +1.43% | +0.53% | −0.16% |
| leaf-s1 closed leaves as shared functions | [EX164](../../experiments.md#ex164) | base8 | +1.66% | +1.39% | −0.01% |
| leaf-s2 (+ cut where credit ends) | EX164 | base8 | +3.37% | +1.63% | −0.02% |
| rename-s1 (inline, static window renaming) | EX164 | base8 | **+4.06%** | +1.66% | +0.11% |
| memo-s2 lean memo hit | EX195 | int-c | +1.79% | +1.80% | +0.11% |
| memo-s2 + lane-s2b memo hits from the round batch | EX195 | int-c | +2.77% | +2.36% | +1.28% |
| hop-s2 early no-region verdict | EX168 | int-c | +0.24% | −0.15% | +0.53% |
| hop-s2 + hop-s2b flash epoch covers mask ROM | EX168 | int-c | +0.68% | +0.77% | +0.90% |
| gen-s1 register-quad addressing | [EX198](../../experiments.md#ex198) | int-c | **+3.86%** | +4.88% | +4.85% |
| gen-s2 store runs finish their loop | EX197 | int-c | +0.58% | −0.34% | −1.13%, 4-pair recheck +0.07% |

Every pair of every fluidbox job in this table was faster except one pair each of loop-s1 (−2.08, +1.41, +0.22, +0.15) and hop-s2b (+1.20, +0.74, −0.03, +0.63). Each lane's pick followed fluidbox: calls took rename-s1 over leaf-s3 (+2.36%), shell took lane-s1c + batch-s1. From leaf-s1 and leaf-s2, the closed-leaf formation, page checks and cuts survive; leaf-s1's separate leaf functions were replaced by rename-s1's inline form and removed in the review-calls cleanup (slice 4). Two TinyDraw losses were rechecked with four pairs: gen-s2's −1.13% (two pairs) became +0.07% with mixed pairs, so noise; watch-s1's became −0.68% with every pair slower (−2.30, −0.72, −0.47, −0.52), a regression by the rule. Under q256, removing watch-s1 from w4base (`w4-nowatch`, 4 pairs each) measured fluidbox −0.33% with every pair slower (−0.36, −0.56, −0.26, −0.30), Pocket +0.16% (mixed) and TinyDraw **+0.93%** with every pair faster (1.02, 0.79, 0.85, 1.01). watch-s1 costs TinyDraw consistently under both contracts, so it was dropped (below).

**Not adopted:**

| Candidate | ID | Baseline | Fluidbox | Pocket | Reason |
| --- | --- | --- | ---: | ---: | --- |
| tailcut-s1 short-credit dispatch runs the copy | [EX182](../../experiments.md#ex182) | base8 | +0.77% | +1.47% | vs loop-s1 alone (+0.71%, +2.19%) no fluidbox increment; removing it from int-c (`int8-cnt`): +0.14% / −0.16%; dropped at slice 3 |
| exit-s1b dirty spills at budget exits | [EX163](../../experiments.md#ex163) | base8 | −0.70% | −0.80% | slower than loop-s1 alone; TinyDraw −1.92% (2 pairs, both slower); bytes +3.9% |
| nloop-s1 multi-chunk loops, merged ranges | [EX181](../../experiments.md#ex181) | base8 | −0.38% | +0.77% | fluidbox flat to slower |
| nloop-s2 small disjoint ranges | EX181 | base8 | +0.04% | pruned | flat |
| leaf-s3 resume inside the region's leaf | EX164 | base8 | +2.36% | pruned | below leaf-s2 (+3.37%) and rename-s1 (+4.06%) |
| lane-s1 + batch-s1 (first memo stack) | EX195 | base8 | +1.45% | pruned | superseded by lane-s1c + batch-s1 |
| watch-s1 code watch per 4 KiB | [EX110](../../experiments.md#ex110) | int-c | +0.41% | +0.03% | TinyDraw −0.86% (2), four-pair recheck −0.68% (all slower); at q256 its removal gained TinyDraw 0.93% (all faster); in `x8/final` slice 8, not shipped; its W1 resize fix goes with it |
| watch-s1b skip the bit test for all-watched entries | EX110 | w4base (q256) | −0.61% | −0.55% | TinyDraw −0.81% (4, all slower); a probe found all-watched entries never occur |

Built but not timed: alias-s2 (four-byte PIE alias arming; censuses identical to base, [EX172](../../experiments.md#ex172)), exit-s1 (every region spill; bytes +10%, EX163), the first two lane-s1 revisions (exact, but bytes +12% and +92%; EX195), the loops lane stack `c1b82968` (store-s1 + nloop-s1/s2) and the gen-s1+gen-s2 stack `ed842e68` (the same two changes are inside w2-all). Not built, with reasons in the catalog: lane-s2 (EX195: below noise after lane-s1), exit-s2 (EX163: the static form is EX163 m4), tramp-s1 (premise refuted, EX187), store-s2 (at most about 10M iterations, EX197), rename-s2 (subsumed by leaf-s2's cut form, EX164) and retire-s1 (nothing per module return, EX168).

## Wave 4 under q256 (stopped, not adopted)

w4base = `x8/q256-final` `693a76f5` (x8/final plus the first two q256 commits; artifact `648ae421fcd1`, 111,175 cases), q256 pins. The wave retried mechanisms whose per-exit cost a four-times-rarer region exit might now repay (lab task `w4.md`). Every job ran against w4base; each candidate's fluidbox job has 4 pairs.

| Candidate | ID | Commit, artifact | Fluidbox | Pocket | TinyDraw |
| --- | --- | --- | ---: | ---: | ---: |
| stack-s1 stack-window proof (x6 codegen-s1 port) | [EX188](../../experiments.md#ex188) | `ea2e3457`, `9a9dd71fb883` | **+2.24%** (2.21, 2.98, 2.26, 2.54) | +0.68% (all +) | +0.72% (2) |
| calls-s1 inline leaves may load and store | EX164 | `75a015f3`, `d8455a56bc8a` | +1.11% (1.47, 0.95, 0.37, 1.26) | pruned | pruned |
| tiny-s1 chain interprets tiny heads with no region | [EX171](../../experiments.md#ex171) | `9088c536`, `588daddc1e8e` | +0.39% (mixed) | pruned | pruned |
| atom-s1 L32E/S32E/S32C1I, RFWO/RFWU | [EX159](../../experiments.md#ex159) | `c4a77750`, `41504508861d` | −0.02% (mixed) | −0.29% (mixed) | pruned |
| atom-s1+s2 CCOUNT/INTERRUPT head reads | [EX191](../../experiments.md#ex191) | `af97cdbd`, `b20cf130668c` | −0.68% (3 of 4 slower) | pruned | pruned |
| atom-s1+s2+s3 SR/THREADPTR/ROTW/RFE | [EX192](../../experiments.md#ex192) | `9f36eeb4`, `9b24ee597966` | +0.35% (mixed) | pruned | pruned |
| nowatch (w4base minus watch-s1; ablation) | EX110 | `0825ae9a`, `d2f6f69a94f7` | −0.33% (all slower) | +0.16% (mixed) | **+0.93%** (4, all faster) |
| watch-s1b all-watched entries skip the bit test | EX110 | `a3e8c137`, `24ef47bc2e0e` | −0.61% (3 of 4 slower) | −0.55% (3 of 4 slower) | −0.81% (4, all slower) |

Built, not timed: fr-s1, FP registers in locals with write-back ([EX105](../../experiments.md#ex105) retry, `076d0bc7` alone and `35e15aaf` on stack-s1); calls-s2/s2b, inline leaves as traces through branches (`732eb54f`, `4c8a2d99`). The nowatch and watch-s1b rows decided watch-s1 (dropped). stack-s1's fluidbox result is the first positive EX188 screen: it measured −0.04% (3 pairs) at q64 in x6. It was not reviewed or adopted because the wave stopped. The calls, lookup and loops lanes did not write w4 notes before the stop, so mechanism text for their candidates comes from commit messages and job notes. The atom candidates' census and gates are in the quantum lane note: interpreted instructions 46.1M+11.3M → 6.7M+1.8M per 30 s, bytes +1.05%, 115,741 cases.

## Controls

Two-pair A/A runs of base8 against itself: fluidbox −0.56% (−1.56, +0.40; round start) and 0.00% (+0.41, −0.41; before the final), Pocket +0.21% (+0.64, −0.23); at q256 through the export, fluidbox −0.67% (−1.29, −0.07; before the ship confirmation). Two pairs cannot define a noise floor; they justify treating effects under about 1% as unresolved, and the adoption rule's ±2% band for controls. No A/A ran against w4base.

## Method

**M3 Chrome.** One M3 Pro, reserved for this round, runs a serial queue. `bench.sh` freezes each artifact by SHA-256 and pushes it. Before timing, the runner's strict 30 s Pocket exactness gate checks each frozen artifact once. Each arm starts a fresh headless Chrome 153.0.8010.53 (V8 15.3.76.13) with background throttling disabled through `tools/browser-benchmark/run-pairs.py`. Pairs alternate baseline/candidate then candidate/baseline, and the harness refuses a campaign whose inputs change mid-run. All artifacts use the production build policy (`tools/wasm-rustflags.sh`: inline threshold 2000, no debug info). Timing builds carry no diagnostic features; counters come from separate `jit-profile` builds under Node (the census, lab `bin/census.mjs`). Job start times and 1/5/15-minute load averages at each job's start are in the arm files. The runner switched to the q256 pins at 15:14 UTC. Four w4 candidates that arrived just before failed the old q64 gate at 15:10 UTC, then passed the q256 gate at 15:15 and were timed ([results.json](results.json) `excluded`); no timing was discarded. For the ship confirmation the M3 tree's workload manifests pass `esp32sim_set_quantum 256` explicitly, so the q64-default base8 runs at q256 in the same pairs. The headline single arms (`bin/m3-time.py`, runner paused) ran base8 from a tree without that export (q64) and x8/ship with it, three alternating arms per workload.

**Native on the M3.** A separate bench directory times the native CLI with the runner paused: `screen-native.mjs` (Pocket Tank 30 guest s), `screen-panel.mjs` (panel SID, about 7 s) and `screen-fluid.mjs` (fluidbox without motion, 30 s). It runs alternating pairs and fails if work or console differ between arms; the `-diff` variants, used for q256, allow it. Binaries: base8 from `x8/base` (`212d2de2`), final8 from `x8/final` (`4192e1a4`), w4base from `693a76f5` (`10f5398d`, quantum 256 on every target), q256fix from `x8/q256-fix` with the per-target default (`d849e768`, native 64) and ship from `x8/ship` (`4e3e81cf`, native 64).

**No timing on the M1.** The M1 Pro laptop ran no benchmark or timing of any kind this round: another agent's heavy compute held its load average near 200. It built artifacts and ran tests, Node exactness gates and census counters, which count work.

## Exactness contracts

**q64 (the x8 stack and every wave 1–3 job).** A candidate must reproduce base8's instruction total, serial-console bytes, frame count and frame-content hash. The Node gate checks each candidate before queueing: 30 s Pocket Tank 10,073,833,775 instructions, 737 frames, frames `71a5ccdb…`, console `b9d9966e…`; 30 s fluidbox (`fluidbox-motion-v2`, [EX194](../../experiments.md#ex194)) 13,335,545,195, 744 frames, frames `9b196782…`, console `247ec1b7…`. The browser checks every arm: completed status, the pinned instruction total, console hash equal within the job, zero JIT failures and the workload checks (Pocket `model_decisions`/`render_reports`, fluidbox `sim_reports`, TinyDraw verdict `tinydraw-gate1-v1`). TinyDraw is pinned at 9,819,885,134 instructions and 428 frames. All 780 Chrome arms pass ([verifier](verify-results.mjs)). The WASM differential suite grew from 94,063 cases at base8 to 110,955 at x8/final and 111,125 at x8/ship (without watch-s1's cases, with the q256 and reviewer tests).

**q256 (the new browser contract: wave 4, the ship confirmation and headline).** Node, 30 s: fluidbox 13,334,523,179 instructions, 744 frames, frames `01bdf42b…`, console `aac4b7cf…`; Pocket Tank 11,565,467,394 (+14.8%), 737 frames, frames `68caa66a…`, console `154258ef…`; TinyDraw 9,816,141,790, 430 frames, gate-1 verdict passes. The lab's q256 worker produced these pins from its build (`87068f60`). They are identical in every field to the coordinator's refs from w3-all with `esp32sim_set_quantum 256` and to the M3 validation arms ([q256.json](q256.json) `nodeRefs`). The TIMG review fix moves no pin at q64 or q256; `x8/ship` reproduces them (Node exactness at q256 on the tip, q64 on slice 7). With round batching and virtual quanta both off, the browser build still produces 11,565,467,394 for Pocket ([q256.json](q256.json) `nodeRefs`, `…-nobb-30s.json`), so batching and virtual quanta are exact at q256 too.

## Native results

| Screen | Work | Baseline → candidate | Pairs | Baseline s | Candidate s | Reduction | Pairs |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| Pocket Tank 30 s | equal | base8 → final8 | 4 | 46.995 | 45.815 | **+2.51%** | 2.19, 2.63, 2.54, 2.48 |
| Panel SID | equal | base8 → final8 | 4 | 2.670 | 2.554 | **+4.36%** | 4.49, 4.19, 4.52, 4.37 |
| Fluidbox still 30 s | equal | base8 → final8 | 2 | 63.435 | 61.216 | **+3.50%** | 3.41, 3.58 |
| Pocket Tank 30 s | q64 → q256 | final8 → w4base | 2 | 45.831 | 48.296 | **−5.38%** | −5.62, −5.14 |
| Panel SID | q64 → q256 | final8 → w4base | 2 | 2.557 | 2.351 | +8.08% | 7.98, 8.18 |
| Fluidbox still 30 s | q64 → q256 | final8 → w4base | 2 | 61.231 | 55.421 | +9.49% | 9.10, 9.88 |
| Pocket Tank 30 s | equal | final8 → q256fix | 4 | 45.762 | 45.873 | −0.24% | −0.67, 0.81, −0.25, −0.24 |
| Panel SID | equal | final8 → q256fix | 4 | 2.561 | 2.571 | −0.38% | −1.02, −0.50, 0.08, −0.49 |
| Fluidbox still 30 s | equal | final8 → q256fix | 2 | 61.214 | 61.154 | +0.10% | −0.10, 0.30 |
| Pocket Tank 30 s | equal | base8 → ship | 4 | 47.034 | 45.708 | **+2.82%** | 1.88, 3.05, 4.91, 2.79 |
| Panel SID | equal | base8 → ship | 4 | 2.676 | 2.552 | **+4.64%** | 4.51, 4.56, 4.66, 5.10 |
| Fluidbox still 30 s | equal | base8 → ship | 2 | 63.054 | 61.318 | **+2.75%** | 2.31, 3.19 |

The x8 stack does not regress native: all ten base8 → final8 pairs are faster, with equal instructions and console per screen ([native.json](native.json)). At q256 the CLI retires 11,564,578,121 Pocket instructions (4,664,831,600 + 6,899,746,521) instead of 10,073,833,775, and that is slower in both pairs. Native quantum switches are cheap, so the longer spin-waits dominate. This is why the default is per target: `a66e16bf` keeps 64 natively. With it, the native build is back at parity with final8 (q256fix rows: mixed pairs within ±1%, equal work), and the shipped stack is faster than base8 in all ten native pairs with equal work. The native q256 Pocket total is 889,273 instructions below the browser pin (11,565,467,394) because of where each harness stops, not because of emulation. The browser and Node loop checks the clock between 2M-step run chunks and ends at 7,200,444,855 cycles; the native CLI stops at 7,200,000,183 (30.000 s exactly). That is about 0.44M cycles on each of two cores. The browser build with round batching and virtual quanta off still gives the pin. At q64 the two totals happen to agree. The native fluidbox screen is the no-motion workload, whose CLI total is not a browser pin either.

## The q256 contract

**What changes.** `esp-soc` `QUANTUM` 64 → 256 on wasm32; native stays at 64 (`a66e16bf`, `esp-soc/src/machine.rs`). `esp32sim_set_quantum` still selects 64–4096 in multiples of 64, so 64 restores the old interleaving; the approximate-timing path keeps its own quantum. Device time advances once per round (`after_round` → `bus.tick`), so a device event due inside a busy round reaches the core up to 255 cycles late instead of 63, and a device-register read sees device time up to 255 cycles old. A cross-core interrupt or shared-memory hand-off reaches the peer up to 256 instructions later. Host script events and touch land at 256-instruction round boundaries. Core-local CCOMPARE timers still bound every dispatch, and a sleeping peer's timer still bounds the round. With a cadence-driven device active, the 256-cycle deferral cap now equals one round, so neither virtual quanta nor round batches span more than one round while it lasts. Round batching still covers 85% of fluidbox rounds (86% at q64); mean batch depth is 46 rounds (85 at q64). Receipts: the lab's q256 note, `esp-soc/src/machine.rs` and `esp32s3/src/bus.rs` line references there.

**Validation on the M3** (w3-all artifact `79e0b66a8e24`, `esp32sim_set_quantum 256` through the workload exports, three alternating single arms per workload and quantum in ABBAAB order; not a paired job):

| Workload | q64 median s | q256 median s | Wall | Instructions q64 → q256 | Frames | Guest output |
| --- | ---: | ---: | ---: | --- | --- | --- |
| Fluidbox | 29.803 | 19.873 | −33.3% | 13,335,545,195 → 13,334,523,179 | 744 → 744 | 14 reports each; mean 164.0 → 162.6 fps, 41.95 → 42.06 steps/s; values differ per report |
| Pocket Tank | 19.850 | 15.608 | −21.4% | 10,073,833,775 → 11,565,467,394 | 737 → 737 | at t = 20 s: decisions 10 → 10, 24.3 → 24.3 tok/s, asks 12 → 23; 13 → 15 advisor lines, different actions |
| TinyDraw | 23.169 | 20.361 | −12.1% | 9,819,885,134 → 9,816,141,790 | 428 → 430 | gate-1 verdict valid and passed, all 36 items |

Each quantum's three arms produce identical guest output; q64 and q256 differ, as a changed interleaving must. The Pocket fish reach the same decision count at the same token rate but choose different actions. A single-arm ceiling run on the same artifact measured fluidbox q64 29.91 s, q128 23.39 s (13,353,431,060 instructions, +0.13%) and q256 19.86 s ([q256.json](q256.json) `quantumCeiling`).

**Review.** The q256 correctness review held the change on one blocker and one major ([reviews.md](reviews.md)). The blocker: TIMG autoreload dropped the timer steps after an alarm crossed within one tick, reading 0 instead of 10 in the reviewer's scenario. The major: the WASM round-batch equivalence test compared batching with batching. The TIMG defect predates q256: on the unfixed model at q64, the fix's machine test reads 0 instead of 2 for a reload period shorter than a round, and 40 instead of 38 counting down. Fix `2af5043b` (shipped as `937abffc`) keeps the residual steps (modulo the reload period) as its own commit. The test now uses an unbatched reference (`bb_max = 1`, asserted). No pin moved. The per-target default `cf573dc2` and browser pins `25ce9ee1` (shipped as `a66e16bf`, `9e8d3378`) replaced the first shared-default commits. Gates on `25ce9ee1`: 111,175 cases, 462 native tests, 14 native goldens at their unchanged q64 pins, `ESP32SIM_VQ_NATIVE=1` virtual_stops 7 passed with its tests at 64 and 256. The follow-up review of `x8/q256-fix` found both findings resolved and the per-target default applied consistently: **ready to merge**. It lists pre-existing minor items: the TIMG model keeps `ALARM_EN` set after an autoreload alarm where hardware clears it, it does not detect an alarm crossing after a 54-bit wrap, docs described a single 64 default (updated in this commit) and `wasm/tests/abi.rs` runs natively at 64.

**Native.** At q256 the CLI Pocket screen is 5.38% slower (above), so native keeps 64 (precedent: EX177's `bb_max` is 128 on wasm32 and 1 natively), and the native goldens keep their q64 pins. `docs/architecture.md`, `docs/decisions.md` and `docs/speed-plan.md` now state the per-target default.

## Lab names and catalog IDs

| ID | Lab name(s) | Kind |
| --- | --- | --- |
| [EX195](../../experiments.md#ex195) | lane-s1 (`722f275f`, `c38c0bfa`, `4cc43c70`), lane-s1l, lane-s1c, memo-s2, lane-s2b; lane-s2 unbuilt | new |
| [EX196](../../experiments.md#ex196) | loop-s1 | new |
| [EX197](../../experiments.md#ex197) | store-s1, gen-s2; store-s2 unbuilt | new |
| [EX198](../../experiments.md#ex198) | gen-s1 register quads | new |
| EX047 (+ EX143, EX162) | q256, q256-fix, TIMG fix, per-target default | appended |
| EX177 (+ EX190) | batch-s1 | appended |
| EX172 | alias-s1, alias-s2 | appended |
| EX168 (+ EX165) | inner-s1, event-s1, hop-s2, hop-s2b, retire-s1 | appended |
| EX164 | leaf-s1, leaf-s2, leaf-s3, rename-s1, rename-s2, w4 calls-s1/s2/s2b | appended |
| EX182 | tailcut-s1 | appended |
| EX163 (+ EX104) | exit-s1, exit-s1b, exit-s2, dirty-spill probe | appended |
| EX181 | nloop-s1, nloop-s2 | appended |
| EX169 | hotwat-fl WAT reading | appended |
| EX187 | tramp-s1 | appended |
| EX110 (+ EX173, EX180) | watch-s1 (not shipped), w4 nowatch, watch-s1b | appended |
| EX159, EX191, EX192 | w4 atom-s1, s2, s3 | appended |
| EX188, EX105 | w4 stack-s1, fr-s1 | appended |
| EX171 | w4 tiny-s1 | appended |
| EX094 | x8 ship confirmation, headline and q64 final | appended |

## Files and curation

- [results.json](results.json): per job: name, round, workload, work contract, quantum, lane, lab note, source revision, baseline label and artifact, candidate artifact, pairs, medians, reduction, pair reductions, instructions, frames, console hash and pass state. It also lists baselines, contracts, exclusions, jobs pending at curation and pruned jobs.
- [arms/w1.json](arms/w1.json), [arms/integration.json](arms/integration.json), [arms/w2w3.json](arms/w2w3.json), [arms/final.json](arms/final.json), [arms/w4.json](arms/w4.json), [arms/ship.json](arms/ship.json): every arm in execution order (pair, arm, wall seconds, frames, instructions, JIT bytes, JIT compile ms, console hash), with the lane, start time and load at the job's start.
- [q256.json](q256.json): the 18 validation arms with parsed guest metrics, the quantum ceiling, the 18 headline arms (base8 at q64 against x8/ship at q256) and the Node exactness references.
- [native.json](native.json): the twelve native screens (rows with per-core instructions, JIT line, console hash, load before each arm; input hashes; binary hashes).
- [profiles.md](profiles.md) and [profiles.json](profiles.json): profile shares at step 0, int-c, w3-all and w3-all at q256.
- [reviews.md](reviews.md): the seven correctness reviews, findings, severities, fixes and receipts.
- [verify-results.mjs](verify-results.mjs): recomputes every median, reduction, pair order and work check (`node docs/evidence/perf-x8-2026-09-23/verify-results.mjs`).
- [curate.py](curate.py): builds the JSON files from the lab queue, M3 summaries, per-arm captures, validation arms, native screens and profiles.
- [sources.json](sources.json): SHA-256 and size of the lab files (brief, task files, lane and review notes, Node refs, queue, census receipts, profiles) and M3 files this was curated from, plus a digest over the 780 per-arm captures.

Omitted: raw event logs, serial logs, Chrome logs, per-arm captures, CPU profiles, census text and worker notes. They hold host paths, scratch locations and debugger URLs. The committed files keep the numeric samples, hashes and checks needed to challenge each comparison. Native screen arguments (asset paths) are reduced to input hashes; the Node refs drop their laptop wall times, which are not timing results. Percentages in `results.json` are rounded to four decimals; wall seconds are not rounded.

## Reproduction

Build a candidate with the production policy, supply asset manifests matching the input hashes in the arm provenance, and time one job with the stock harness as in x6/x7 (create a virtual environment with `uv venv .venv` if absent):

```sh
uv run --offline --no-project --python .venv/bin/python python tools/browser-benchmark/run-pairs.py "$OUTPUT" \
  --baseline-tree "$BASE_TREE" --candidate-tree "$CANDIDATE_TREE" \
  --baseline-wasm "$BASE_WASM" --candidate-wasm "$CANDIDATE_WASM" \
  --assets "$ASSETS_JSON" --pairs 4
```

q256 jobs need the q256 `workloads.json` pins (`9e8d3378`). Source revisions in `results.json` are the lab commits that produced each artifact; lane branches are local (`x8/<lane>`), and the frozen artifact hash identifies what was timed. Published code: the `x8/ship` commits, as the PR stack. `x8/final` (with watch-s1) stays a lab branch.

## Limits

One M3 Pro, one Chrome version, small nonrandomized samples, no confidence intervals. Jobs ran over about eight hours with other jobs between them; stages were never timed against each other except where a job says so. TinyDraw coverage is thin (mostly two pairs, many pruned; four pairs only for the rechecks, wave-4 watch jobs and the ship confirmation). There was no Safari, Firefox, M1 or iPhone timing this round; the round's next target, fluidbox in real time on the M1 Pro, is unmeasured. The q256 validation and the headline are three single arms per workload, not pairs, and q256 changes the work, so their wall-time changes are not equal-work reductions; the paired ship confirmation is. The shipped code was not timed at q64, and `x8/final` (timed at q64) also carries watch-s1. Counters come from Node census runs and again did not predict speed reliably: exit-s1b kept dispatch counters identical and lost 0.70%, nloop-s1 converted all 221.6M backward re-dispatches and lost 0.38%, and gen-s1 moved no dispatch and gained 3.86%. Review coverage is bounded (the reviews list what they did not check). Live resize after code has run remains unsafe because cached version indices are not reset; that is pre-existing and not shipped as fixed. Wave-4 candidates had no correctness review.
