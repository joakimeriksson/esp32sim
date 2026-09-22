# EX120 benchmark policy correction — September 22

Source base: `0ccb8337`. This follows the [#138 review](https://github.com/joakimeriksson/esp32sim/pull/138#issuecomment-5778013950).
The earlier `size-policy-review.json` receipt remains unchanged as historical evidence:
its last limitation describes the harness before this correction.

`tools/browser-benchmark/run-pairs.py` now removes ambient Rust flags and release
profile overrides, then sources the harness checkout's `tools/wasm-rustflags.sh`
for both arms. It records that file's SHA-256, resolved Rust flags and release
profile overrides in each build receipt. Defaults match production: inline
threshold 2000, debug=0 and strip=debuginfo. Explicit `--*-rustflags` values,
including an empty value, remain available for compiler experiments. Supplied
artifacts are unchanged and require their own provenance review.

This is a build-policy correction, not a rerun of historical timing experiments.
Existing measurements retain their original artifact identities. A/B comparisons
using the corrected harness require fresh artifact hashes and fresh timings.

The older gzip-9 comparison of 2,846,126 versus 581,068 bytes is from rebuilt
12,219,979-byte debug=1 and 2,088,105-byte default artifacts in
`size-policy-review.json`. The historical raw debug artifact in
`size-validation.json` was 12,219,972 bytes. Compression does not measure startup
latency and these artifacts are not claimed to have identical executable bytes.

Validation: `uv run --python <venv>/bin/python -m unittest discover -s tools/browser-benchmark -p 'test_*.py'`
passed all 15 tests. Added checks exercise the real shared shell policy with hostile
ambient profiling settings and both empty and nonempty explicit Rust flag overrides.
The README gives policy-aware hand-build and CPU profiling commands, including
intentional DWARF overrides. Local build paths and logs are not committed.

A full release build using the harness's resolved environment passed
`node tools/check-wasm-sections.mjs <artifact>`: DWARF absent, name section retained,
2088105 bytes, SHA-256 `eb754176f3100563da93b5269a54af1b93ff611abcf09fbae5026fd110b8409a`, gzip-9 581068 bytes.
Build command: `cargo build --release --target wasm32-unknown-unknown -p esp32sim-wasm -j 4`,
Rust 1.98.1. The environment was resolved by `production_environment` without
an explicit flags override. No browser timing was performed.
