# Browser CPU and drawing-response benchmarks

Run these against a local TinyDraw battery firmware build. The battery enters its normal
interactive app after the automated gates. The drawing page waits for
`TINYDRAW_VECTOR_V2_READY`, uses the production WASM worker and pacing, and renders its
RGB565 output. No device is required.

Build the normal emulator with `tools/wasm-build.sh`. Create an asset map, using absolute
paths to your local files:

```json
{
  "wasm": "/absolute/path/esp32sim/web/wasm/esp32sim.wasm",
  "rom": "/absolute/path/esp32s3_rev0_rom.elf",
  "bootloader": "/absolute/path/bootloader.bin",
  "ptable": "/absolute/path/partition-table.bin",
  "app": "/absolute/path/tinydraw_esp32.bin",
  "elf": "/absolute/path/tinydraw_esp32.elf"
}
```

From the repository root:

```sh
python3 tools/browser-benchmark/serve.py /absolute/path/assets.json
```

Open `http://127.0.0.1:8792/`. Once ready, draw manually or replay the three fixed
strokes. Reload before repeating a benchmark so its document and cache history start
the same way. Use **Save receipt** to retain inputs, frame timings and firmware output.
Keep other simulator runs and builds stopped during the strokes.

The latency endpoint is the first changed pixel near pen-down submitted to the canvas.
It is a diagnostic proxy: unrelated changes in that region can produce false attribution,
and a correct stroke must also be checked visually. The replay holds down for 250 ms
before moving; pen-down, each of eight movement points, and lift-to-commit-report timing are recorded. Last canvas change after lift is also retained; it does not prove final authority equality. Three strokes are a
smoke baseline, not a latency distribution. Animation-frame callback timestamps do not
prove when the screen displayed the pixels. Firmware timing still uses the simulator's
instruction-count model.

## Build and time matched pairs

`run-pairs.py` builds both checkouts before starting Chrome, then alternates fresh
baseline and candidate captures. Give it the firmware asset map above (the `wasm`
entry is replaced by each build):

```sh
python3 tools/browser-benchmark/run-pairs.py target/browser-pairs \
  --baseline-tree /absolute/path/baseline-checkout \
  --candidate-tree /absolute/path/candidate-checkout \
  --assets /absolute/path/assets.json --pairs 3
```

It retains the binaries, source hashes and diff, build logs, browser versions, raw
console events, and a `summary.json` with individual pairs and median wall time.
Every run must pass the 36 firmware checks defined in `verdict-schema.json`,
report zero JIT failures, and match the expected instruction total; each capture's
`result.json` records success as `result.passed`. The current TinyDraw battery expects
9,819,885,134 instructions; changing
`--expected-instructions` requires a separately justified workload baseline. Inputs
must stay identical within each arm; firmware, harness, browser and console output
must match across arms. Stop other builds and simulator runs during timing.
Execution timing starts after firmware loading and boot initialization, and ends after
the completed automated verdict has been drained and validated. The summary reports
separate baseline/candidate median wall times, the reduction `100 × (1 − candidate
median / baseline median)`, and each matched pair's percentage reduction. Setup time
is recorded separately.

Use `--pairs 1` for screening only. Three pairs are a starting point, not a confidence
guarantee. `--baseline-wasm` and `--candidate-wasm` reuse existing artifacts; check
their build provenance before interpreting results. Explicit compiler experiments
can use `--candidate-rustflags='-Ctarget-feature=+simd128'`; ambient `RUSTFLAGS` are
otherwise cleared. Use `--chrome /path/to/chrome` if Chrome is not installed at the
standard macOS location. `--archive /path/to/extracted-review-bundle` can supply its
firmware assets instead of `--assets`. These timings exclude canvas rendering and
do not establish input latency or hardware clock accuracy.

## CPU sampling

For uninstrumented battery timing, use an ordinary release build and the headless
Chrome launch below, then run:

```sh
node tools/browser-benchmark/capture-battery.mjs http://127.0.0.1:8792/battery.html target/battery-timing
```

This headless-only capture saves the browser version, result and event stream without
starting the CPU profiler. Use a fresh output directory per run and alternate baseline
and candidate builds across repeated runs. Keep other simulator runs and builds stopped.

`/battery.html` runs the unpaced battery without rendering. For automated profiling,
launch a windowless Chrome process with a dedicated profile (macOS example):

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --no-first-run --disable-background-timer-throttling --disable-renderer-backgrounding --remote-debugging-address=127.0.0.1 --remote-debugging-port=9228 --user-data-dir="$PWD/target/benchmark-chrome" about:blank
```

Then run:

```sh
node tools/browser-benchmark/capture-cpu.mjs http://127.0.0.1:8792/battery.html target/cpu-profile
python3 tools/browser-benchmark/summarize-cpu.py target/cpu-profile/battery.cpuprofile
```

The capture starts the worker profiler before firmware execution and writes the raw
DevTools profile, browser version, battery result and console events. It uses the
Profiler domain without enabling the source debugger. Import the `.cpuprofile` into
Chrome's profiler for call-tree inspection.

For additional attribution, build `esp32sim-wasm` for `wasm32-unknown-unknown` in release
mode with `--features cpu-profile`, and point the asset map at that binary. This feature
marks the machine block wrapper, Xtensa block runner and JIT invocation function as
non-inline at Rust compilation. Browser optimization can still inline functions.
It adds no per-block clocks, but changes code layout and execution cost; do not use
its elapsed time as an optimization benchmark. Compare against an ordinary build's
profile and run uninstrumented performance comparisons separately.

The summary weights exclusive samples by `timeDeltas`. Inlined work stays attributed
to its enclosing function; the categories are not exact boundaries between hardware
emulation and compiler overhead. Generated WASM modules are grouped separately from
the main `esp32sim_wasm` module.

To capture the visible replay without a CPU profiler:

```sh
node tools/browser-benchmark/capture-response.mjs http://127.0.0.1:8792/response.html target/drawing-response
```

Both capture tools close only their own tab. They create background tabs in a visible
browser and selected tabs in headless Chrome, which has no OS window. They never
activate a visible Chrome window. The default debugging port is 9228; pass another port explicitly for an existing browser. Record actual input timestamps when comparing replays:
background timer throttling can change the requested input spacing.

For a drawing CPU profile, use a `cpu-profile` build (without `jit-profile`):

```sh
node tools/browser-benchmark/capture-cpu.mjs http://127.0.0.1:8792/response.html target/drawing-cpu 9228 drawing
```

This waits for app readiness and profiles only the three strokes and their settling
intervals. The feature also gives generated functions `xtensa_<block-head-PC>` names.
For battery coverage, combine `cpu-profile,jit-profile`. Export symbols from the exact
app and ROM with `xtensa-esp-elf-nm -n -C` into one text file, then join them:

```sh
python3 tools/browser-benchmark/summarize-jit.py target/cpu-profile/events.json target/cpu-profile/battery.cpuprofile symbols.txt target/cpu-profile
```

The join reports generated-block CPU samples and sampled instruction coverage separately.
A missing-opcode bundle's percentage estimates newly eligible guest work, not wall-time
savings. Resumed execution is attributed to the original decoder block head.

## Retaining results

Keep raw captures in the `target/` output directories used above. Commit compact
results and reproduction details according to the [evidence retention policy](../../docs/evidence/README.md).
Publish captures needed to substantiate a claim as fork release assets or in a separate
evidence repository, then include verified links and SHA-256 hashes in the summary.

## Compare workload intervals

Use `compare-runs.py` to compare repeated uninstrumented captures:

```sh
python3 tools/browser-benchmark/compare-runs.py \
  --baseline target/baseline-1 target/baseline-2 target/baseline-3 \
  --candidate target/candidate-1 target/candidate-2 target/candidate-3 \
  > target/comparison.json
```

The report includes every sample and the median change in total host wall time. It also
splits execution at firmware console milestones into boot/native kernels, cold rendering
and initial pan, pan sequences, cache tour, mixed drawing, hairlines, export, history and
settling. These intervals include setup and console delivery between milestones; they
are not isolated function timings or guest-device measurements.

The tool requires matching console hashes, instruction counts, final firmware verdicts,
Chrome versions and V8 versions. It also requires an explicit timing capture mode,
zero JIT failures, stable build hashes within each arm, and matching firmware and
harness hashes. Only the WASM may differ by default; declare other intended differences
with `--allow-change` followed by the exact provenance key. `--legacy` permits inspection
of older receipts without certifying their capture mode or build identity. `--screening`
permits fewer than three pairs. Verify ordinary, unprofiled build settings from the
retained build records too. Run comparisons serially with other builds and
simulator workloads stopped; three samples per build are a starting point, not a
statistical confidence guarantee.

## Select a candidate worker

When comparing changes to the worker itself, select its checkout separately from the
WASM and firmware paths in the asset map:

```sh
python3 tools/browser-benchmark/serve.py /absolute/path/assets.json \
  --web-root /absolute/path/candidate-checkout --port 8801
```

`--web-root` selects `web/wasm/worker.js` and its imported JavaScript modules. The asset
map still selects the WASM binary and firmware. Record both the worker checkout commit
and the asset hashes so each candidate uses its matching worker and binary.
