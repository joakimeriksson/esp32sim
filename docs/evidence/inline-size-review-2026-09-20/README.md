# EX154: compiler inline threshold download sizes

Threshold 2000 produces a 7.49% smaller gzip download than 4000 on this runtime
revision. The existing desktop timing evidence does not establish a clear speed
difference between those two thresholds. This measurement adds the missing size
comparison. At the time of this measurement, the selected threshold remained 4000.

| LLVM inline threshold | Raw bytes | gzip -9 bytes | Gzip increase over default |
| --- | ---: | ---: | ---: |
| Compiler default | 9,206,986 | 2,171,846 | — |
| 2000 | 11,829,749 | 2,750,566 | 26.65% |
| 4000 | 12,726,683 | 2,973,365 | 36.90% |

[Machine-readable sizes and hashes](sizes.json) record three fresh release builds
of runtime revision `661623ea`, Rust 1.98.1, default package features and no
Binaryen postprocessing. These artifacts include later runtime changes than the
original EX154 experiment, so their byte counts are separate from the sizes in
the [PR review](https://github.com/joakimeriksson/esp32sim/pull/114#issuecomment-5752898469).
The build/test scripts share their threshold policy at `c243b6ba`.

Reproduce each build from that revision with:

```sh
RUSTFLAGS='' tools/wasm-build.sh
# Save web/wasm/esp32sim.wasm before the next build.
RUSTFLAGS='-Cllvm-args=-inline-threshold=2000' tools/wasm-build.sh
RUSTFLAGS='-Cllvm-args=-inline-threshold=4000' tools/wasm-build.sh
# For each saved artifact:
wc -c ARTIFACT.wasm
gzip -9 -n -c ARTIFACT.wasm | wc -c
```

The `-n` gzip option excludes the filename and timestamp so sizes are comparable.
No performance timings were collected in this follow-up. The iPhone default-versus-4000
comparison requested in the review remains unmeasured; these build sizes cannot
resolve the EX166 JavaScriptCore performance question. The existing desktop timing
results remain in the [original EX154 evidence](../perf-combined-2026-09-20/README.md#final-inline-selection).

## September 21 selection

The maintainer selected **2000** as the shared production/test default. The table
above records a saving of **222,799 bytes (7.49%)** over 4000 for the measured
`661623ea` artifacts. The existing desktop timings do not establish a decisive
speed advantage for 4000. This is a size-versus-speed decision from the existing
EX154 evidence, not a new benchmark or a claim that current artifact sizes are
identical to those historical builds.

The default lives in [the shared compiler policy](../../../tools/wasm-rustflags.sh).
Explicit `RUSTFLAGS`, including an empty value, still override it. Historical
4000 build hashes and speed measurements remain unchanged.
