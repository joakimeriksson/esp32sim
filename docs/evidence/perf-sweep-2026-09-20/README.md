# September 20 experiment snapshot

**Superseded for final coverage by the [round closeout](closeout.md).** This earlier snapshot is retained for chronology; pending entries below now have first-pass results.

Captured 2026-09-20T13:06:35.150Z. This is a frozen snapshot of an ongoing two-pass campaign, not a completion report. 65 queued jobs; 56 have at least one completed Pocket Tank pair in this snapshot.

The original laptop sweep compares against cf1187a6. M3 results use separate baselines and are linked below. Positive percentages mean less wall time. Single pairs are preliminary screens; neither a positive screen nor an exactness pass establishes an improvement. Do not add standalone or cross-campaign percentages.

[Full source summary](overnight.json), [controls and original table](overnight.md), [queue metadata](jobs.jsonl), [30-second exactness outcomes](exact.md) and [short diagnostic checks](exact-6s.md). Queue revisions may describe a source checkout with uncommitted changes; preserved revision files and dirty patches under provenance/ supplement the frozen artifact identity. Raw run summaries include artifact hashes and output checks.

| Catalog family | Variant | Completed Pocket Tank pair reductions | Work/output checks | TinyDraw pairs completed |
|---|---|---|---|---:|
| [EX156](../../experiments.md#ex156) | kernel-s1 | [12.02%](runs/kernel-s1-f2717977-pocket-tank--p1.json) | Pass | 0 |
| [EX155](../../experiments.md#ex155) | coverage-s1 | [10.72%](runs/coverage-s1-be80f476-pocket-tank--p1.json) | Pass | 0 |
| [EX152](../../experiments.md#ex152) | turnover-s1 | [4.45%](runs/turnover-s1-b505b798-pocket-tank--p1.json) | Pass | 0 |
| [EX153](../../experiments.md#ex153) | onecall-s1 | [3.38%](runs/onecall-s1-b0dbc7f7-pocket-tank--p1.json) | Pass | 0 |
| [EX153](../../experiments.md#ex153) | onecall-s1r | [10.51%](runs/onecall-s1r-7067f8d2-pocket-tank--p1.json) | Pass | 0 |
| [EX157](../../experiments.md#ex157) | deadlines-a | [10.11%](runs/deadlines-a-54ad01a7-pocket-tank--p1.json) | Pass | 0 |
| [EX158](../../experiments.md#ex158) | frames-a-nopub | [1.85%](runs/frames-a-nopub-bebd6b97-pocket-tank--p1.json) | Pass | 0 |
| [EX158](../../experiments.md#ex158) | frames-b-oldcadence | [2.93%](runs/frames-b-oldcadence-92243a4f-pocket-tank--p1.json) | Pass | 0 |
| [EX157](../../experiments.md#ex157) | deadlines-q60k | [9.29%](runs/deadlines-q60k-b1c22417-pocket-tank--p1.json) | Pass | 0 |
| [EX153](../../experiments.md#ex153) | onecall-s2 | [3.73%](runs/onecall-s2-18d20624-pocket-tank--p1.json) | Pass | 0 |
| [EX153](../../experiments.md#ex153) | onecall-s2b | [2.91%](runs/onecall-s2b-9d3496a5-pocket-tank--p1.json) | Pass | 0 |
| [EX153](../../experiments.md#ex153) | onecall-s2only | [-7.11%](runs/onecall-s2only-602b1cbf-pocket-tank--p1.json) | Pass | 0 |
| [EX159](../../experiments.md#ex159) | calls-s1 | [-2.03%](runs/calls-s1-09cc6fa1-pocket-tank--p1.json) | Pass | 0 |
| [EX159](../../experiments.md#ex159) | calls-s1s2a | [-1.37%](runs/calls-s1s2a-db128518-pocket-tank--p1.json) | Pass | 0 |
| [EX159](../../experiments.md#ex159) | calls-s1s2as3 | [-1.61%](runs/calls-s1s2as3-f4108bf2-pocket-tank--p1.json) | Pass | 0 |
| [EX159](../../experiments.md#ex159) | calls-s2 | [1.59%](runs/calls-s2-3b7c7cd5-pocket-tank--p1.json) | Pass | 0 |
| [EX159](../../experiments.md#ex159) | calls-s3 | [-0.75%](runs/calls-s3-39e89862-pocket-tank--p1.json) | Pass | 0 |
| [EX157](../../experiments.md#ex157) | deadlines-u256k | [9.78%](runs/deadlines-u256k-b391b6b0-pocket-tank--p1.json) | Pass | 0 |
| [EX157](../../experiments.md#ex157) | deadlines-u64k | [8.45%](runs/deadlines-u64k-05ed799a-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-abort | [-0.88%](runs/flags-abort-cdd97cf4-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-all-nosimd | [-2.29%](runs/flags-all-nosimd-08809cfd-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-all | [2.41%](runs/flags-all-e971ebbf-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-base-O3 | [-0.94%](runs/flags-base-O3-f38df6bc-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-base-O3inl | [-0.48%](runs/flags-base-O3inl-cfc3d6c9-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-base-O4 | [-1.96%](runs/flags-base-O4-33b86dcc-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-bulk | [-2.85%](runs/flags-bulk-e3b68a44-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-inl2000 | [5.09%](runs/flags-inl2000-5ce976e6-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-inl250 | [0.36%](runs/flags-inl250-a5094223-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-inl4000 | [6.32%](runs/flags-inl4000-8c091acf-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-inl500 | [0.45%](runs/flags-inl500-c020cd86-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-inline1000 | [0.85%](runs/flags-inline1000-5dc81c51-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-nodebug | [3.34%](runs/flags-nodebug-f33104f1-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-opts | [-12.79%](runs/flags-opts-a03c18e9-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-simd-O3 | [-0.93%](runs/flags-simd-O3-5ba54786-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-simd-extc | [-0.19%](runs/flags-simd-extc-0201ac68-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-simd-inl1000 | [4.35%](runs/flags-simd-inl1000-b1686279-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-simd-tail | [-2.95%](runs/flags-simd-tail-a0d50c14-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-simd | [0.49%](runs/flags-simd-78be7c25-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-sinl-O3 | [2.18%](runs/flags-sinl-O3-2f1ed3c9-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-sinl-O4 | [-0.21%](runs/flags-sinl-O4-73ac2afb-pocket-tank--p1.json) | Pass | 0 |
| [EX154](../../experiments.md#ex154) | flags-unroll600 | [2.71%](runs/flags-unroll600-4a58c696-pocket-tank--p1.json) | Pass | 0 |
| [EX158](../../experiments.md#ex158) | frames-s1 | [0.35%](runs/frames-s1-d8f187d9-pocket-tank--p1.json) | Pass | 0 |
| [EX158](../../experiments.md#ex158) | frames-s1a | [-0.37%](runs/frames-s1a-75a1048a-pocket-tank--p1.json) | Pass | 0 |
| [EX158](../../experiments.md#ex158) | frames-s1b | [2.01%](runs/frames-s1b-155f7517-pocket-tank--p1.json) | Pass | 0 |
| [EX158](../../experiments.md#ex158) | frames-s1s3 | [-0.29%](runs/frames-s1s3-a4640d81-pocket-tank--p1.json) | Pass | 0 |
| [EX158](../../experiments.md#ex158) | frames-s3 | [3.21%](runs/frames-s3-0ac83196-pocket-tank--p1.json) | Pass | 0 |
| [EX160](../../experiments.md#ex160) | hwloop-s2b | [1.76%](runs/hwloop-s2b-b924ac73-pocket-tank--p1.json) | Pass | 0 |
| [EX160](../../experiments.md#ex160) | hwloop-s2be | [0.63%](runs/hwloop-s2be-efa41ae8-pocket-tank--p1.json) | Pass | 0 |
| [EX156](../../experiments.md#ex156) | kernel-s1s2 | [11.92%](runs/kernel-s1s2-b2a0ac89-pocket-tank--p1.json) | Pass | 0 |
| [EX156](../../experiments.md#ex156) | kernel-s1s2s3 | [12.10%](runs/kernel-s1s2s3-cbc02ff6-pocket-tank--p1.json) | Pass | 0 |
| [EX156](../../experiments.md#ex156) | kernel-s2 | [2.34%](runs/kernel-s2-02afd0fa-pocket-tank--p1.json) | Pass | 0 |
| [EX156](../../experiments.md#ex156) | kernel-s2s3 | [-1.46%](runs/kernel-s2s3-a35c7315-pocket-tank--p1.json) | Pass | 0 |
| [EX153](../../experiments.md#ex153) | onecall-c1 | [9.46%](runs/onecall-c1-6125df00-pocket-tank--p1.json) | Pass | 0 |
| [EX150](../../experiments.md#ex150) | spec-tax | [-12.38%](runs/spec-tax-e23efb72-pocket-tank--p1.json) | Pass | 0 |
| [EX150](../../experiments.md#ex150) | spec-tax1 | [1.72%](runs/spec-tax1-f1687a3f-pocket-tank--p1.json) | Pass | 0 |
| [EX150](../../experiments.md#ex150) | spec-tax2 | [-3.63%](runs/spec-tax2-a31e1614-pocket-tank--p1.json) | Pass | 0 |
| [EX150](../../experiments.md#ex150) | spec-tax3 | Pending | Pending | 0 |
| [EX150](../../experiments.md#ex150) | spec-tax4 | Pending | Pending | 0 |
| [EX150](../../experiments.md#ex150) | spec-tax7 | Pending | Pending | 0 |
| [EX163](../../experiments.md#ex163) | spills-m1 | Pending | Pending | 0 |
| [EX163](../../experiments.md#ex163) | spills-m2 | Pending | Pending | 0 |
| [EX152](../../experiments.md#ex152) | turnover-s1s2 | Pending | Pending | 0 |
| [EX152](../../experiments.md#ex152) | turnover-s1s2s3 | Pending | Pending | 0 |
| [EX152](../../experiments.md#ex152) | turnover-s2 | Pending | Pending | 0 |
| [EX152](../../experiments.md#ex152) | turnover-s3 | Pending | Pending | 0 |

## Interpretation and adoption

The adopted runtime bundle is EX153/EX155/EX156/EX157 with EX154 inline threshold 4000. [M3 individual and combined results](../perf-combined-2026-09-20/README.md) confirm the bundle; [EX152 turnover confirmation](../perf-turnover-2026-09-20/README.md) found no incremental gain. Published as ready PRs [110](https://github.com/joakimeriksson/esp32sim/pull/110), [111](https://github.com/joakimeriksson/esp32sim/pull/111), [112](https://github.com/joakimeriksson/esp32sim/pull/112), [113](https://github.com/joakimeriksson/esp32sim/pull/113) and [114](https://github.com/joakimeriksson/esp32sim/pull/114); proposed upstream, not merged.

Frame suppression and old-cadence probes intentionally change presentation, so matching retired work and console output cannot qualify those as product candidates. Spec-tax variants measure tracking overhead, not a completed speculative scheduler speedup. Elastic blockq variants fail the pinned instruction/output contract and are not comparable exact-work speedups. Spin-loop fast-forward was rejected from census evidence before implementation. Remaining unqueued region-formation and in-region-call work is incomplete, not a negative benchmark result.

The latest local TinyDraw 2.3.0 release was used for browser boot/manual checks; timed TinyDraw comparisons used the older pinned benchmark battery. Manual Safari/Chrome realtime readings remain observations, not controlled timing results; see the combined evidence.

## Snapshot manifest verification

`manifest.json` records the earlier snapshot. Its `README.md` hash predates the
closeout rewrite and is intentionally historical. `closeout-manifest.json` records
the closeout snapshot; its README hash predates this clarification. Verify either
README entry against the corresponding committed snapshot rather than the current
README. The other entries retain their original hashes; files removed under the
[evidence retention policy](../README.md) remain in the [capture archive](../ARCHIVE.md).
