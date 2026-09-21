# Combined + inline 4000: local exploratory profiles

Investigate compiled-code admission and dispatch next. Both normal and diagnostic builds put substantial sampled time in the JIT wrapper, while direct interpreter execution is a smaller component. This identifies a place to investigate, not an achievable speedup.

These captures may overlap Lightroom RAW processing, as reported by the user. Treat timings, latency and sample proportions as preliminary. Quiet M3 confirmation is required before choosing or promoting a performance change. No runtime change was made from these results.

## Captures and checks

Source is `74b87d24`, the combined runtime plus inline threshold 4000, without turnover. Rust 1.98.1 and Chrome 153.0.8010.53 were used. [Artifact hashes](artifacts.sha256) distinguish ordinary release and the separate `cpu-profile` build. The latter changes inlining and names generated functions; the battery's `instrumented:false` field does not detect this feature and must not be treated as proof of a normal timing build.

Pocket Tank ran 30 guest seconds. Both captures passed, with 10,073,833,775 instructions, 3,094 frames and no JIT failures: [release](release-pocket-tank/result.json), [diagnostic](cpu-pocket-tank/result.json). The compiled-instruction counter was 10,014,435,040 in each, approximately 99.4% of total instructions. Instruction coverage is not CPU-time coverage.

TinyDraw used the latest local 2.3.0 demo and matching ELF, not the old battery. The [three-stroke release replay](release-tinydraw-latest/result.json) committed all three strokes. The denser [release](release-tinydraw-latest-dense/result.json) and [diagnostic](cpu-tinydraw-latest/result.json) replays committed all 12 strokes. This is a moderate replay, not a worst-case large-document test. It repeats four stroke positions; later strokes can produce no newly changed pixels along the existing line. Do not interpret missing movement-pixel detections as lost input. [Replay source](dense-response.mjs).

## Sample attribution

Percentages below exclude samples labeled idle and weight exclusive samples by their time deltas. Inlined work remains attributed to its enclosing function. These are worker profiles; they do not measure all main-thread canvas or browser rendering costs. [Analysis and function details](analysis.json), [analysis script](analyze.mjs).

| Sample category | Pocket Tank release | Pocket Tank diagnostic | TinyDraw 12-stroke release | TinyDraw 12-stroke diagnostic |
|---|---:|---:|---:|---:|
| Generated blocks | 44.9% | 43.6% | 33.6% | 29.3% |
| JIT admission/wrapper (`run_inner`) | 19.5% | 19.7% | 14.8% | 15.2% |
| Interpreter instruction execution | 1.7% | 1.9% | 5.0% | 4.9% |
| Decoded-block lookup | 2.6% | 2.4% | 3.0% | 3.2% |

The stable prominence of `run_inner` makes its region guards, entry selection and return handling a better first investigation than blanket missing-opcode expansion. Sampling does not isolate which of those operations is expensive. Diagnostic inlining also moves time between the machine wrapper, block dispatcher and outer JIT wrapper, so their individual release/diagnostic percentages are not directly comparable.

## Next investigations

1. Count normal/resumed entries and rejection reasons on this combined baseline, then isolate the expensive path inside `run_inner`. Preserve page-version, interrupt, loop and budget semantics. Existing helper-decoration copy experiment EX030 already found only an unconfirmed 0.67% screen; do not relabel it as a new win.
2. Use that exit census to decide whether EX164's internal call/return edges can remove frequent wrapper transitions. The profiles alone do not establish calls as the dominant cause.
3. Keep EX161's missing-operation census targeted to hot interpreted blocks, especially TinyDraw. Pocket Tank's high compiled coverage makes a broad opcode expansion a lower priority unless the census identifies an expensive residual operation.

All original profile files are retained as gzip-compressed `.cpuprofile` files alongside browser results and event streams. [Capture conditions](conditions.txt) record the workload and interference limits. Local path labels in archived receipts are normalized to Alice. The selected browser artifact remains ordinary combined + 4000.
