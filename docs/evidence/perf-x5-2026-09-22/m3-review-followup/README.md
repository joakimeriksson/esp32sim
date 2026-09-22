# Corrected runtime: fresh M3 Pro comparison

The corrected runtime measured **16.36% less Pocket Tank wall time and 14.49% less TinyDraw wall time** than corrected pre-x4 source on the M3 Pro. All four alternating pairs improved for each workload. This measures two optimization rounds, called x4 and x5, plus their review fixes: fewer dispatch checks, small-block execution inside compiled chains, faster memory probes and stores, coalesced vector loads, retained accumulator values, batched housekeeping, direct region branches and compiled processor-state/windowed-return operations. The baseline predates these changes. Both builds use matched production settings. It does not attribute a gain to an individual PR. [All results](results.json)

| Workload | Baseline median | Corrected median | Wall reduction | Per-pair reductions |
| --- | ---: | ---: | ---: | --- |
| Pocket Tank | 28.69465 s | 24.00143 s | 16.36% | 16.45%, 16.00%, 17.00%, 16.83% |
| TinyDraw battery | 29.20511 s | 24.97447 s | 14.49% | 14.73%, 14.58%, 14.27%, 14.12% |

Sources: [Pocket samples](before-after-pocket.json) and [TinyDraw samples](before-after-tinydraw.json). Reduction is `100 × (1 − median(candidate wall) / median(baseline wall))`, not the median of the pair percentages. All samples are retained.

The baseline is `7828e683bec41b9fc47dbbc38391a3a794fc4956`, artifact `37c13458`: pre-x4 source including the preserved memory-correctness patch beyond main `0042d053`. The measured candidate is `7c54ce3400e8ee92b5dccfb1f423223c4020e0ea`, artifact `3bc68c66`. Both were rebuilt with Rust 1.98.1, LLVM inline threshold 2000, release debug=0 and strip=debuginfo, without wasm-opt or diagnostic features. Both arms use the same JavaScript runtime from the measured candidate. [Full identities and input hashes](provenance.json), [baseline reconstruction](../m1-before-after/baseline-from-main.patch)

The later published-source artifact `fbb189ac`, built from `fdefbdfb`, was not timed in this campaign. Its 1,362 function bodies and every section except data match measured `3bc68c66`; all 28 changed bytes are Rust source-line records from a test-only accessor. Layout, filename pointers, lengths and columns match. Carrying these timings forward relies on that comparison; error-path diagnostic line numbers differ. [Code comparison](final-code-comparison.json), [data classification](final-data-comparison.json)

Three same-build control pairs per workload ran before the comparisons. Their apparent reductions were **−0.553% Pocket** and **+0.069% TinyDraw**. These small controls describe observed variation; they do not establish a noise floor or statistical confidence. The complete campaign retains 28 arms, using fresh headless Chrome processes and baseline/candidate then candidate/baseline order. [Pocket control](control-pocket.json), [TinyDraw control](control-tinydraw.json), [retained harness](harness/run-pairs.py)

Every browser arm passed its completion verdict, pinned instruction total, frame count and console-equality checks with zero JIT failures. Pocket retired 10,073,833,775 instructions with 737 frames; TinyDraw retired 9,819,885,134 with 428 frames. A separate 30-second Node Pocket check also matched full frame-content hashes and console output against the baseline. Browser captures do not hash frame contents, so TinyDraw frame-content equality remains untested. Node elapsed time is not performance evidence. [Browser verifier result](verification.json), [Pocket exactness](pocket-exact30.json)

The host reported Apple M3 Pro, macOS 27.0 arm64 and Chrome 153.0.8010.53. Start/end snapshots showed AC power, 100% battery and low-power mode disabled. Builds ran on another machine before timing. Aggregate load is retained per arm; no unrelated process inventory was collected. These are small, nonrandomized samples on one machine and two workloads, without confidence intervals or a guarantee of otherwise idle conditions. [Environment and provenance](provenance.json), [individual arms](before-after-pocket.json)

This is a fresh aggregate comparison under existing EX094, materially differing from the historical x5-only screen by corrected production source, a pre-x4 baseline and matched build policy. Earlier measurements remain historical. [Experiment catalog](../../../experiments.md#ex094), [historical x5 report](../README.md)

To verify the committed numeric receipts from the repository root:

```sh
node docs/evidence/perf-x5-2026-09-22/m3-review-followup/verify.mjs
```

To repeat a workload, check out the measured source revisions, build each with the settings above and preserve each artifact with its `build.json`. Supply an asset manifest matching the retained hashes, use the measured candidate checkout for both runtime trees and run the checked-in paired harness with four pairs. Use the baseline artifact in both positions with three pairs for the same-build control. Create a virtual environment using `uv venv .venv` if absent:

```sh
uv run --offline --no-project --python .venv/bin/python python "$CANDIDATE_TREE/tools/browser-benchmark/run-pairs.py" "$OUTPUT" \
  --baseline-tree "$CANDIDATE_TREE" --candidate-tree "$CANDIDATE_TREE" \
  --baseline-wasm "$BASELINE_WASM" --candidate-wasm "$CANDIDATE_WASM" \
  --assets "$ASSETS_JSON" --pairs 4
```

The retained campaign harness replaces Git inspection with frozen runtime-snapshot metadata and preserves browser profiles in task scratch; timing and validation logic are unchanged. To replay that exact harness, copy the measured checkout's `tools/browser-benchmark` directory, replace its `run-pairs.py` with the retained copy and place a `source-revision.txt` containing the common runtime revision beside the runtime tree's `Cargo.toml`. The provenance records both script hashes. Public receipts omit event streams, raw logs, private paths and browser-session details while retaining their original file hashes. Raw captures remain in task storage, not a public archive. [Curation manifest and limits](source-manifest.json), [harness provenance](provenance.json)
