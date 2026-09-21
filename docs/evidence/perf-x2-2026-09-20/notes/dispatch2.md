(•̀ᴗ•́)و QUEUED — nine preserved EX168 wave-2 candidates are ready for M3 sync by frozen hash; all nine pass the receipt audit below. Incremental speed remains unmeasured. Keep t5 unqueued: its workload checks passed, but its flash-epoch proof was not completed before interruption.

# EX168 wave 2: dispatch2 recovery handoff

Paths below are relative to `/Users/alice/src/a/esp32sim-x2` unless labeled source. Source worktree: `/Users/alice/src/a/esp32sim-x2-dispatch2`.

## What to sync

Sync the **existing nine dispatch2 jobs**, their frozen `queue/wasm` files, source revisions and receipts below. Do not rebuild from the current worktree tip: `x2/dispatch2` points to queued full stack `b1d77c28`, but the checked-out branch is `x2/dispatch2-stack-t5` at diagnostic commit `3f68df78`. The audit left both unchanged (`runs/dispatch2/audit.json`, `head` and `status`; `git show-ref --heads`).

**Queue corrections:** no revision, artifact hash or baseline correction is needed for these nine jobs. Suggested coordinator-only note clarification: the `dispatch2-stack-t1t2t3t4` queue note's approximately 2.0M slow lookups comes from the **t1+t2 diagnostic**, not a separate full-stack census. Also, 20.66M saved Hot refills is the opportunity for t3 in isolation; t2 has already removed those calls in the full stack. No queue or artifact sidecar was edited. No jobs were added, no remote sync was performed and no implementation or git history was changed.

## Provenance and correctness audit

Audit command: `node runs/dispatch2/audit.mjs`. Machine-readable results: [`runs/dispatch2/audit.json`](../runs/dispatch2/audit.json). This is a receipt audit, **not a fresh build or test run**.

For each of the nine jobs, the audit verifies:
- Current original artifact SHA-256 equals the frozen queue artifact SHA-256.
- Queue revision equals the `.rev` sidecar; every `.dirty.patch` is empty. Git commit objects resolve; parents and tree hashes are recorded in `audit.json`.
- The preserved 30-second exact receipt matches the full reference fields, including frame-content SHA-256, and reports zero panics and zero JIT failures.
- A matching revision header and `PASS: 78903 WASM differential cases` exist in `runs/dispatch2/jittests.txt`.

All nine current hashes also occur in the original build outputs in session **S**, providing a historical bridge from source/build to today's frozen bytes:
`~/.pi/agent/sessions/--Users-alice-src-a-esp32sim-x2-dispatch2--/2026-09-20T18-44-57-309Z_32455a2a-86ce3b70-0be3377e-283f.jsonl`.
Session S lines 128–129 record all nine enqueue commands and acknowledgments. Source/build recipe is `bin/build.sh`: release wasm32, default features, inline threshold 4000 and no wasm-opt. Default `cache-inline` is enabled (`source wasm/Cargo.toml:22–26`). The queued artifacts are not the `jit-profile` or `cpu-profile` builds.

### Queued candidates

For each job `J`, artifact = `wasm/J.wasm`; exact receipt = `runs/dispatch2/exact-J.txt`. Differential line ranges refer to `runs/dispatch2/jittests.txt`. Every row has **strict 30-second exact PASS** and **78,903 differential cases PASS**.

| Job | Commit | Source composition / test | Differential lines | Counter evidence and expectation |
|---|---|---|---|---|
| `dispatch2-stack-t1t2t3t4` | `b1d77c28` | wave-1 s1–s6 + t1+t2+t3+t4 | 1–3 | t1+t2 census: 87.42M→2.00M slow lookups; profile supports a possible small incremental gain, not a timing result |
| `dispatch2-stack-t1` | `2bcbb222` | wave-1 stack + negative region verdict | 4–6 | 59.03M slow lookups avoided; speed unknown |
| `dispatch2-stack-t2` | `5d13993e` | wave-1 stack + whole loop list, admits and rejects | 7–9 | t1+t2 census moves 20.66M admits into hot guard; isolated speed unknown |
| `dispatch2-stack-t3` | `9ae0e2e3` | wave-1 stack + skip identical Hot rewrite | 10–12 | baseline identifies 20,661,881 redundant fills; speed unknown |
| `dispatch2-stack-t4` | `b3e14bfc` | wave-1 stack + conditional Helpers copy | 13–15 | 372.32M run_inner calls exposed to copy avoidance; speed unknown |
| `dispatch2-t1` | `946a0ad9` | base + census parent + t1 | 16–18 | same negative-verdict mechanism; no separate single census retained |
| `dispatch2-t2` | `a9988df6` | base + census parent + admit-only t2 | 19–21 | does not include stacked t2's rejection shortcut; speed unknown |
| `dispatch2-t3` | `7c0dfeb7` | base + census parent + t3 | 22–24 | same refill opportunity; no separate single census retained |
| `dispatch2-t4` | `e00610ff` | base + census parent + t4 | 25–27 | same Helpers-copy mechanism; no separate single census retained |

The four base singles have parent `2b559491` (feature-gated census on base `414c6e07`). The four stack singles have parent `c46bfea5` (wave-2 census on wave-1 stack `580970f4`). Full-stack ancestry is `c46bfea5 → 2bcbb222 → e1d1f395 → ee3ee319 → b1d77c28`. Every t1–t4 mechanism diff changes only `xtensa-lx7/src/jit/wasm.rs`; the inherited wave-1 stack has the broader changes described in [`notes/dispatch.md`](dispatch.md). These are not nine unrelated implementations.

### Frozen SHA-256 inventory

Each queue filename is the first 12 characters of its full hash below plus `.wasm`. Full revisions, commit tree hashes and current exact fields are in `audit.json`.

| Job | Artifact and frozen SHA-256 |
|---|---|
| `dispatch2-stack-t1t2t3t4` | `d1154098343830354fda638636294315ccbf494817d0bbf196a0929c14228773` |
| `dispatch2-stack-t1` | `eec0284388590574b3aa4fcd056e4377a85ed17f493dfaa17393479d3e45dbf6` |
| `dispatch2-stack-t2` | `c4ab8143551c661d92596e17c6e3f2574f30abb6c8c5cfc6ec499c46974da758` |
| `dispatch2-stack-t3` | `1912470f05c96d97329ec31646c751976ce21698ad801cd4e339476db65babb3` |
| `dispatch2-stack-t4` | `56b55dbb85b2b305dbe2419eee7a852f7dc9adcbf91369b3a69fdaa0b78a8b72` |
| `dispatch2-t1` | `bfaad70f6b40b5cdc24f9f7ddd09b9076fc0efba344d9fbbe66524084424e46f` |
| `dispatch2-t2` | `a0fc1d49825a9acb66d8b7b0da84a35ebd404b54d9094e66bbb37d14f01987b5` |
| `dispatch2-t3` | `69c2c22256af492bee3f2a0f7e5bcf607de3308767657ce330074d42b0fa2c50` |
| `dispatch2-t4` | `7cf2fc5c2b1bb4fb65295afa3d5f22a2f233e8284d2a6c8aaf5cc52069e68185` |

All jobs use frozen base `queue/wasm/cfbf86c55386.wasm`, byte-identical today to `wasm/base.wasm`, full SHA-256 `cfbf86c5538646a47d9614469463ffbc666e391e1c69ee24f3ecb9e737567117`.

### Exact contract and limits

All nine receipts and `notes/exact-ref-pocket-tank-30s.json` agree on:
- Instructions: **10,073,833,775**; binary frames: **3,094**.
- Console SHA-256: `b9d9966e5d9d73984203c11846eb7f8c86507cb53ffe133c4e7e92dff66319d7`.
- Frame-content SHA-256: `847b5478d0a7b2a6648804b7ada0c0f5f4a756c427e36020779104f14e6f4fdf`.
- `secs=30`, `EXACT=true`, `EXACT ok`, `panics=0` and `jitFailures=0`.

`bin/exact.mjs` hashes the concatenated binary payloads in drain order and separately compares frame count; this is stronger than frame count alone, but not a per-frame-boundary or full machine-state hash. Its cached reference does not bind firmware, glue or baseline bytes by hash. The historical gates name artifact paths rather than embedding artifact hashes; session build hashes plus the unchanged frozen bytes provide the provenance link, not a cryptographic test attestation.

The differential runner (`runs/dispatch2/jittests.sh`) builds each revision with `jit-tests`, then runs the Node suite. All nine entries report a completed build and **81,884 compiled modules released**. This is a separate feature build, not a test of the production wasm binary itself. The runner lacks `pipefail` and pipes output through `tail`; its exit status alone is weak evidence. Here the explicit successful build lines and PASS summaries are the receipts. No new differential run was needed to recover them.

## Mechanisms and measured opportunities

This continues **EX168**, not a new experiment ID. Related catalog entries: **EX136** cached region facts and the unsuccessful one-loop-pair extension `6b844713`, **EX165** in-place admission facts and **EX030** conditional helper decoration `be64c0e7`. Their prior results remain in `source docs/experiments.md` (EX030 line 54, EX136 line 160, EX165 line 189). The original session inspected the preserved earlier changes before implementing; this audit inspected the current commit diffs without retrying them.

- **t1, negative region verdict:** cache “no own region, no live cover, formation tries spent” under the coverage epoch. Coverage insertion/reset invalidates the stamp; removals cannot create a previously absent cover. Source: `git show 2bcbb222` and `git show 946a0ad9`. Unlike EX136's positive-entry rejection, this avoids repeated lookup for heads without any region.
- **t2, complete loop list:** cache up to three `(LEND, LBEG)` pairs. The stack variant takes the slow path if the whole list does not fit and otherwise decides both admission and rejection. The base single only admits a matching pair; missing/oversized lists still use the slow lookup. This differs from EX136's one-pair extension and now sits on EX165's in-place facts. Source: `git show 5d13993e` and `git show a9988df6`.
- **t3, no redundant refill:** skip copying facts when the region epoch is current, relying on that block still selecting the same immutable owner/chunk. Current-page checking remains ahead of refill. Source: `git show 9ae0e2e3` and `git show 7c0dfeb7`. Counter evidence identifies the opportunity, not isolated timing.
- **t4, Helpers copy only with a cache view:** reuse the caller's helper pointer otherwise. Source: `git show b1d77c28`, `git show e00610ff`; `source wasm/Cargo.toml:22–26` confirms the feature is on by default. This is explicitly an EX030 retry on the new base/stack, not a new mechanism. The prior 0.67% single noisy screen remains unconfirmed; the historical session preserves WAT inspection in `runs/dispatch2/dispatch2-t4.wat` and `dispatch2-stack-t1t2t3t4.wat`.

Counters below are aggregate across both cores, printed twice by the census runner: **do not sum the two `[ex153-dispatch]` arrays**. Raw files: `runs/dispatch2/jp0.txt`, `jp1.txt`, `jp2.txt`, `jp5.txt` and `jp5b.txt`; parsed arrays in `audit.json`.

| Counter | Wave-1 stack `jp0` | +t1 `jp1` | +t1+t2 `jp2` | +t1–t5 `jp5` |
|---|---:|---:|---:|---:|
| run_inner calls, DSP[0] | 372,317,495 | 372,317,495 | 372,317,495 | 372,317,495 |
| slow lookup entries, DSP[26] | 87,415,678 | 28,388,561 | 2,002,018 | 2,008,580 |
| hot region calls, DSP[9] | 117,021,515 | 117,021,515 | 137,683,396 | 137,681,773 |
| slow admitted region calls, DSP[27] | 20,687,864 | 20,687,864 | 25,983 | 27,606 |
| epoch-current same-chunk refill opportunities, DSP[52] | 20,661,881 | 20,661,881 | 0 | 1,623 |
| negative verdict skips, DSP[69] | 0 | 59,027,117 | 59,027,117 | 59,027,117 |
| hot active-loop admits, DSP[70] | 0 | 0 | 20,661,881 | 20,661,864 |

`jp0` reports 30 region drops (DSP[51]), 14 cache resets (DSP[68]), zero >8-page admitted misses (DSP[50]) and zero epoch-current different-chunk refill opportunities (DSP[53]). The plain-base 156.8M slow lookups comes from wave-1 `runs/dispatch/jp0.txt`, not wave-2 `jp0.txt`.

For profile attribution, the preserved back-to-back **8-second Node cpu-profile** runs compare wave-1 `dispatch-cpu2` with full t1–t4 `dispatch2-cpu3`: run_inner share **20.67%→18.65%**, aggregate dispatch share **37.73%→36.78%**, dispatch/generated ratio **0.852→0.830**. Receipt: `runs/dispatch2/audit-profile.txt`, reproduced from `ratio.mjs` and `prof2b`/`prof3b`; session S line 154 records the same values. These are sample shares on a shared host, not browser speedups. No isolated slow-lookup attribution or individual timing result was recovered. The prior agent's approximate +0.5–1% incremental-stack expectation was a prediction, not a demonstrated gain; this audit makes no numerical speed prediction.

## t5 disposition: preserved, unqueued and not adoption-ready

**Do not include t5 in this M3 timing batch.** It was built and tested before interruption, but never enqueued (session S ends after its diagnostic run; `queue/jobs.jsonl` has exactly the nine t1–t4 dispatch2 jobs). There is no failing full-workload gate to report and no completed general proof to certify.

| Preserved t5 artifact | Revision | SHA-256 | Receipts |
|---|---|---|---|
| `wasm/dispatch2-stack-t1t2t3t4t5.wasm` | `a294db4c8545dfa0b47ba3dc1d64aeed398cf798` | `40e20293351a9b84b14fa6349966ac68ce35eabdb4b1e7bddbf5eb253a6f1ed5` | strict 30-second exact log of same stem; differential lines 28–30 |
| `wasm/dispatch2-t5.wasm` | `739b1c2a714e3e4e1ca757c56f2bf2dac8ba2b57` | `12e9e0878a3e0ec1a622e74ec4086e68d1354e3b6319333776aee84917c43645` | strict 30-second exact log of same stem; differential lines 31–33 |
| `wasm/dispatch2-jp5b.wasm` (diagnostic only) | `3f68df78462f5876f0455e9f39175e1cbd256915` | `b0545e0d0b1464e0e66d571969f0f92a815955647bf7419cb47cce28b5fe673e` | `jp5b.txt`; session S lines 158–159 |

Both production t5 logs have the same full instruction/console/frame contract listed above. The differential logs each report 78,903 cases. Session S lines 152–157 also preserve the native S3 `bus::` test command at `a294db4c` and completion: **71 passed, 0 failed, 46 filtered out**. Only the summarized completion was retained there, not a standalone full native-test log.

### What t5 implements and what remains uncertain

It replaces flash-index page comparisons in cached Hot facts with a bus epoch and retains per-page checks for other memory. Source `git show a294db4c` changes `emu-core/src/bus.rs`, `esp32s3/src/bus.rs` and `xtensa-lx7/src/jit/wasm.rs`:
- Bus defaults advertise an empty stable range (`source emu-core/src/bus.rs:111–118`). S3 advertises `[ver_base[FLASH], ver_base[PSRAM])` and its epoch (`source esp32s3/src/bus.rs:642–646`). PSRAM code is not omitted just because it is MMU-mapped.
- MMU mapping changes bump versions and the epoch (`source esp32s3/src/bus.rs:274–283`). `bump` and `note_written` also advance it for flash-index touches, including the predecessor-page case at the PSRAM boundary (`:344–365`). SPI dirty ranges are replayed through `note_written` (`:443`).
- Flash TLB entries are not writable (`source esp32s3/src/bus.rs:322`); emitted fast-store probes require writable entries and `record_store` updates the selected version directly (`source xtensa-lx7/src/jit/wasm_memory.rs:195–228`). This is the reason a global all-code-page epoch was rejected in wave 1: generated RAM writes bypass a bus epoch.
- Hot refill first validates all region pages, then omits only the advertised stable range; admission checks the cached flash epoch and remaining pages (`source xtensa-lx7/src/jit/wasm.rs:625–630, 742–750, 777–792` at `3f68df78`).

These are inspected mechanism receipts, **not an exhaustive proof** over every loader, cache writeback, remap, page-vector lifecycle and version mutation. No t5-specific directed test was added by `a294db4c` or `739b1c2a`. Generic differential test buses inherit the empty stable range, so their PASS is not evidence that every S3 flash invalidation path advances the epoch. Completing that proof and directed coverage remains unfinished; no implementation changes or new experiments were attempted during recovery.

### Diagnostic counter collision: do not misread 166 as failures

The added check in `3f68df78` increments **DSP[56]** when an epoch-admitted hot entry disagrees with a full page comparison. That index was **already used** by the slow-path loop-list histogram (`dsp(54 + r.loops.len().min(4), 1)`, `source xtensa-lx7/src/jit/wasm.rs:768`). Both `jp5.txt` and `jp5b.txt` have DSP[56] = **166**, and their complete 72-counter arrays are identical. Thus the comparison supplies **zero added mismatch counts relative to the unchanged control**, not a directly isolated zero-mismatch counter and not 166 demonstrated epoch failures. It checks entries that pass the new epoch/remainder-page guards only, not all possible invalidation paths. Counters 57/58 also overlap the old histogram and must not be treated as pure retained/omitted-page totals.

The old admission-profile page-count proxy DSP[7] falls **433,210,622→58,057,789** in `jp0` versus `jp5`. It is a conditional census quantity, not a count of every executed page comparison under the widened t2 guard. The stack gains 1,623 slow admissions/refills compared with t1+t2 (27,606 versus 25,983). Neither quantity establishes a timing win or closes the invalidation proof.

The unqueued t1+t2 intermediate is also preserved: `wasm/dispatch2-stack-t1t2.wasm`, revision `e1d1f395`, SHA-256 `15a2bd8b246be2cd18a53b6b361fbaf69581bb263d6883b004bbb2d6da5f18c3`; its exact receipt passes. It is supporting evidence, not a tenth queued job (`audit.json`, `extra`).

## Catalog extension and remaining handoff

Append the following wave-2 text to EX168's existing entry, retaining the wave-1 results. Do not create a competing EX ID or erase the earlier EX030/EX136 negatives.

| ID | Mechanism | Status | Evidence / outcome | Adoption / receipts |
|---|---|---|---|---|
| EX168 (wave-2 extension) | Negative no-region verdict per coverage epoch; complete active-loop list in Hot; skip epoch-current Hot refill; conditional Helpers decoration (EX030 retry); unqueued flash-only epoch | Nine t1–t4 candidates queued; receipt-audited for M3 sync. Timing unmeasured. t5 preserved, proof incomplete | On base 414c6e07 and wave-1 stack 580970f4: 372.32M run_inner calls/30 guest s. Stack slow lookups 87.42M→28.39M with t1→2.00M with t1+t2; 20.66M admits move into hot guard, making t3 mostly redundant in that stack. Eight-second Node full-stack profile: dispatch/generated 0.852→0.830, not timing evidence. All nine frozen artifacts match source sidecars/session build hashes and pass preserved strict instruction/console/frame-content gates plus 78,903 differential cases. t5 production singles/stack pass the same workload gate and suite; diagnostic uses a colliding counter and supports zero incremental mismatch counts only. | No adoption. Existing nine jobs against frozen plain base; t5 stays unqueued pending exhaustive flash-version proof and directed coverage. Source commits/hashes and limitations: `/Users/alice/src/a/esp32sim-x2/notes/dispatch2.md`, `runs/dispatch2/audit.json`, raw `runs/dispatch2/` and session S. Related EX136, EX165 and EX030; retain earlier unsuccessful retries. |

**Ready:** coordinator may sync the nine frozen t1–t4 candidates and audit receipts. **Unfinished:** actual M3 transfer/timing and adoption decision, optional coordinator queue-note clarification and t5's general invalidation proof/directed coverage. No benchmark outcome is inferred from Node wall times or sample shares.
