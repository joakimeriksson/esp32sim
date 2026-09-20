QUEUED (prediction: stack +2 to +4%, each single +0.2 to +1.5%): a per-step profile of the Rust path found six cheap, exact removals; the biggest are 69.4M needless slow region lookups and 7 stack stores per own-module call that only served a panic message.

# EX168 dispatch: profile-led diet of the Rust path between scheduler and generated code

Agent `dispatch`, worktree `/Users/alice/src/a/esp32sim-x2-dispatch`, base 414c6e07. All numbers: pocket-tank, Node on the shared M1 Pro (never a benchmark). Counters are for 30 guest s (pinned 10,073,833,775 instructions), profiles for `SECS=8`.

## 1. What I measured

### 1a. Call counts (jit-profile build `dispatch-jp0`, commit 2b559491, raw `runs/dispatch/jp0.txt`, line `[ex153-dispatch]`)

| quantity (both cores, 30 guest s) | count |
|---|---:|
| `run_block_inner` dispatches | 184.5M (98.9M hit the resume slot, 4.83M have a raw pending interrupt bit, 0.63M waiting) |
| `jit::run` wrapper calls | 150.1M |
| `run_inner` calls (wrapper + chained hops) | 372.3M (222.3M chained) |
| ... resumed, `entry != 0` (straight to own module) | 98.5M |
| ... hot facts admitted, region entered | 117.0M (3 stale-page misses; 433M page compares = 3.7 pages per call; 46% have LCOUNT != 0) |
| ... hot facts fail: epoch miss / budget < len / bloom / loop conflict | 61.0M / 69.4M / 0 / 26.4M |
| slow region lookup entered | 156.8M, of which only 20.7M enter a region (20.66M of those through `r.loops.contains`) |
| own-module calls (`run_block_body`) | 234.8M (63% of `run_inner` calls) |
| chain loop stops: exit kind / budget / HELPED / no target / slot not ready | 99.3M / 12.5M / 24.3M / 13.9M / 22.6K |

So there are 2.5 generated calls per wrapper call (372.3M / 150.1M), and the region fast path is the minority: two of three generated calls are single-block modules.

### 1b. Per-step attribution (the table)

Build `dispatch-cpu1` = `--features cpu-profile` + temporary `#[inline(never)]` splits (patch kept at `runs/dispatch/attribution.patch`, not committed; `census()` atomics disabled for it). Profile `runs/dispatch/prof1`, summarizer `runs/dispatch/sum2.mjs`. Share = self time, % of non-idle samples (total 17.53 s). ns/call = share x 17.53 s / (calls x 8/30). Inlining distorts and V8 itself inlines tiny wasm functions (the call trampoline `attr_call_gen` shows 0.05%, so call cost lands in its caller): a ranking, not a measurement.

| step | share | calls / 30 s | ~ns/call | what is in it |
|---|---:|---:|---:|---|
| generated code | 41.4% | | | |
| `run_block_body` (own module) | 8.27% | 234.8M | 23 | slot fetch, indirect call, offset reconstruction, `note_pc`, spills for the panic closure |
| `jit::run` + `run_inner` shell | 7.43% | 150.1M / 372.3M | 13 per run_inner | HELPED store/load, chain loop control, 36-byte `hinted` Helpers rebuild per run_inner (wasm.rs:562), `Option<FastMem>` unpack, hot indirect call |
| slow region lookup | 5.63% | 156.8M | 24 | RefCell borrows, `covered_by` liveness, Vec page loop, then mostly nothing: 136M of 157M end in the own module |
| `run_decoded` | 4.10% | ~150M | | 3 CCOMPARE deadlines, `ready`, cache take/put, `fast_mem()` |
| `run_unmodeled` | 3.62% | per round | | round bookkeeping |
| hot admission guard | 2.85% | 273.8M | 6.8 | borrow flag, epoch/budget/bloom/loop compares, 3.7-page version loop |
| `step_blocks` | 2.77% | 184.5M | | probes, observe, refresh_irq |
| chain lookup (`chain_target` + slot) | 2.03% | 236.2M | 5.6 | hash, 32-byte entry, 2 versions, arena bounds check + first-insn match |
| `find_block` | 1.95% | 184.5M | | |
| `loop_len` | 1.25% | 234.8M | 3.5 | out-of-line, Option returned through memory |
| `after_round_rest` | 0.70% | per round | | 304-byte frame, dyn `display_push_hz()`, `i64.div_u` (base.wat 19262-19330) |
| `check_interrupts` | 0.63% | 184.5M | | out-of-line for a test that is zero 97.4% of the time |
| accounting (`advance_ccount` etc.) | 0.54% | | | |
| hot result decode + site + `note_pc` | 0.32% | 117.0M | | |
| **host dispatch total** | **42.3%** | | | |

Unsplit cpu-profile run on base (`dispatch-cpu0`, `runs/dispatch/prof0`): run_inner 21.41%, run_block_inner 5.36%, jit::run 3.94%, run_unmodeled 3.39%, find_block 2.20%, step_blocks 1.99%, after_round_rest 0.87%, check_interrupts 0.55%; dispatch 39.9%, generated 42.9%.

### 1c. Wasm inspection (`wasm-tools print wasm/base.wasm` -> `runs/dispatch/base.wat`, helper `runs/dispatch/fn.sh`)

- `run_inner` (func 88, line 249396): 352-byte frame, 2492 lines, 128 stores. Prologue on every call: `CACHE_PROBES` load, then 8 stores rebuilding the 36-byte `Helpers` on the stack (EX030's copy, still there, 372M times).
- Own-module path: `entry`, `budget`, `looping`, `initial_lcount`, `result`, `done` are stored to the frame (offsets 288-308, 348) on every call because the `unwrap_or_else(|| panic!(...))` closure at wasm.rs:785 captures them by reference. `loop_len` is a real call returning `Option<usize>` through memory. Two unwrap checks survive for census-only math.
- Hot path: RefCell borrow flag load + 2 stores, a 9-way slice bound check, the page loop (2 loads + bounds check + load + compare per page).
- `step_blocks` (func 9) is 5346 lines with run/run_decoded/build inlined, 112-byte frame; `run_unmodeled` 13038 lines.

### 1d. Candidates (all on top of 2b559491 = base + feature-gated counters)

| | commit | change | evidence |
|---|---|---|---|
| s1 | f92becfb | Epoch-valid, page-current cached facts that fail budget/bloom go straight to the own module (wasm.rs:608 on the stack branch). One body copy, no second epoch test. | 69,377,036 fast rejects measured (`dsp[25]`, `runs/dispatch/jp1.txt`); slow lookups 156.8M -> 87.4M (-44%); step share 5.63% |
| s2 | 6df7cb6a | Cold by-value `bad_offset()` instead of the by-reference panic closure; census-only math behind `cfg!` | `run_inner` stores 128 -> 98, lines 2492 -> 2338, frame 352 -> 336 (`fn.sh`); 234.8M calls; step share 8.27% |
| s3 | 0c6b6496 | `loop_len` tests `lcount == 0 || lbeg != pc` inline, rest out of line | 192,876,030 of 234,848,384 body calls (82%) exit inline (`dsp[29]`); step share 1.25% |
| s4 | 0354b267 | Per-round web/pacing tail: inline compare against cached `push_every`; board call, division, push and pacing clock `#[cold]` | `after_round_rest` no longer exists as a function (inlined guard); share 0.70-0.87% -> 0 |
| s5 | 0641c8b8 | EX152 s2 isolated: inline raw-pending AND before `check_interrupts` (cherry-pick of 4d90d321) | 97.4% of 184.5M dispatches skip the call; `check_interrupts` 0.55% -> 0.04% (`prof2`) |
| s6 | 9aff3dc0 | `Entry.chain` flag (fits padding, size asserted 32) replaces the arena bounds check + first-instruction match in `chain_target` | 236.2M lookups; step share 2.03% |
| stack | 580970f4 | s1..s6 + feature-gated counters | cpu-profile `dispatch-cpu2`, `runs/dispatch/prof2`: dispatch share 39.9% -> 36.9%, dispatch/generated 0.93 -> 0.83; every base dispatch counter identical |

Exactness of s1: while `hot.epoch` is current no region was dropped, so the slow lookup selects the same (owner, chunk) the facts were copied from (a covered head never forms its own region while its cover lives; an own region stays until dropped). With current pages the slow path then fails the same `budget >= lens[k] && bloom == 0` test and does nothing. Stale pages still take the slow path and drop the region. Loop conflicts still take the slow path because `r.loops.contains` admits 20.66M of 26.4M of them.
Exactness of s4: same condition, the interval is recomputed from the board at every `run()`/`run_until_cycle()` entry and before every push. s5: `check_interrupts` returns None before any side effect when the raw AND is zero (exec.rs:49-50). s6: the arena is append-only until `flush`, which empties all entries.

Gates: `exact.mjs` 30 s `EXACT ok` for all seven artifacts; `tools/wasm-jit-test.sh` PASS 78,903 cases on all seven commits (`runs/dispatch/jittests.txt`).

### 1e. Killed or not queued

- **Global "any code page changed" epoch: DEAD.** Generated stores bump `page_ver` directly through the `versions` pointer (wasm_memory.rs:144, bus.rs:635), so such an epoch would move on every RAM store. A flash-only epoch (flash versions move only in bus.rs:279 and `note_written`) might remove most of the 433M page compares, but needs a proof about PSRAM-mapped code; not attempted.
- `hinted` Helpers rebuild per run_inner (8 stores + load, 372M times): same effect as EX030's be64c0e7 on this workload; not repeated.
- Slow path rewrites the 112-byte `Hot` on every one of its 20.7M region calls (wasm.rs:689) even when already valid. Caching the admitted loop pair was EX136's extension 6b844713 (no gain); a plain "skip the refill when epoch-valid" was not built (budget).
- RefCell -> raw read of `hot`: 3 memory ops of ~60 on the hot guard; not worth a slot.
- EX152 s3 (quiet after_round) never fired in the browser: it required `web.is_none()`, and wasm always sets `m.web` (wasm/src/lib.rs:156). s4 is the version that fires.
- `find_block` already has a last-hit cache (the resume slot, 54% hit) in front of a direct-mapped table; nothing to add.

## 2. Queued candidates (all against plain base, AGENT=dispatch)

| job | artifact (sha256 prefix) | commit | tests | counter evidence | predicted |
|---|---|---|---|---|---|
| dispatch-s1 | dispatch-s1.wasm (ad34620d7609) | f92becfb | cached rejection skips slow region lookup | 69.4M of 156.8M slow lookups gone; 5.6% step | +0.5 to +1.5% (EX136's a5b251ef lost 7/7 on a busy host: could repeat) |
| dispatch-s2 | dispatch-s2.wasm (e4e8d03b71be) | 6df7cb6a | no address-taken locals in own-module path | 30 fewer stores in run_inner; 234.8M calls | +0.5 to +1% |
| dispatch-s3 | dispatch-s3.wasm (0a6678f4fada) | 0c6b6496 | inline loop_len refusal | 192.9M calls avoided; 1.25% step | +0.3 to +0.8% |
| dispatch-s4 | dispatch-s4.wasm (3163b7b02361) | 0354b267 | per-round tail without call, dyn call, div | 0.7-0.9% step -> 0 | +0.3 to +0.7% |
| dispatch-s5 | dispatch-s5.wasm (9f8b3ff191e2) | 0641c8b8 | EX152 s2 isolated | 179.6M calls avoided; 0.55% -> 0.04% | +0.2 to +0.5% |
| dispatch-s6 | dispatch-s6.wasm (92526aeb2403) | 9aff3dc0 | chainable flag | 236.2M lookups shorter; 2.0% step | +0.2 to +0.5% |
| dispatch-s1s2s3s4s5s6 | dispatch-s1s2s3s4s5s6.wasm (633a7182076a) | 580970f4 | all six | dispatch share 39.9 -> 36.9% (Node) | +2 to +4% |

Singles other than s1/s2 are below the M3 A/A floor seen in EX152/EX165 (0.05-2.5%); only the stack is expected to separate cleanly.

## 3. Catalog row (ready to paste)

| <a id="ex168"></a>EX168 | **Profile-led diet of the Rust dispatch path**<br>dispatch; run_inner; run_block_body; slow region lookup; after_round; chainable flag | Queued (awaiting M3) | Base 414c6e07. Per-step attribution (Node cpu-profile + temporary inline(never) splits, 8 guest s): own-module wrapper 8.3%, run/run_inner shell 7.4%, slow region lookup 5.6%, run_decoded 4.1%, run_unmodeled 3.6%, hot guard 2.9%, step_blocks 2.8%, chain lookup 2.0%, find_block 2.0%, loop_len 1.3%, after_round_rest 0.7%, check_interrupts 0.6%; host dispatch 42.3% vs generated 41.4%. Counters per 30 guest s: 372.3M run_inner calls, 234.8M own-module, 117.0M hot region, 156.8M slow lookups of which 20.7M enter a region. Six exact singles: s1 cached budget rejection (69.4M slow lookups removed; retry of EX136's a5b251ef in a one-body-copy shape on the EX165 base), s2 by-value cold panic (run_inner stores 128->98), s3 inline loop_len refusal (82% of calls), s4 per-round web tail without call/dyn call/division (EX152 s3 could never fire in the browser), s5 EX152 s2 isolated, s6 chainable flag in Entry padding. Stack: dispatch share 39.9->36.9% under Node. Dead: global code-page epoch (generated stores bump versions directly). All seven pass 30 s exactness and 78,903 differential cases. No timing yet. | Not adopted; queued as dispatch-s1..s6 and dispatch-s1s2s3s4s5s6 against base. Branches `x2/dispatch` (stack 580970f4) and `x2/dispatch-s1..s6`; note `/Users/alice/src/a/esp32sim-x2/notes/dispatch.md`; raw `runs/dispatch/`. Related: EX136, EX165, EX152, EX030, EX036. |

## 4. Next step and risk

- If the stack wins: adopt s1+s2+s3 first (largest shares, smallest diffs). Exactness risk is low: every change is host-side and the dispatch counters of the stack equal base's to the last digit (`jp0.txt` vs `jp1.txt`).
- Then the two unbuilt items with counts behind them: skip the `Hot` refill on the 20.7M loop-admitted slow calls, and a flash-only version epoch for the 433M page compares (needs a proof for PSRAM-resident code: that one is the exactness risk).
- The structural finding for others: 63% of generated calls are single-block modules at ~23 ns of Rust each, and 98.5M of them are resumes after a budget cut. Fewer, longer calls (tailchain EX167, longer budgets) attack that better than any further shaving here.
