# TinyDraw: early checkpoints versus historical x5 source

**Measurement scope:** the M1 source `0ccb8337` and the M3 artifacts compared here predate the x4 correctness review fixes beginning at `6a13ddf0` and recorded through `5cc58493`. Those fixes change generated WASM. These historical results do not measure the corrected source, and no new corrected-source speed claim is made. [x4 corrections](../../perf-x4-2026-09-22/review-fixes.md), [artifact limitation](../../perf-x4-2026-09-22/review-validation.json).

Historical x5 source `0ccb8337` completed the same TinyDraw firmware task in **75.53% less elapsed time (4.09× task throughput)** than the earliest tested checkpoint that passed the firmware gates and matched the x5 artifact’s final image. Against the first shipped browser JIT, historical x5 source used **70.44% less time (3.38× task throughput)**. These are observations on an in-use M1 Pro with reported concurrent compilation, not isolated JIT effects. [Samples](results.json), [conditions](conditions.json)

| Historical source | Historical median | x5 median | Less elapsed time | Task throughput ratio |
| --- | ---: | ---: | ---: | ---: |
| September 3 board fix, `a2db66cf` | 150.22557 s | 36.76385 s | 75.53% | 4.09× |
| September 5 first shipped JIT, `80c43cae` (#38) | 118.73402 s | 35.09455 s | 70.44% | 3.38× |

The rows are separate paired batches, so their x5 medians differ; neither is a universal runtime. [Earliest batch](earliest-vs-current.json), [first-JIT batch](first-jit-vs-current.json).

Each historical comparison retains one warm-up pair and four alternating primary pairs. Warm-ups are excluded from the medians. The measured x5 source is `0ccb8337`; all sources use the recorded compiler and common inline2000/debug0/stripdebuginfo policy with no wasm-opt. These rebuilds do not reproduce the original release binaries. [Artifact and compiler identities](provenance.json)

“Same task” means identical TinyDraw firmware/assets and successful firmware gates. It does not mean equal retired instructions: both historical checkpoints execute 9,820,325,756 instructions and publish 101 binary frames; historical x5 source executes 9,819,885,134 and publishes 428. Each timed arm is checked against its own prequalified instruction/frame totals and console hash. [Historical and x5 contracts](earliest-vs-current.json)

Separate Node qualification pilots for the three accepted checkpoints reached the same 368×448 final published LCD image: decoded RGBA SHA-256 `dbf0d387a05232f38dc694f81ea871e448e2dc112c90d0fd94d9ecaef1fa41d0`, with 16 colors. The September 3 first board version `31037b26` timed out without frames. The immediate parent `e7a16784` completed the firmware gates but produced a different final image. This establishes the earliest qualified checkpoint among those tested, not an exhaustive proof about every earlier commit. Browser timing checks firmware output, work and frame counts; it does not independently establish pixel equality or interactive display performance. [All accepted and rejected qualifications](qualification.json)

The two-pair identical-x5-artifact control showed an apparent **1.84% reduction**, with pair reductions of 0.11% and 3.49%. That small control is not a noise floor or a correction factor. Every completed primary sample is retained. The user reported compilation during the earliest warm-up; background work remained uncontrolled even when spot counts found no matching compiler processes. AC power was observed, but load and power snapshots do not establish uniform thermal or interference conditions. [Control](control-current.json), [condition samples](conditions.json)

The original browser JIT and current JIT must compile blocks with zero failures. Pre-JIT builds explicitly report interpreter mode and unavailable JIT instruction counters as null. All arms use fresh headless Chrome processes and the same current JavaScript runtime; the historical JIT host was checked byte-identical. [Harness identities](harness-provenance.json), [provenance](provenance.json)

These results extend aggregate-comparison experiment [EX094](../../../experiments.md#ex094), with the original shipped JIT recorded in [EX019](../../../experiments.md#ex019). [Reproduction commands](REPRODUCE.md) and [portable verifier](verify.mjs) accompany the receipts. Retained compact receipts preserve sample values, original hashes, qualification failures and aggregate conditions while omitting private paths, browser profiles and raw logs. [Original-file hashes](originals.json)
