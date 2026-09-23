# x8 correctness reviews

Seven adversarial reviews ran on the x8 changes. Five covered the integrated trial branch `x8/trial3` (`3b696862`), sliced at its first-parent stages. Then one covered the rebuilt linear branch `x8/final` (`69858e1c`) and one the q256 contract change (`x8/final` → `x8/q256-final` `693a76f5`), with a follow-up on its fix branch `x8/q256-fix` (`25ce9ee1`). The shipped branch `x8/ship` (`9e8d3378`) is `x8/final` slices 1–7 with the reviewers' leftover tests folded into slices 4 and 5, without slice 8 (watch-s1), plus the three `x8/q256-fix` commits. `git patch-id` shows slices 6–7 and the q256 commits are identical to the reviewed ones, and slices 4–5 differ only by the added tests. Every review used the same brief. The contract: generated code must leave bit-identical architectural state, traps and instruction counts to the interpreter in every timing mode, cached facts must be invalidated by every event that can make them wrong, and native builds must not regress. Severities: **blocker** = wrong guest-visible state or a crash/panic; **major** = a missing invalidation or test for a plausible path, a native regression or an unsound `unsafe` block; **minor** = clarity, dead code or low-risk test gaps. Reviewers worked in their own scratch worktrees with `CARGO_BUILD_JOBS=3` and ran no benchmarks.

Receipts: the review notes are in the lab (`notes/review-{shell,lookup,loops,calls,watch,final,q256}.md`; hashes in [sources.json](sources.json)). The reviewers' scratch logs and patches stay local and are not committed; the notes cite them by file name. Wave-4 candidates were not reviewed: the round stopped first.

| Review | Range | Blocker | Major | Minor | Fix |
| --- | --- | ---: | ---: | ---: | --- |
| shell | `85c639db..b1ec6426`, `ad7c9853..1a5cf59a` | 0 | 0 | 0 | none needed; its prepared-entry test route shipped in slice 5 `6b071959` |
| lookup | `b1ec6426..f8599f5f`, `1a5cf59a..b4e438ac` | 0 | 0 | 1 | slice 2 `eba3f049` |
| loops | `f8599f5f..a6deda55`, `b4e438ac..c567017c` | 1 | 0 | 0 | slice 3 `e9979e2b` |
| calls | `a6deda55..ad7c9853` | 0 | 0 | 1 | slice 4 (`2478dff8` in x8/final; shipped as `db7a3e50` with the two directed tests) |
| watch | `c567017c..3b696862` | 0 | 1 | 0 | slice 8 `69858e1c` (conservative); not shipped: watch-s1 was dropped |
| final | `3b696862` → `69858e1c`, all eight slices | 0 | 0 | 0 | none; confirms the fixes above |
| q256 | `69858e1c..693a76f5` | 1 | 1 | 0 | `2af5043b` (TIMG; shipped as `937abffc`), test repair in `cf573dc2` (shipped as `a66e16bf`) |
| q256 follow-up | `69858e1c..25ce9ee1` | 0 | 0 | 4 (pre-existing) | ready to merge |

## shell — no findings

Scope: the resume memo (lane-s1/s1l/s1c, [EX195](../../experiments.md#ex195)), batch-s1 ([EX177](../../experiments.md#ex177)), memo-s2 and lane-s2b. Checked and found sound:
- memo admission compares the flash epoch and every retained non-flash page version, and checks the region epoch before indexing the saved record;
- decoder flush clears the memo; reset, region drop and tuning advance the region epoch;
- interrupts and WAITI are checked before memo admission;
- the owned-loop, foreign-loop and copy-resume rules hold;
- pricing, observation and probes decline;
- deferred hints are replayed on a declined memo;
- sites are read before `note_short` can replace the site vector;
- batch-s1 folds completed rounds before a deferred access and re-bounds after the open round.

Validation: 110,955 differential cases on `3b696862`, the same count with `resume_memo::turn` routed through the prepared entry (`run_memo`) first, and `ESP32SIM_VQ_NATIVE=1` virtual_stops 7 passed. Recommendation: keep the prepared-entry variation as regression coverage. It shipped in slice 5 `6b071959`, alongside the ordinary route.

## lookup — one minor

**M1 (minor): generic timer fixtures bypassed `refresh_event`.** event-s1 ([EX168](../../experiments.md#ex168)) caches the next CCOMPARE match in `Cpu::event_at` (wasm32). Test fixtures in `xtensa-lx7/src/core.rs`, `xtensa-lx7/tests/pricing*.rs` and `esp32s3/tests/{machine,virtual_stops}.rs` assigned CCOUNT/CCOMPARE directly. Transplanted into the WASM suite, the priced-timer fixture fails the suite-wide consistency assertion (`ccount/ccompare changed without refresh_event`); with a refresh it passes. These are test paths; no production writer was found. Fixed in slice 2 `eba3f049`: the fixtures go through `write_sr`, and the transplanted fixture is kept in `scheduler::event_writers`.

Checked and found sound:
- timer writes and the cached-event arithmetic (an independent seeded model agreed with the old scan in 1,000,000 comparisons, including wrap and CPI 1–100);
- wrapper-scoped helper tables and memory pointers (inner-s1);
- the no-region verdict and its coverage-epoch invalidation (hop-s2);
- named decoded resumes (alias-s1, including the leaf-cut third site);
- the mask-ROM epoch range (hop-s2b: it starts at ROM's second page, and every ROM writer moves the epoch).

Validation: 110,955 cases; native 133 passed.

## loops — one blocker (overflow-check builds only)

**B1 (blocker): `store_run` shifted before rejecting an unsupported stride.** `xtensa-lx7/src/jit/wasm_memory.rs:355–360` evaluated `1 << (off / width)` before the `stride <= 16` check. The hardware-loop body `s8i a6,a4,32; addi a4,a4,64` panics with `attempt to shift left with overflow` when overflow checks are enabled. The default release artifact did not panic or mis-execute. Fixed in slice 3 `e9979e2b`: `stride == 0 || stride > 16` is rejected right after the pointer step is recognized, and the reviewer's sparse-loop regression is in `loops::store_runs`. `CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=true tools/wasm-jit-test.sh` passes 110,685 cases at slice 3 and 110,955 at the tip.

Checked and found sound:
- loop entry and resume admission (`Hot.lp`, loop-s1, [EX196](../../experiments.md#ex196));
- copy cuts and retired counts;
- bulk completion at the quantum boundary, including LCOUNT `u32::MAX`;
- the whole-range proof before any write;
- watched pages, own code and the EX180 previous-page rule (store-s1/gen-s2, [EX197](../../experiments.md#ex197));
- register quads never crossing the 64-register wrap (gen-s1, [EX198](../../experiments.md#ex198)).

An expanded default-release matrix passed 115,791 cases.

## calls — one minor

**Minor: leaf-s1's multi-function module builder had no caller after rename-s1.** Dead code, not a state bug. Fixed in slice 4 `2478dff8`: `finish`/`module` take one body, rename-s1's wide locals are kept, and the leaf test asserts the module's function section holds exactly one function.

Directed tests added in scratch pass 110,957 cases on unmodified code:
- the ROM trampoline's JX literal flipped between the target and another leaf;
- a warmed nested leaf whose wrapper's RETW must raise WINDOW_UF8 because the caller frame is absent. Forcing the underflow guard false fails this test at turn 602 (no trap, interpreter `Exception(770)`).

Recommendation: commit both directed tests. They shipped in slice 4 `db7a3e50`.

Checked and found sound ([EX164](../../experiments.md#ex164)):
- ENTRY reach proof and overflow;
- RETW token and caller-frame checks;
- leaf page checks including nested callees, with CALLX/JX literals compared at run time;
- cuts, memory and resume (leaves are not region chunks);
- disabled FPU;
- loops, observers and pricing exclusions;
- leaf lifetime.

## watch — one major (not shipped)

**W1 (major): `rebuild_page_table` kept numeric watch slots, not the watched buffers.** A PSRAM resize moves the version bases of DROM and both RTC buffers, so an RTC code watch landed in a different, unwatched 4 KiB group. In the reviewer's sequence (watch `RTC_FAST_LOW + 0x880`, grow PSRAM by 4 KiB, write a byte there), the page's version stays 0 instead of 1. The old 64 KiB watch happened to cover that move. Fixed in slice 8 `69858e1c`, conservatively: after a non-initial rebuild in which any mark was set, every sub-block is watched. Tests `review_watch_survives_shifted_rtc_version_base` and `watch_survives_psram_shrink_and_regrowth` fail without the fix and pass with it. Production resizes run only before code executes (`wasm/src/lib.rs`, `cli/src/lib.rs`), so the conservative rule does not trigger in the timed workloads.

watch-s1 and this fix were not shipped: watch-s1 failed the TinyDraw control at q64 (four-pair recheck −0.68%, all slower) and at q256 (its removal gained TinyDraw 0.93%, all four pairs faster) (README). The shipped stack keeps the base's 64 KiB watch.

**Not fixed (pre-existing):** CPU decode and JIT caches hold version indices recorded before a resize, so a live resize after code has run is still unsafe without a cache reset. Neither the watch fix nor this round claims otherwise.

Checked and found sound for a fixed buffer layout ([EX110](../../experiments.md#ex110)):
- late watches drop published TLB entries;
- 4 KiB boundaries and aliases;
- neighbor marking covers EX180 and spanning stores;
- the generated store gate;
- DMA and store-s1 bulk bumps;
- flash/PSRAM mappings and the stable-page epoch;
- the 32-byte `TlbEntry` layout;
- native generated stores stay unconditional.

Validation: native 171 passed; 110,955 cases.

## final — no new findings

The final review compared every rebuilt slice tree with its trial stage. It confirmed the four earlier fixes within their scope (B1, W1, M1, the calls cleanup) and the removal of tailcut-s1 at slice 3. After that removal no `CODE_CUT` conversion survives and copy resumes stay intact. It also confirmed memo-s2's exit-site ordering at slice 5, the complete `REGION_STATS` renumbering at slice 6 and the quad local in the single-body builder at slice 7.

Per-slice gates:

| Slice | Commit | Native tests | Differential cases |
| --- | --- | --- | ---: |
| 3 | `e9979e2b` | 51 passed | 110,685 |
| 5 | `edd868c3` | 51 passed | 110,692 |
| 8 | `69858e1c` | 51 passed | 110,955 |

At the tip, the overflow-check suite passed 110,955 cases and `cargo test --release -p esp32s3` 173. Both leftover directed patches (calls, shell prepared entry) apply to the tip and pass. Recommendation: commit them, running the memo fixture through both the ordinary and the prepared entry. Both shipped in `x8/ship` (slices 4 `db7a3e50` and 5 `6b071959`); they are not in `69858e1c`. Stated limit: the watch fix does not make live resize safe (above).

## q256 — one blocker, one major (held, then fixed)

**Blocker: TIMG autoreload dropped the steps after an alarm within one tick.** Configure TIMG0 timer 0 to count up with autoreload, divider 2, alarm 32, then run exactly 256 instructions with batching and virtual quanta off. With APB = CPU/3 the timer should read `floor(256/6) − 32 = 10` at cycle 256. At q64 it reads 10; at q256 it reads 0. `TimerGroup::tick` advanced the whole tick, then replaced the count with `load`. The defect predates q256: on the unfixed model at q64, the fix's machine test reads 0 instead of 2 for a reload period shorter than a round and 40 instead of 38 counting down. q256 makes it common. It is shared by C3/C6.

Fixed by `2af5043b` (own commit, before the quantum change). The tick advances to the alarm, sets `int_raw` and reloads; the remaining steps are reduced modulo the reload period and applied from `load`; a one-shot alarm disarms and counting continues. New tests: `timer_long_tick_equals_single_ticks` (one long tick equals n single ticks across directions, dividers, crossings and load positions; over 100 crossings) and `timg_steps_after_an_alarm_survive_a_long_round` (machine level, q64 and q256). Both fail on the old model. No pin moves at q64 or q256.

**Major: the WASM round-batch equivalence test compared batching with batching.** `both_busy_rounds` set only `b.bb_max = 128`, and the wasm32 default is already 128. Fixed by setting `a.bb_max = 1` and asserting `a.bb_stats[0] == 0`; all 111,175 cases pass.

Checked and found sound or bounded:
- integer bounds at q256;
- peer wake and incomplete rounds;
- console, script, web-push and idle bounds;
- the approximate-timing path keeps its own quantum;
- the q64-pinned tests keep their meaning;
- device models (UART, I2C, touch, SPI/GDMA, SYSTIMER): late, never early.

Stated limits:
- the RTC watchdog feed-versus-expiry window widens with the round;
- the external C6 goldens were not run;
- TinyDraw's q256 total rests on the author's Node driver run, not the reviewer's.

Gates after the fixes (`5090bfec`): 462 native tests, 111,175 cases, virtual_stops 7 passed at 64 and 256.

## q256 follow-up — ready to merge

After native Pocket measured 5.38% slower at 256, the fix branch replaced the shared default with a per-target default: `cf573dc2` (wasm32 256, native 64; shipped as `a66e16bf`) and browser pins `25ce9ee1` (shipped as `9e8d3378`). The follow-up review of `69858e1c..25ce9ee1` found both earlier findings resolved:
- the original directed TIMG test reads 10 at both q64 and q256;
- the WASM reference asserts that it never batches.

The per-target default is applied consistently: `esp-soc/src/machine.rs` selects 256 only on wasm32, the setter is unchanged and `git diff x8/final..25ce9ee1 -- tests/golden cli` is empty, so native goldens keep their q64 pins. Tests that depend on round length pin 64 or loop over 64 and 256.

Gates:

| Check | Result |
| --- | --- |
| `cargo test --workspace --release` | 462 passed, 0 failed, 22 ignored |
| goldens | 14 passed without updates |
| `ESP32SIM_VQ_NATIVE=1` virtual_stops | 7 passed |
| `tools/wasm-jit-test.sh` | 111,175 cases |

**Verdict: ready to merge.** Remaining minor items, all pre-existing and none a q256 regression:
- The TIMG model keeps `ALARM_EN` set after an autoreload alarm and reloads on every crossing within a tick. Hardware clears the bit on an alarm (ESP32-S3 TRM, Timer Group chapter), so with no ISR between crossings the model reads 2 where hardware would read 37.
- The model does not detect an alarm crossing after a 54-bit wrap.
- Several docs still described a single 64 default. Updated in this commit: `docs/architecture.md`, `docs/decisions.md` and `docs/speed-plan.md` now state 256 on wasm32 and 64 natively.
- `wasm/tests/abi.rs` builds natively, so it runs at 64; only the Node suites exercise the wasm32 default.

External C6 goldens and benchmark workloads were not rerun in the review.
