# September 20 combination and M3 confirmation

Status: M3 campaign completed September 20 at 12:41:33 BST. All correctness and timing checks passed; combination retained on the local integration branch.

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

Final M3 timings are below. The original single-pair results are recorded in EX153, EX155, EX156
and EX157, including the negative checked-region-copy variant. The combined artifact passed the planned correctness and timing gates; it has not been pushed or merged into main.

## Deferred compiler-flag follow-up

Alice flagged inline thresholds 2000 and 4000 for later confirmation on September 20. Original-laptop first-pass screens: flags-inl2000 74.117685→70.342445 s (5.09% reduction), flags-inl4000 72.899460→68.292615 s (6.32%), flags-inl250 72.033045→71.774190 s (0.36%). Each preserves the pinned instruction total and console hash. These are single pairs, with an earlier same-machine A/A apparent improvement of 4.63%; neither larger threshold is confirmed.

Receipts: /Users/alice/src/a/esp32sim-exp/runs/overnight/flags-inl2000-5ce976e6-pocket-tank--p1/summary.json and flags-inl4000-8c091acf-pocket-tank--p1/summary.json in the same directory. Proposed follow-up: compare the combined production build with and without each flag, using a same-host baseline and repeated alternating pairs. Do not add the standalone percentages to the combined result. This follow-up is deferred; do not alter the running M3 campaign.

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

## Inline follow-up launched

The M3 started inline-campaign.sh at 12:53:03 BST. EX154 compares thresholds 2000 and 4000 against combined.wasm, not the original base. Both use production source 93741045 and Rust 1.98.1 with only -Cllvm-args=-inline-threshold=N added. All builds and per-variant correctness gates precede serial timing. Local launcher: /Users/alice/src/a/esp32sim-exp/m3-transfer/inline-campaign.sh. Local refreshed results: /Users/alice/src/a/esp32sim-exp/results/m3/README.md. Browser hand-testing follows selection of the final artifact.

## Higher inline thresholds

Alice requested 8000 and 16000 after 2000 and 4000 showed gains. The high-inline-campaign.sh waits for the previous campaign to finish and takes the same exclusive lock. Its baseline is combined-inl4000.wasm, so percentages measure further improvement over 4000. Both candidates must pass differential and full exactness gates before timing; then two A/A pairs, four balanced pocket-tank pairs per threshold and three TinyDraw pairs for consistently positive candidates. No concurrent browser hand-tests. This continues EX154, not a new experiment.

Scope correction: Alice limited this follow-up to 8000 versus 4000. The 16000 candidate is canceled. If 8000 adds no clear gain, retain the validated winner and proceed to browser hand-testing; do not escalate thresholds.
