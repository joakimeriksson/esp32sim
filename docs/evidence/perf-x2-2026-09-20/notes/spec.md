NO GAIN EXPECTED (prediction, not timing) for this conservative engine: K4/16/64 restore 69.93%/92.54%/98.44% of attempts; four gated candidates are queued, not adopted (`runs/spec2-profile-comparison6.json`, queue receipts below).

# EX150 spec2: exact both-busy windows, saved handoff

## Recovery and scope

All receipt paths below are relative to `/Users/alice/src/a/esp32sim-x2-spec`, branch `x2/spec`, unless absolute. The implementation is committed locally: engine `af2f1f23`, original gates/queue receipts `daa015ce`, tracking control and dispatch counters `4b2ac321`, control gates `c54cbf95`, comparable profile counts `da3078d1`. The standalone committed copy of this note is `runs/SPEC2-HANDOFF.md`. No parent session is needed to recover it. Artifact directory: `/Users/alice/src/a/esp32sim-x2/wasm/`; frozen jobs: `/Users/alice/src/a/esp32sim-x2/queue/jobs.jsonl`. Queue receipts: `runs/spec2-queue.txt`, `runs/spec2-tax1-queue.txt`.

The interrupted draft was retained as a census-only checkpoint (`36d0f324`), not an engine result. This retry stays under **EX150**. Related mechanisms are EX133/EX144 (idle-peer virtual quanta), EX134/EX157 (device deadlines) and EX047 (larger, nonexact quantum). The old `7bb0bbab` engine and `966d447f`/`8ae67ad0` tax emitter were inspected before implementation. The new engine replaces guest-address conflict keys, a wrapping undo ring and raw CPU copies with physical tracking, a bounded log and typed snapshots. Prior-engine review: `runs/spec2-prior-review.md`; catalog: `docs/experiments.md:157–181`.

## 1. New-base opportunity census: 30 guest seconds

On base **414c6e07**, 53,797,340 both-busy rounds were observed. Device deadlines are **not** still a four-quantum ceiling: 96.84% of rounds have a bound of at least 16 quanta. Core 0 touches device registers in 1,842,815 rounds (3.425%, 10,514,455 accesses); core 1 in 64,132 rounds (0.119%, 265,238 accesses). Source: `runs/spec2-census-summary.json`, `runs/spec2-census30.txt`; instrumentation: `esp-soc/src/machine.rs:759`, `esp32s3/src/bus.rs::vq_backstop`.

| Cap | Observational windows | Mean covered rounds/window |
|---:|---:|---:|
| 4 | 13,415,364 | 3.87 |
| 16 | 3,867,570 | 13.43 |
| 64 | 1,532,627 | 33.90 |
| 256 | 958,188 | 54.22 |

These are **device-only groupings of the ordinary schedule**, not achieved speculative windows. They exclude leader device-access rounds, end on follower device access and do not model memory conflicts, interpreter/helper refusals or core-local timer bounds. The new-base census passes the full 30-second instruction/console/frame gate (`runs/spec2-census-exact30.txt`). Build: `SPEC_CENSUS=1 bin/build.sh <tree> spec2-census --features jit-profile`; runner: `PROFILE=1 node tools/spec-run.mjs <wasm>`; summary: `node tools/spec-summary.mjs <output>`; commands must be niced per the brief.

## 2. What the engine actually implements

Core 0 runs up to K whole quanta, then core 1 runs the same count. A complete window commits only if tracked cross-core read/write pairs commute. Otherwise **both cores** and all logged memory/version cells are restored and the original scheduler replays the window. This is not the suggested leader-only partial rollback. Source: `esp-soc/src/machine.rs:700–754`.

- Data conflicts use physical host addresses, covering guest aliases. Instruction dependencies use physical page-version cells, including cached blocks and regions. Stores log old data and old versions before modification. The log refuses overflow before the next store; generation rollover clears the maps. Source: `emu-core/src/spec.rs`, `xtensa-lx7/src/jit/wasm.rs:554`.
- CPU snapshots inventory architectural fields exhaustively and preserve live owning caches; continuation cursors and compiled-instruction counts are restored separately. Source: `xtensa-lx7/src/state.rs::architectural_snapshot`, `xtensa-lx7/src/core.rs:19`, `xtensa-lx7/src/block.rs::BlockCursor`.
- Guest helpers, interpreter fallback, interrupt entry, TLB fill, decode/compile and unsafe cache changes refuse the window before untracked effects. This includes S32C1I and helper slow paths. Scalar and PIE generated memory accesses share the tracked probe; bus reads/writes, bulk access and unpriced access refuse speculation. Pure fused arithmetic is allowed. Device/script/web/run limits bound the window; observers, stubs, function probes, real-time pacing and modeled execution are excluded. Sources: `xtensa-lx7/src/block.rs::run_block_inner`, `xtensa-lx7/src/jit/wasm.rs::h_exec`, `esp32s3/src/bus.rs`, `esp-soc/src/machine.rs:571,700`.

### Added emitted WASM operations per access

The engine uses a **correctness-first callback**, not the proposed inline per-core generation-byte map. For both a scalar load and a store, the shared probe adds **23 encoded WASM operations**: three for the active-window guard, 19 for pointer/argument setup, indirect callback and refusal branch, then `end`. This excludes all work inside the Rust tracking callback; stores additionally pay undo logging there. The guard remains outside windows. The same probe covers a 16-byte PIE access. Source/count inventory: `xtensa-lx7/src/jit/wasm_memory.rs:216–230`.

The tracking-only control reprises the old best **mode 1: leader marks only**, now indexed by physical host address: **15 emitted operations per core-0 load/store, zero per core-1 access**. It never changes the 64-instruction schedule. This is a **lower-bound tracking-cost screen**, not a matched full-engine control: it omits follower checks, undo logging, snapshots and replay. Do not subtract its timing from engine timing to claim an isolated scheduling benefit. Sources: `xtensa-lx7/src/jit/wasm_memory.rs:208–215`, `emu-core/src/spec.rs:14–28`; old screen: `docs/evidence/perf-sweep-2026-09-20/closeout.md:62–67`.

## 3. Exactness and rollback gates

K4, K16, K64, forced-restore K4 and the tracking-only control all print **EXACT ok at 30 guest seconds**:

- Instructions: **10,073,833,775**; binary frames: **3,094**.
- Console SHA-256: `b9d9966e5d9d73984203c11846eb7f8c86507cb53ffe133c4e7e92dff66319d7`.
- Frame-content SHA-256: `847b5478d0a7b2a6648804b7ada0c0f5f4a756c427e36020779104f14e6f4fdf`.
- Zero panics and zero JIT failures.

Receipts: `runs/spec2-{k4,k16,k64,force,tax1}-exact30.txt`. The forced knob is `SPEC_FORCE_RESTORE=1`: **N=1**, every otherwise successful window is restored. In six guest seconds it forces 849,417 otherwise successful windows through undo/replay, among 2,824,588 total restores (`runs/spec2-engine-counters.json`). Arbitrary N is not implemented.

WASM differential tests pass **78,922 cases for each K**, including 19 directed speculative cases; default-off and the tax control pass **78,903**. Directed tests cover physical aliases, instruction dependencies, helper/cold-code/interrupt refusal, mid-block state, generated 1/2/4/16-byte undo and real S3 scheduler comparisons. Receipts: `runs/spec2-jit-tests.txt`, `runs/spec2-tax1-tests.txt`, `runs/spec2-test-commands.txt`; test sources: `xtensa-lx7/src/jit/wasm_tests/spec.rs`, `wasm/src/jit_tests_spec.rs`. Native tracker/snapshot and SoC tests also pass (`runs/spec2-focused-tests.txt`, `runs/spec2-native-tests.txt`). These gates are evidence for the tested contract, not a universal proof for all firmware.

## 4. Comparable work counters: six guest seconds

All four `jit-profile` runs use source `4b2ac321` with `SPEC_DISPATCH=1`, the same runner and the same six-second work/output reference: **2,034,559,010 instructions, 577 frames and identical console/frame hashes**. Base has no SPEC_K; candidates set it to 4/16/64. Dispatch totals include both ordinary `step_blocks` calls and direct speculative `core.run` calls. `run_inner` is wrapper calls plus chained hops; JIT counters include discarded speculative work. Raw: `runs/spec2-profile-{base,k4,k16,k64}-6.txt`; comparison: `runs/spec2-profile-comparison6.json`; counter sites: `esp-soc/src/machine.rs:352,725`.

| Mode | Machine dispatches | run_inner calls | Region budget exits |
|---|---:|---:|---:|
| Base | 38,370,376 | 76,340,012 | 14,704,062 |
| K4 | 35,950,557 | 70,791,579 | 12,395,510 |
| K16 | 37,816,043 | 75,572,060 | 14,171,039 |
| K64 | 38,501,802 | 76,692,507 | 14,702,019 |

| Mode | Attempts | Commits | Restores | Committed rounds | Mean rounds/commit |
|---|---:|---:|---:|---:|---:|
| K4 | 2,824,588 | 849,439 | 1,975,149 | 3,393,806 | 4.00 |
| K16 | 716,741 | 53,480 | 663,261 | 823,200 | 15.39 |
| K64 | 189,672 | 2,963 | 186,709 | 84,243 | 28.43 |

The K4 mechanism reduces dispatches by 6.31% and run_inner calls by 7.27%, but also logs 61,004,198 stores and restores 69.93% of attempts. K16/K64 lose most of the larger-window opportunity to refusal. This supports the **no-gain prediction for this implementation**, not a measured slowdown or rejection of the broader EX150 idea. Sources: comparison above and `runs/spec2-engine-counters.json`. No local wall time was treated as a benchmark.

## 5. Queued candidates

All four jobs are against plain `base.wasm`, under `AGENT=spec2`. No timing result was awaited or used. Queue receipts: `runs/spec2-queue.txt`, `runs/spec2-tax1-queue.txt`.

| Job | Artifact in shared wasm directory | Source commit | Tests / counter evidence | Predicted effect |
|---|---|---|---|---|
| `spec2-ex150-k4` | `spec2-final-k4.wasm` (`5e7ba389ea2f`) | `af2f1f23` | K4 engine; 30s exact, 78,922 cases; 69.93% restores, 6.31% fewer dispatches | Likely loss after tracking/restore cost; magnitude unknown |
| `spec2-ex150-k16` | `spec2-final-k16.wasm` (`21ec74c7339f`) | `af2f1f23` | K16 engine; same gates; 92.54% restores | No gain expected |
| `spec2-ex150-k64` | `spec2-final-k64.wasm` (`b9835afcac59`) | `af2f1f23` | K64 engine; same gates; 98.44% restores, more dispatches than base | No gain expected |
| `spec2-ex150-tax1` | `spec2-tax1.wasm` (`2c1fae3c544f`) | `4b2ac321` | Leader-mark-only lower bound; 30s exact, 78,903 cases; 15 emitted ops/core-0 access | Near-neutral or loss; no scheduling benefit |

The forced artifact `spec2-final-force.wasm` (`897542c248d3`) is validation-only, not queued. Clean rebuild checksums for the engine artifacts: `runs/spec2-clean-artifacts.txt`; control checksum/build: `runs/spec2-tax1-build.txt`.

## 6. Existing EX150 catalog row: append this revision, retain earlier results

| <a id="ex150"></a>EX150 | **Optimistic two-core execution; spec-tax tracking overhead; exact both-busy engine** | Earlier tax screens retained; conservative engine exact and queued, no gain expected (prediction) | Retry on 414c6e07 after EX157: 30s ordinary-schedule census finds 96.84% of both-busy deadline bounds >=16 quanta; device-only grouping means 13.43 at K16 and 33.90 at K64. New af2f1f23 engine uses typed CPU/cursor snapshots, physical data and instruction-version conflicts, bounded undo and full restore/replay on unsupported paths. K4/16/64 plus N=1 forced restore pass the 30s pinned instruction/console/frame gate; each K passes 78,922 differential cases. At 6s, restore rates are 69.93%/92.54%/98.44%; K4 dispatches 38.37M->35.95M and run_inner 76.34M->70.79M, before pricing tracking/snapshot/replay overhead. Forced K4 undoes 849,417 otherwise successful windows. 4b2ac321 adds the old best leader-mark-only control using physical indexing (15 emitted ops), exact with 78,903 cases; it is not the full engine tax. | Not adopted. Four AGENT=spec2 jobs: spec2-ex150-k4/k16/k64/tax1. No local timing claim. Evidence and limitations: `x2/spec:runs/SPEC2-HANDOFF.md`, `runs/spec2-profile-comparison6.json`, `runs/spec2-queue.txt`, `runs/spec2-tax1-queue.txt`. Preserve the earlier interruption as not a result; do not infer speed from tax-only screens. |

## 7. What to do next

Retain the exact engine as a reference, not a production optimization. The deadline premise is confirmed, but the conservative enders prevent most large windows from committing. Before another EX150 revision, count abort reasons: return helpers currently refuse, code-version cells introduce coarse false sharing and speculative code-page TLB misses refuse. These are source-inspection hypotheses, not measured shares. The reviewed suggestions are in `runs/spec2-engine-review.md`; production logic is in `xtensa-lx7/src/jit/wasm.rs`, `emu-core/src/spec.rs` and `esp32s3/src/bus.rs`.

A faster follow-up needs an exact contract for any newly admitted helpers and distinct code-dependency tracking before reducing false conflicts. Removing conservative guards merely to improve the commit rate would invalidate the current proof. The inline per-core generation-map design, leader-only rollback and a matched full tracking-cost control remain unimplemented. Keep using the exact30 frame/console/instruction gate and forced undo tests for any retry (`runs/spec2-engine.md`).

## Preserved interruption history (before spec2)

INTERRUPTED, NOT A RESULT: the EX150 round-2 retry (agent `spec`, Sep 21) was killed after 11m54s by a provider-side
usage-policy classifier block ("violative cyber content"; auto-retry exhausted). The idea was neither tested nor refuted.
Do NOT record this as a negative or "no gain" outcome in docs/experiments.md. Another agent may pick it up later.

State left behind (nothing committed, nothing built, nothing queued):
- Worktree /Users/alice/src/a/esp32sim-x2-spec, branch x2/spec, HEAD = base 414c6e07, DIRTY (left untouched on purpose):
  modified emu-core/src/lib.rs, esp-soc/src/lib.rs, esp-soc/src/machine.rs, esp32s3/src/bus.rs, wasm/src/lib.rs;
  new emu-core/src/spec.rs, tools/spec-run.mjs, runs/. This looks like the start of porting round 1's census/engine
  (exp/spec 7bb0bbab in /Users/alice/src/a/esp32sim-exp-spec) to the new base. Unreviewed: treat as a draft.
- Session log: /Users/alice/.pi/agent/sessions/--Users-alice-src-a-esp32sim-x2-spec--/2026-09-20T17-58-44-104Z_7c78e235-133d3256-e3e8d719-5e83.jsonl
- The task text given to the agent: see the coordinator session; design hints are also summarized below.

Why the retry is still justified (coordinator census, notes/census.md): the device-deadline cap that made round 1 predict
MODEST is gone on the new base: virtual-quanta runs average 131 quanta (374,850 runs / 49.0M quanta) instead of 3.97, the
cadence256 bounds are negligible, while both-busy ping-pong is still 47.9% of guest time and 64% of core-1 region exits
are budget exits. Prize estimate from EX047: quantum 512 gave -17% wall while retiring +7.5% instructions. Enemy: the
tracking tax (round-1 tax-only screens -12.4% .. +1.7%).

Design hints handed to the agent (unverified): 64-256 B conflict granularity (4 KiB conflicts 13-22% of windows);
per-core code caches let core 0 carry only leader marks and core 1 only follower checks; if the follower checks BEFORE
each access, only the leader needs an undo log + register snapshot and a deterministic solo replay; anything not cheaply
trackable (interpreter blocks, helpers, DMA/host writes, S32C1I) should end the window instead; leader = device-heavy core 0;
gate = exact.mjs EXACT ok, also with a forced-conflict knob so rollback is exercised.
