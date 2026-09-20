# September 20 combination and M3 confirmation

Status: M3 campaign completed September 20 at 12:41:33 BST. All correctness and timing checks passed; combination retained on the local integration branch.

The combination starts at cf1187a6 and includes kernel-s1, coverage-s1,
onecall-s1r and deadlines-a. Its production source commit is 93741045.
The original integration commits are retained separately on
codex/perf-combined-0920. The runtime changes are now proposed in PRs #110–113 and the build setting in PR #114, all ready for review. They are not merged into main.

## Reproduction inputs

- Frozen candidate identities, queue metadata and firmware SHA-256 values:
  /Users/alice/src/a/esp32sim-exp/m3-transfer/manifest.json
- Original single-pair runs:
  /Users/alice/src/a/esp32sim-exp/runs/overnight/
- Original batch report:
  /Users/alice/src/a/esp32sim-exp/results/overnight.md
- Standalone M3 campaign script:
  /Users/alice/src/a/esp32sim-exp/m3-transfer/campaign.sh
- M3 workspace

kernel-s1 uses commits a473c33c, ac47f03f and 94f2cef4. coverage-s1
uses 10b01db5. onecall-s1r uses ea42f20b, 74a3000e and 21dc368b.
deadlines-a was built from 62da929a plus the preserved
wasm/deadlines-a.dirty.patch; its resulting production diff equals
cf1187a6..fb506140. The integration applies that production diff directly.

## Measurement contract

Transfer the already measured immutable artifacts for individual confirmation.
Build the combination with Rust 1.98.1, release, no extra features and no
wasm-opt. Check native Xtensa/S3 tests, wasm JIT differential tests and the
30-second Node exactness gate before timing. The combined exactness gate
requires the cached baseline instruction total, console SHA-256, frame count,
zero panics and zero JIT failures.

Run baseline and candidate on the same host. Keep M3 results separate from the
original laptop; Chrome versions initially differ (151 on M3, 153 locally).
The first M3 timed arm reports Chrome 153.0.8010.53 / V8 15.3.76.13 after Chrome updated, matching the original laptop. Record the browser/V8 version and platform in the harness receipts. Do not
combine percentages across hosts or add individual speedups.

The M3 script serializes a two-pair A/A control, three alternating pairs per
individual candidate, four alternating pairs for the combination and three
TinyDraw pairs for the combination. All compilation and correctness runs
finish before timing starts. Three pairs alternate order but are not evenly
balanced; the four-pair combined run is balanced.

## Outcome

Build passed with Rust 1.98.1. Native tests: 117 S3 tests and 20 Xtensa tests passed. WASM differential suite: 78,903 cases passed, 81,884 compiled modules released. The 30-second Node gate matched 10,073,833,775 instructions, console SHA-256 b9d9966e5d9d73984203c11846eb7f8c86507cb53ffe133c4e7e92dff66319d7 and 3,094 frames, with zero panics and JIT failures. Node elapsed time is not a browser benchmark. Local copies of logs: /Users/alice/src/a/esp32sim-exp/results/m3/logs/.

Final M3 timings are below. The original single-pair results are recorded in EX153, EX155, EX156
and EX157, including the negative checked-region-copy variant. The combined artifact passed the planned correctness and timing gates; the source is proposed in PRs #110–114 and is not merged into main.

## Final M3 results

| Candidate | Median baseline → candidate (s) | Reduction | Per-pair reductions |
|---|---:|---:|---|
| [control-aa](runs/control-aa.json) | 57.714 → 57.199 | 0.89% | 1.27%, 0.51% |
| [kernel-s1](runs/kernel-s1.json) | 57.119 → 52.069 | 8.84% | 7.85%, 8.66%, 9.33% |
| [coverage-s1](runs/coverage-s1.json) | 56.938 → 51.542 | 9.48% | 9.16%, 10.01%, 9.79% |
| [onecall-s1r](runs/onecall-s1r.json) | 57.096 → 50.037 | 12.36% | 13.68%, 12.36%, 12.32% |
| [deadlines-a](runs/deadlines-a.json) | 57.216 → 51.775 | 9.51% | 9.22%, 9.04%, 9.89% |
| [combined](runs/combined.json) | 57.161 → 35.359 | 38.14% | 37.94%, 37.53%, 38.34%, 38.33% |
| [combined-tinydraw](runs/combined-tinydraw.json) | 37.943 → 33.311 | 12.21% | 12.44%, 12.96%, 11.97% |

All individual confirmations use three alternating pairs. The combined pocket-tank result uses four balanced-order pairs and TinyDraw uses three alternating pairs. Every arm passed pinned instructions and console-output checks. M3 A/A per-pair differences were 1.27% and 0.51%; these two pairs describe observed control variation, not a statistical bound. The combined pocket-tank reduction corresponds to about 1.62× baseline throughput. Do not pool these measurements with the original laptop sweep.

## Final inline selection

All measurements below are on the M3. Thresholds 2000 and 4000 compare against combined.wasm; 8000 compares against combined-inl4000.wasm. These are incremental reductions and must not be added to the earlier gains.

| Run | Median baseline → candidate (s) | Reduction | Per-pair reductions |
|---|---:|---:|---|
| [inline-control-aa](runs/inline-control-aa.json) | 35.312 → 35.579 | -0.76% | -0.72%, -0.79% |
| [combined-inl2000](runs/combined-inl2000.json) | 35.244 → 33.330 | 5.43% | 5.25%, 5.47%, 5.76%, 5.33% |
| [combined-inl2000-tinydraw](runs/combined-inl2000-tinydraw.json) | 33.230 → 30.990 | 6.74% | 6.14%, 6.74%, 7.08% |
| [combined-inl4000](runs/combined-inl4000.json) | 35.381 → 33.281 | 5.94% | 6.33%, 6.23%, 5.83%, 6.04% |
| [combined-inl4000-tinydraw](runs/combined-inl4000-tinydraw.json) | 33.215 → 30.991 | 6.70% | 6.91%, 6.68%, 6.70% |
| [high-inline-control-aa](runs/high-inline-control-aa.json) | 33.344 → 33.240 | 0.31% | 0.02%, 0.60% |
| [combined-inl8000](runs/combined-inl8000.json) | 33.250 → 34.382 | -3.41% | -2.85%, -3.23%, -3.95%, -3.37% |

Select combined-inl4000.wasm. Its pocket-tank and TinyDraw improvements are consistent across pairs and exceed the observed A/A differences. Threshold 8000 regressed in every pair; its TinyDraw timing was skipped by the positive-candidate gate. Threshold 16000 was canceled at Alice’s request. All tested thresholds passed differential and exactness gates before timing. Control differences describe observed variation, not a statistical bound.

## Browser hand-test setup

After benchmarks stopped, copied the selected combined-inl4000.wasm into a separate handtest/web tree on the M3. Frozen benchmark artifacts remain unchanged. A loopback HTTP server serves that tree on port 8810. Open on the M3: http://127.0.0.1:8810/run.html?wasm&fw=pocket-tank or http://127.0.0.1:8810/run.html?wasm&fw=tinydraw-latest. Use one emulator at a time.

Sequential headless Chrome checks confirmed rendered frames and no JavaScript errors: [Pocket Tank](handtest/pocket-tank.json), [TinyDraw](handtest/tinydraw-latest.json). TinyDraw also emitted TINYDRAW_VECTOR_V2_READY. This is boot verification, not a manual interaction test. TinyDraw uses the latest local 2.3.0 release demo with matching bootloader and partition table, not the older benchmark battery; [firmware provenance](handtest/provenance.json).

## User browser observations

Alice reported these approximate live UI readings on the M3 Pro after hand-testing the selected combined + inline-4000 artifact on September 20: Pocket Tank about 60% realtime in Safari and 77% in Chrome. TinyDraw appeared realtime in Safari with more testing needed; in Chrome it appeared realtime with observed dips to 94% at drawing start and 71% at another point. These are user-reported interactive observations, not controlled benchmark samples. Browser versions, observation durations and concurrent system activity were not captured for this manual check.
