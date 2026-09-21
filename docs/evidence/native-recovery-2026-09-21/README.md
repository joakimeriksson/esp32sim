# Native regression recovery, September 21

The selected native changes recovered the earlier 14.22% Pocket Tank regression on the M3 Pro. Four fresh balanced main-versus-selected pairs measured medians of **46.84419→46.66852 seconds**, a 0.375% reduction. Treat this as **native parity**, not a meaningful speedup: the same-build control ranged from 46.77577 to 46.90399 seconds and one main confirmation arm took 47.50646 seconds. The four control observations do not establish a noise bound, confidence interval or formal equivalence margin. All eight confirmation arms retired the same 10,073,833,775 instructions and produced identical console output. [Confirmation](acceptance/confirm-main-recovery/summary.json) · [same-build control](acceptance/control-main-recovery/summary.json) · [earlier regression](../overnight-review-2026-09-20/README.md).

Browser preservation passed: reviewed source → selected source measured Pocket Tank **28.60176→28.40646 seconds** (0.683% less) and TinyDraw **28.96617→28.88666 seconds** (0.275% less). These small differences support preserved browser performance, not a new browser optimization claim. Each uses two balanced pairs with matching work, console hashes, verdicts and zero JIT failures. [Pocket preservation](acceptance/recovery-pocket-tank/summary.json) · [TinyDraw preservation](acceptance/recovery-tinydraw/summary.json).

## Direct before/after results

Before is main `d5446b4a`; after is selected `88dda106`, production-equivalent to published `b2a25703`. Values are medians of balanced AB/BA pairs. Browser gains compare the whole stack, so they are not attributed to one PR. These are bounded workload measurements, not a universal speed claim. [Audited results](acceptance-audit.json).

| Runtime and workload | Before seconds | After seconds | Less elapsed time | Throughput ratio | Pairs |
| --- | ---: | ---: | ---: | ---: | ---: |
| Native Pocket Tank | 46.84419 | 46.66852 | 0.375% | 1.004× | 4 |
| Browser Pocket Tank | 59.06415 | 28.45501 | 51.824% | 2.076× | 2 |
| Browser TinyDraw | 61.93931 | 28.89977 | 53.342% | 2.143× | 2 |
| Native panel SID control | 2.74603 | 2.64967 | 3.509% | 1.036× | 2 |

The native panel SID control reuses the exact hashed inputs and seven-guest-second workload from the prior EX027 qualification, with fresh main/selected binaries. All four arms retired core 0 **260,026,792** plus core 1 **136,442,769** instructions and emitted console SHA-256 `a77aaabb68350611617f518acdf4687b919906b7439ef327d816239c62b683d9`. Both balanced pairs favored the selected source in this short secondary control. [Panel receipts](acceptance/confirm-panel-recovery/summary.json) · [panel harness](screen-panel.mjs) · [prior manifest](../native-panel-qualification-2026-09-19/summary.json).

## Selected source and profiling

Measured source is `88dda106`, reconstructed from `9bc4d5a5` with [the preserved production patch](patches/recovery.patch). Published source [b2a25703](https://github.com/joakimeriksson/esp32sim/tree/sanitized-b2a25703b59fda73ec1715774c22a151d60a973a) has identical production Rust, Cargo configuration and build flags; the [remaining Rust diff](published-equivalence.patch) contains two test files. Full source identities are in [provenance](provenance.json). This is a source-equivalence claim, not a claim that the published checkout itself was timed.

Native-only changes force hot scheduler wrappers, block lookup and cached-data pricing to inline, then omit unused WASM fetch-ring fields from the native CPU structure. The latter shrinks native CPU state from 1,632 to 1,120 bytes. [Before layout](native-layout-before.txt) · [after layout](native-layout-after.txt) · [layout probe](layout_receipt.rs). No WASM attribute policy changed. These mechanisms follow existing EX027; the material difference from earlier campaigns is sampled native attribution against current whole-stack code, isolated candidate screens, fresh balanced confirmation and a same-build control. [Selected patch](patches/recovery.patch) · [catalog](../../experiments.md#ex027).

Serial macOS `sample` captures showed sample residency in outlined `step_blocks`, `observe_execution` and `find_block` functions in the regressed build, motivating inlining screens. The combined candidate removed those outlined boundaries. Sampling is diagnostic only: profiled elapsed times are excluded from performance results and sample residency alone does not establish removable call overhead. For example, forcing `Core::run` to inline made the isolated candidate slower. [Main profile](profiles/baseline.top.txt) · [regressed profile](profiles/final.top.txt) · [combined profile](profiles-combo/combo.top.txt) · [profile harness](profile.mjs). Compressed full samples and exact native output are retained beside the summaries.

WASM executable code is byte-identical through all three selected source steps: the 2,022,650-byte code section has SHA-256 `3eb5d3645bb6088f0d438cdb759f9b5c174c10c5887917c74953523ddbab8fdb` throughout. Other noncustom differences are classified source-line diagnostic fields, not executable code; the final native-state step has identical noncustom sections. Browser timings above independently check preservation. [Combined identity](wasm-identity/combo/identity.json) · [combined data classification](wasm-identity/combo/data-classification.json) · [pricing identity](wasm-identity/pricehelper/identity.json) · [pricing data classification](wasm-identity/pricehelper/data-classification.json) · [final state identity](wasm-identity/price-state/identity.json).

## Isolated screens

Each row uses two balanced AB/BA pairs on the same M3, with equal per-core retired work and console hashes. Negative results were retained. Screens against the combined candidate are incremental comparisons, not main comparisons; their percentages must not be added. [Audited calculations and binary identities](audit.json) · [audit program](audit.mjs) · [candidate source patches](patches/).

| Candidate | Matched baseline | Baseline → candidate median seconds | Less elapsed time | Decision |
| --- | --- | --- | --- | --- |
| Deadline-first cadence | Reviewed stack | 53.27886 → 53.32172 | −0.080% | Flat, omitted |
| Native scheduler wrapper inlining | Reviewed stack | 53.27778 → 51.05705 | 4.168% | Combined |
| Native block lookup inlining | Reviewed stack | 53.23353 → 50.51565 | 5.106% | Combined |
| Both inline changes | Reviewed stack | 53.22459 → 48.52153 | 8.836% | Incremental baseline |
| Unpriced dispatch specialization | Combined | 48.34390 → 48.98514 | −1.326% | Rejected |
| Core dispatch inlining | Combined | 48.31127 → 49.11580 | −1.665% | Rejected |
| Cached-data pricing inlining | Combined | 48.15752 → 47.31098 | 1.758% | Selected |
| Wide native TLB bounds check | Combined | 48.34706 → 48.50953 | −0.336% | Rejected |
| Omit native fetch-ring state | Combined | 48.32604 → 47.73595 | 1.221% | Selected |

The final selected candidate combines the positive native changes and is judged by its separate main comparison, not predicted from these screens. Unmeasured continuation/cold-tail candidates were not needed for the Pocket Tank acceptance gate.

## Measurement contract

All timings ran serially on the actual Apple M3 Pro (12 cores, 36 GB, macOS 27). Native executables use default Cargo release settings, rustc 1.98.1 and no Rust flags. The frozen reviewed executable is source `946497a3`, production-equivalent to the source baseline `9bc4d5a5`; [their source diff](baseline-production-equivalence.patch) records the distinction. Benchmark overrides beginning with `ESP32SIM_` or `ESP_EMU_` are removed. Native timing includes the process lifetime; browser timing covers the firmware execution loop, so cross-runtime seconds are not directly comparable. [Native harness](screen-native.mjs) · [original host/toolchain receipts](../overnight-review-2026-09-20/README.md).

Pocket Tank runs 30 guest seconds from the fixed ROM/bootloader/partition/application/model files recorded with SHA-256 hashes in each summary. Every native arm must exercise JIT, retire core 0 **4,446,180,089** plus core 1 **5,627,653,686** instructions and emit console SHA-256 `9e8a66e483f741c80db1bd20c62bc43943697c2e6a0053a6af1f7d52af8c67dd`. JIT compilation counts and generated code sizes may legitimately differ across revisions. No independent framebuffer or audio equivalence is claimed. Private account/host/path labels in receipts are normalized to generic labels; recorded source, artifact and input hashes are preserved.

Browser runs use Chrome 153.0.8010.53 / V8 15.3.76.13, common selected JavaScript glue and identical workload/harness input hashes except the WASM artifact. Main WASM uses default release flags; reviewed and selected WASM use `-Cllvm-args=-inline-threshold=4000`. No arm uses wasm-opt. Supplied artifacts retain their original source/build identity in `artifactBuild` within each `build.json`; the surrounding prepared-glue commit is not the artifact's source revision. Pocket Tank has the native count above; TinyDraw retires **9,819,885,134** instructions. Matching console hashes, successful workload verdicts and zero JIT failures are required. [Preservation commands](confirm-recovery.sh) · [direct main commands](final-main-browser.sh) · [acceptance audit](acceptance-audit.mjs).
