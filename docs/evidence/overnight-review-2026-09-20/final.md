# M3 Pro: review fixes versus the pre-review stack

The review fixes preserve native Pocket Tank speed in this quick screen: **53.50132→53.29427 seconds**, a **0.39% reduction**, across two balanced pairs. This is near-flat evidence and does not establish recovery from the native regression measured against main. Baseline is the pre-review stack `661623ea`; candidate is reviewed source `946497a3`. [Native measurements](runs/final-native-pocket-tank/summary.json).

| Pocket Tank, 30 guest seconds | Pre-review median | Reviewed median | Wall-time reduction |
| --- | ---: | ---: | ---: |
| [Native](runs/final-native-pocket-tank/summary.json) | 53.50132 s | 53.29427 s | 0.39% |
| [Chrome, identical final JavaScript glue](runs/final-pocket-tank/summary.json) | 28.62113 s | 28.41964 s | 0.70% |

Each row has two balanced pairs. Browser pair reductions were 0.11% and 1.29%; these small differences do not establish an additional repeatable speedup. No confidence interval or same-build control was collected. The browser and native results use the same M3 Pro and build conditions as the [main-to-stack comparison](README.md). These follow-up numbers are incremental and must not be added to that comparison's percentages. [Final audit](final-audit.json).

All four browser arms retired **10,073,833,775** instructions, passed the firmware output checks and had zero JIT failures. Console hashes, non-WASM input hashes and browser versions matched. All four native arms matched per-core instruction totals and console output and exercised the JIT. No image/audio equality was checked. The review changes device-tick scheduling, so this report does not claim identical frame publication or frame-content hashes. [Browser audit](final-audit.mjs) · [native receipts](runs/final-native-pocket-tank/summary.json).

An earlier completed browser campaign used each source revision's own glue and measured 28.65613→28.26168 seconds (1.38% less). Its input audit exposed changed `experiments.mjs` and `worker.js` hashes. Those results are [preserved separately](runs/final-pocket-tank-source-glue/summary.json); the table uses the subsequent common-glue comparison to keep runtime inputs equal. All completed samples remain available. [Original runner](final.sh) · [common-glue runner](final-common-glue.sh).

The final build source is `946497a3`. The compared WASM artifacts are identified by their source metadata and SHA-256 in the [baseline record](runs/final-pocket-tank/baseline/build.json) and [candidate record](runs/final-pocket-tank/candidate/build.json). Native executable hashes and command lines are in its [summary](runs/final-native-pocket-tank/summary.json). Builds and timed arms ran serially. Run `node final-audit.mjs .` from this directory to validate the retained result contracts. Source path and account labels throughout this evidence directory are normalized to `/Users/alice artifact, input and source SHA-256 values are unchanged.

This follow-up remains in **EX094** and **EX027**. It changes the candidate to the reviewed source and keeps the pre-review tip as baseline, preserving the earlier main-to-stack result and its native regression. [Catalog](../../experiments.md#ex094).
