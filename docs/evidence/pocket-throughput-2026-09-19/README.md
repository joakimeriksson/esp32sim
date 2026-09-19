# Pocket Tank throughput experiments, September 19

Two simple refinements failed their first resumed timing screen and remain parked: combining signed-byte (S8) dot reduction with ACCX access cleanup was flat, and caching the existing display interval was slower. The signed-byte reduction alone was inconclusive in two fresh alternating pairs, with one slower and one faster. All three candidates remain unadopted.

| Mechanism | Baseline → candidate wall seconds | Decision |
| --- | --- | --- |
| Signed-byte dot reduction only, pre-pause screen | 83.437980 → 81.207600 (2.67% lower) | Preliminary; separate from resumed measurements |
| Signed-byte dot reduction only, fresh pair 1 | 81.240895 → 81.542300 (0.37% higher) | Inconclusive; host activity observed |
| Signed-byte dot reduction only, fresh pair 2, reverse order | 84.887645 → 78.944525 (7.00% lower) | Inconclusive with sign-changing pairs |
| Signed-byte dot reduction plus ACCX access cleanup | 79.451900 → 79.369575 (0.10% lower) | Flat; parked after one pair |
| Cache existing display interval | 80.583460 → 82.276995 (2.10% higher) | Failed screen; parked after one pair |

The rows come from [S8-only preliminary](s8-only-screen.json), [fresh S8 pairs](s8-only-confirm.json), [combined arithmetic](s8-accx-screen.json) and [cadence](cadence-screen.json). Each JSON preserves the wall times, browser version, firmware and harness hashes, console hash and counted binary frames. These are screening results, not estimates of a repeatable regression or gain. No candidate has been adopted in the later correctness integration.

## Work and artifact controls

Every completed arm ran the same 30.000000229 modeled guest seconds and 10,073,833,775 instructions, produced 3,094 counted binary frames and matched console SHA-256 `9e8a66e483f741c80db1bd20c62bc43943697c2e6a0053a6af1f7d52af8c67dd`. All completed with zero JIT failures. The harness counts binary messages; this is not a screenshot equality test. See the per-arm fields in the linked JSON files.

[manifest.json](manifest.json) records source commits, exact WASM hashes and sizes, local artifact paths, the MacBookPro18,3 host with 32 GiB RAM and macOS 27.0 build 26A428, rustc 1.98.1 and wasm-opt 132 `-O3`. Chrome was 153.0.8010.53 with V8 15.3.76.13. Builds used ordinary release defaults with no diagnostic features or explicit Rust flags. [Build sidecars](build-provenance.json) preserve the original metadata. The initial S8-only artifact's original sidecar records the source patch; its reconstructed source commit is runtime-equivalent, not the original build commit.

The baseline is frozen `cfa669c4461e6c5b06f9740b2554686830c8d8de`, containing the eight open PRs at the initial snapshot. The later review/correctness integration has different source and requires separate validation. Team builds and simulator tests were stopped during these resumed measurements, but [a process sample](host-activity.txt) caught two external `xcodebuild` processes and Spotlight activity during fresh S8 pair 1. Small differences are inconclusive. Fresh baseline times span 79.45–84.89 seconds around the 83.44-second pre-pause screen, so the early positive pair must not be pooled into a repeatability claim.

## What was tried

**EX042 refinement: signed-byte reduction.** Sixteen signed-byte products sum within `[-260096, 262144]`, so generated SIMD partials can reduce in i32 before one signed i64 extension. The signed-16 path still widens before reducing. [Exact source patch](s8-only.patch). The combined variant additionally reads contiguous ACCX words with one i64 load, sign-extends its low 40 bits and writes one masked i64 value. [Combined patch and directed tests](s8-accx.patch). This is a refinement of the adopted PIE SIMD emitter, not a retry of bounds-check changes in EX064. The combined variant's smaller generated code did not produce a useful wall-time reduction in this screen. Fresh S8-only pair reductions were -0.37% and +7.00%; their pooled median reduction of 3.40% is not a repeatability claim. Do not adopt from these data. A retry needs a demonstrably quieter host and fresh alternating pairs, not another arithmetic refinement bundled into the same comparison.

The original S8-only build passed [78,833 differential cases](s8-jit-tests.txt). Both [S8-only](s8-directed-tests.txt) and [combined arithmetic](s8-accx-jit-tests.txt) passed 79,009 cases after adding 176 directed executions for lane contributions, saturation boundaries, old-Q load aliasing and arbitrary unused ACCX bits. This validates arithmetic behavior, not a performance claim.

**EX117/EX128 refinement: cache cadence evaluation.** The candidate refreshes the derived display push interval at execution entry and the external-quantum entry, then reuses it inside scheduling. It preserves the 50/120 Hz rates and phase; it does not reduce publication work. Refreshing only in the constructor would be incorrect because board selection can occur afterward. [Exact source and regression-test patch](cadence.patch). [Twenty-two existing machine tests and one focused test passed](cadence-native-tests.txt), including rate boundaries and board replacement across all three execution entry paths. The timing loss does not establish that caching inherently hurts; it is sufficient to reject this patch without further tuning. Retry only with new evidence of meaningful cadence-computation cost or a materially different representation.

## Historical anomaly

Older Pocket Tank records are not interchangeable with these measurements. The preserved [historical frame-work receipt](historical-frame-work.json) has 737 counted binary frames, versus 3,094 here with equal guest instructions: the binary-message count increased about 4.20×. PR89 changed the AMOLED quiet-push policy and rate. The changed message count is a concrete workload difference, but it does not establish how much of the elapsed-time difference came from publication.

The separate historical [48.5-second report](../../speed-plan.md) lacks the original raw host and browser-patch provenance in this evidence set. Do not label that historical timing gap a source regression. The local resume checkpoint preserves additional artifact-comparison leads for follow-up; they are not validated attribution evidence here.

## Reproduction

Use `tools/browser-benchmark/run-pairs.py` with `--baseline-tree`, `--candidate-tree`, `--baseline-wasm`, `--candidate-wasm`, `--assets` and an unused output directory. The manifest records the exact frozen artifacts and trees; the asset map is `/Users/sarah/src/a/esp32sim/web/wasm/fw/local/pocket-tank-assets.json`. All raw captures, console logs, build sidecars and timing logs remain under `/Users/sarah/src/a/esp32sim/work/pocket-perf-runs`. Run Python with `uv run --no-project --python /Users/sarah/src/a/esp32sim/.venv/bin/python` and finish builds before starting a CPU-quiet timing window.

These small checked-in receipts and source patches preserve negative results even when local binaries are unavailable. Rebuilding changes artifact provenance: record the new compiler, optimizer and hashes, then establish a fresh same-host baseline. Do not retry the parked combined arithmetic or cadence patch merely because the original binaries are unavailable.
