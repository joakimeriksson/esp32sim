# September 20 combination and M3 confirmation

Status: combined production build and correctness gates passed on the M3; browser measurements running.

The combination starts at cf1187a6 and includes kernel-s1, coverage-s1,
onecall-s1r and deadlines-a. Its production source commit is 93741045.
The original integration commits are retained separately on
codex/perf-combined-0920. Nothing is pushed or merged into main.

## Reproduction inputs

- Frozen candidate identities, queue metadata and firmware SHA-256 values:
  /Users/alice/src/a/esp32sim-exp/m3-transfer/manifest.json
- Original single-pair runs:
  /Users/alice/src/a/esp32sim-exp/runs/overnight/
- Original batch report:
  /Users/alice/src/a/esp32sim-exp/results/overnight.md
- Standalone M3 campaign script:
  /Users/alice/src/a/esp32sim-exp/m3-transfer/campaign.sh
- M3 workspace: alice@alice-m3p.local:~/bench/esp32sim

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

Timing remains pending. The original single-pair results are recorded in EX153, EX155, EX156
and EX157, including the negative checked-region-copy variant. The combined
artifact is experimental until its correctness and timing results are known.
