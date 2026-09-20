# Turnover s1s2 on combined + inline 4000

Rejected for adoption; keep the validated combined + inline 4000 artifact. This follow-up shows small positive reductions, but they do not establish an improvement beyond the observed identical-build control variation.

All measurements below are incremental against combined-inl4000 on the M3 Pro, not against the original sweep baseline. Positive percentages mean less wall time.

| Workload | Pairs | Median wall-time reduction | Per-pair reductions |
|---|---:|---:|---|
| [Identical-build Pocket Tank control](turnover-s1s2-control-aa.json) | 2 | 2.15% | 2.31%, 1.99% |
| [Pocket Tank](combined-inl4000-turnover-s1s2.json) | 4 | 1.40% | 1.25%, 1.21%, 1.21%, 2.25% |
| [TinyDraw benchmark battery](combined-inl4000-turnover-s1s2-tinydraw.json) | 3 | 1.87% | 1.63%, 1.87%, 2.30% |

Every candidate pair improved and passed its work/output checks. However, identical-build controls also appeared faster by about 2%. This is an inconclusive incremental result, not proof of zero benefit. The control workload was Pocket Tank; no TinyDraw-specific A/A control was measured. TinyDraw here is the existing benchmark battery, not the latest firmware used for manual browser testing.

Candidate source: local commit `e03a38ee`, adding the s2 interrupt fast-path patch to the prior s1 confirmation tree. The remote Git commit is a source snapshot for benchmark provenance. [Compiler flags](combined-inl4000-turnover-s1s2.rustflags) and [artifact SHA-256](combined-inl4000-turnover-s1s2.sha256) identify the measured build. [Native tests](combined-inl4000-turnover-s1s2-native.log), [78,903 WASM differential cases](combined-inl4000-turnover-s1s2-jit-tests.log) and the [30-guest-second exactness gate](combined-inl4000-turnover-s1s2-exact.log) passed.

The [campaign log](campaign.log) records completion at 14:07:34 UTC. Two setup attempts failed before candidate timing because the copied source lacked usable Git metadata. Timing resumed with the same compiled artifact after metadata repair; the completed control pairs were retained. Local account path labels in these archived receipts are normalized to Alice. Existing benchmark and browser artifacts were not replaced.
