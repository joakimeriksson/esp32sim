# x3/x4 execution improvements

The packed x4 integration reduces Pocket Tank browser wall time by 13.08% and 13.58% in two four-pair jobs against the same plain base. The shipping default enables up to 128 scheduling rounds on wasm32. Restoring the measured indexed loop after a Clippy rewrite makes all 1,356 production WASM function bodies match the measured packed build. [First job](x4/everything-accx-k128-pack32.json), [repeat](x4/everything-accx-k128-pack32-r2.json), [code comparison](codegen/default128-comparison.json).

The measured packed build reduces TinyDraw battery wall time by **8.25%**, from median 29.11198 s to 26.71095 s in three pairs (paired reductions 6.67%, 8.50%, 7.69%). All six arms pass, retire the pinned 9,819,885,134 instructions and report zero JIT failures. Candidate SHA-256 is `9766e5164a94a318d63208b27078a4f2dab46a126bfec610fde3ba74c8645a5e`, built from shipping source `4080afdd` without a rounds override. Older layout measurements remain historical entries in the results table. [Final TinyDraw battery result](x4/final-pack32-default128-td.json).

## What is selected

- **Dispatch overhead (EX168):** cache cheap rejection and chainability facts, avoid identical hot-entry refills, copy helper decoration only when a cache view exists and outline cold tails. The rejected inline interrupt-check variant is excluded.
- **Interpret small blocks inside a compiled chain (EX171):** use an explicit instruction-class whitelist. The admitted classes and pricing exclusions are the correctness contract; widening those classes requires new analysis and tests.
- **Memory accesses (EX110/EX173):** skip code-version updates only for mappings no decoded consumer watches. Watching a page marks the blocks containing that page and its neighboring pages, then invalidates published TLB entries. Keep the previous-page update inside that same code flag. A precomputed mapping span shortens each bounds probe. Pack the writable/code flags as adjacent u16 values so a WASM TLB entry stays 32 bytes; native entries also stay 32 bytes by omitting the WASM-only span. Native generated stores retain unconditional version bumps.
- **PIE arithmetic and loads (EX169/EX178):** sum signed-byte products in i32 and widen once, retain a bounded accumulator after a dominating reset and share a range probe over proven post-increment load runs. Exit paths spill the held accumulator. The unheld path still sign-extends and saturates the architectural 40-bit accumulator.
- **Scheduling (EX177):** batch round housekeeping while preserving core order and each core's instruction quantum. Deadlines and deferred device accesses bound batches. The wasm32 cap is 128; the native default remains 1.
- **Previous-page bookkeeping (EX180):** generated stores now follow the bus's rule at page offsets 0..2. This corrects divergent version counters; no stale-code execution or user-visible failure was demonstrated.

Relevant implementation: [scheduler](../../../esp-soc/src/machine.rs), [block classes](../../../xtensa-lx7/src/block.rs), [TLB layout](../../../emu-core/src/bus.rs), [memory emission](../../../xtensa-lx7/src/jit/wasm_memory.rs), [PIE emission](../../../xtensa-lx7/src/jit/wasm_pie.rs).

## Measurements and limits

[The 66-job summary](results.md) retains positive and negative results; detailed historical receipts stay local. Seven shipping, component and control JSON receipts preserve the original aggregate, every paired reduction, arm order, wall times, work totals, output hashes, workload verdicts, JIT failures, browser/V8 versions and asset/harness hashes. The aggregate reduction is the harness's ratio of median arm times; it is not necessarily the median of the per-pair reductions.

The measurement host was an M3 Pro running Chrome 153.0.8010.53 / V8 15.3.76.13, as recorded in the arms. x4 base is `7828e683bec41b9fc47dbbc38391a3a794fc4956` (main `0042d053` plus EX180); x3 base is `5167baada039ba3014575f7c4bcfbcb56ba70240`. Candidates were compared against their plain round base. Subtracting two standalone reductions does not measure incremental benefit in a stack. x4 Pocket Tank jobs have four pairs, TinyDraw jobs three and controls two. Earlier x3 jobs retain their actual pair counts.

Pocket Tank pins 10,073,833,775 instructions per 30 guest seconds. The original shipping source passed 81,626 WASM differential cases, 492 native test executions and Clippy; every original PR layer has its own differential, exact-output and Clippy checks. The native follow-up passed 455 workspace tests before the last bridge-metadata exclusion, then 51 final xtensa tests and full Clippy. Its 1,356 production WASM function bodies match the measured build. [Per-layer validation](validation.json).

The x4 Pocket Tank A/A job reports −0.87% (paired reductions −1.19%, −0.55%); TinyDraw A/A reports +0.24% (+0.26%, +0.22%). The packed-versus-64-byte difference is small relative to this variation and was not measured by a direct candidate-versus-candidate job. Packing is retained for its smaller WASM entry footprint. [Pocket Tank control](x4/control-aa-1.json), [battery control](x4/control-aa-td.json).

Native comparisons against main `988eef30` found the original stack `8cbe0da4` 1.37% slower on Pocket Tank (four pairs) and 2.23% slower on panel (two pairs). The corrections keep native TLB entries at 32 bytes with natural pointer alignment, retain unconditional native generated-store version bumps and restore direct inlined native decoded-block returns. The continuation loop and bridge metadata remain wasm32-only.

Final candidate `3d81936a` measured **0.63% less Pocket Tank wall time** (46.47909→46.18796 s, two pairs) and **0.25% more panel wall time** (2.65570→2.66224 s, four pairs). Both Pocket Tank pairs matched guest work and console output but their timing differences had opposite signs. These results support near parity on the two measured workloads, not a general native speedup or formal equivalence. A separate four-pair same-binary panel control differed by 0.016% in aggregate, with individual arms from 2.58847 to 2.66620 s; it is context, not an error bound for the earlier session. [Native samples, hashes and unsuccessful variants](native-results.json).

Native measurements use rustc 1.98.1 release builds on an M3 Pro, alternating arm order and process-lifetime timing. Pocket Tank pins 10,073,833,775 instructions; panel pins 396,469,561. Every arm matches per-core instruction counts and console SHA with native JIT active. No independent frame/audio comparison was made. Final Pocket Tank ran after competing browser work ended; two earlier overlapping attempts are excluded from performance conclusions. Local native tests have no ROM directory, so ROM-dependent paths are not exercised locally; CI supplies ROMs. Code-section identity establishes identical emitted WASM instructions, not identical debug metadata or a fresh browser measurement.

## Build and verification

Production builds use the existing release recipe with LLVM inline threshold 4000, no wasm-opt and no profiling feature. The earlier measured rounds builds set `ESP32SIM_BB_BUILD=128`; the shipping integration uses its wasm32 default without that variable.

```sh
RUSTFLAGS='-Cllvm-args=-inline-threshold=4000' cargo build --release --target wasm32-unknown-unknown -p esp32sim-wasm
tools/wasm-jit-test.sh
cargo test --release --workspace -- --include-ignored --skip external_
ESP32SIM_VQ_NATIVE=1 cargo test --release -p esp32s3 --test machine --test virtual_stops
cargo clippy --workspace --all-targets -- -D warnings
node tools/check-evidence-privacy.mjs
```

Use the [browser benchmark harness](../../../tools/browser-benchmark/README.md) with the frozen workload assets identified in each JSON, Pocket Tank at 30 guest seconds and TinyDraw in battery mode. Historical candidate source revisions are abbreviated in the results table; full revisions and detailed receipts remain local. Published shipping receipts record full revisions; these identify preserved local experiment commits, not a promise that every rejected experiment branch is published. The six shipping PRs contain the selected source. Raw source revisions must be available to reproduce a rejected candidate exactly.

`codegen/compare.mjs BEFORE.wasm AFTER.wasm` compares binary function bodies and sections. The recorded default build has SHA-256 `9766e5164a94a318d63208b27078a4f2dab46a126bfec610fde3ba74c8645a5e`; the measured packed artifact is `25f0b2ab1676853fc19817d2ec947e057ef8381d15969998e53037b9983f1fa9`. All function bodies and all other non-custom sections except data match. All 34 changed data bytes are diagnostic source-line numbers, identified by `codegen/restored-data-locations.json`. This code identity supports retaining the measured Pocket Tank timings. The TinyDraw battery campaign directly measured that default build. The native follow-up rebuild has SHA-256 `df1941163acd52e1873079c55e47500808ed7bce94bb8ae83f52fe51f0c36ac9`; all 1,356 function bodies still match it. [Native-fix code comparison](codegen/native-fix-comparison.json). The later profiling-build repair removes only calls guarded by `wasm-jit-profile`, so the production code measured at `3d81936a` is unchanged.

For native reproduction, build the baseline and candidate in separate target directories with `cargo build --locked --release -j 8 -p esp32sim --bin esp32sim`, rustc 1.98.1 and no Rust flags. Use the existing [Pocket Tank harness](../native-recovery-2026-09-21/screen-native.mjs) and [panel harness](../native-recovery-2026-09-21/screen-panel.mjs), each taking `BASELINE_BINARY CANDIDATE_BINARY OUTPUT_NAME PAIRS`. Their asset locations are relative to the run directory; input hashes and full workload arguments are in [native-results.json](native-results.json). The measured local and remote trees match over 201 Rust sources/manifests; running `node docs/evidence/perf-x4-2026-09-22/codegen/source-hash.mjs` from the measured source root reproduces that digest.

## Evidence curation

[Redaction manifest](redactions.json) records original and curated hashes and byte sizes for the seven published browser receipts. Detailed historical receipts and superseded code comparisons remain local; no separate archive is published. Curated summaries omit local run-directory paths and the obsolete `screeningOnly` field when present. Timings, artifact hashes, output checks and order are unchanged. Added fields identify round, source revision and workload. Identical provenance maps within each arm are stored once in provenanceByArm after equality checks. Historical summaries elsewhere retain their original screeningOnly metadata; that field no longer determines interpretation. No private original or process inventory is included. Removing local paths limits filesystem navigation, not the numeric comparison; source-revision availability is the separate limitation described above.
