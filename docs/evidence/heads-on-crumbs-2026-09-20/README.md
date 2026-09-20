# Interior block reuse on crumbs-all: M3 confirmation

**The cleaned EX172 heads-s3 candidate reduced Pocket Tank median wall time by 3.39% beyond crumbs-all.** All four balanced pairs improved. Generated WASM bytes fell from 83,786,506 to 49,478,716. This supports adopting the reviewed candidate for this workload; it does not establish a universal firmware or device improvement.

## Comparison

| Campaign | Pairs | Baseline median | Candidate median | Wall-time reduction | Paired reductions |
| --- | --- | --- | --- | --- | --- |
| [Identical-build control before](runs/control-before.json) | 2 | 29.405040 s | 29.661675 s | −0.87% | −0.98%, −0.76% |
| [heads-s3 on crumbs-all](runs/heads-on-crumbs.json) | 4 | 29.767488 s | 28.758697 s | **3.39%** | **3.71%, 2.80%, 3.20%, 3.39%** |
| [Identical-build control after](runs/control-after.json) | 2 | 29.728957 s | 29.626835 s | 0.34% | −0.32%, 1.00% |

Positive percentages mean less wall time. The headline is the ratio of arm medians, not the median paired reduction. Controls describe the campaign's observed variation; no confidence interval is claimed. This is an incremental comparison against crumbs-all, not against the original round baseline and not a sum of earlier percentages.

M3 Pro, Chrome 153.0.8010.53 / V8 15.3.76.13, Pocket Tank 30 guest seconds. Each campaign alternates AB/BA order. [Audit](audit.json) verifies all 16 arms: expected frozen WASM hashes, 10,073,833,775 instructions, matching console hashes and browser versions, unchanged non-WASM inputs, `passed=true` and zero JIT failures. The [runner log](runner.txt), [runner](runner.sh) and [provenance](provenance.json) preserve conditions and input identities.

## Source and artifacts

- Baseline runtime: crumbs-all `2c0bd42e`, production SHA-256 `2e6f80f4205fd76595dbf97d589083f2273ef56ac0b161d65727255d3e15671d`.
- Candidate: `e458fe64`, clean production SHA-256 `397e8c940dad3f85fe9803e8797059e17f08629d353a564ca1a9314ead3c4688`.
- Both: release WASM, inline threshold 4000, no wasm-opt and no profiling features. Harness/glue source is unchanged from `414c6e07`.

The candidate changes five source/test files. It reuses a valid compiled block at an interior instruction after eligible arrivals instead of decoding and compiling another head. It preserves the existing 4,096-slot alias table, page validation and indexed guarded entry. The sequential-distance heuristic can admit nearby static transfers; it is not a proof of arrival cause. Native aliasing stays disabled and allocates no alias slots. New census instrumentation and production hit/miss counters are absent. The sampled profiler attributes work to the selected owner block.

## Correctness gates

- [Native tests](validation/native.txt): **317 passed**, zero failures, two ignored.
- [WASM differential tests](validation/differential.txt) and [profile-enabled differential tests](validation/profiled-differential.txt): **78,905 passed** each.
- [Strict production check](validation/exact.txt): 30 guest seconds, **10,073,833,775 instructions**, **3,094 frames**, matching console and frame-content SHA-256, zero panics and zero JIT failures.
- Clippy with warnings denied: [native](validation/clippy-native.txt), [WASM production](validation/clippy-wasm-production.txt) and [WASM profile](validation/clippy-wasm-profile.txt) pass. Optional combined test/profile-feature Clippy retains two pre-existing test-harness warnings; no all-feature lint-clean claim is made.

Directed coverage includes actual RFE return into an interior, stale code, flush and observer/pricing/probe bypasses. A deferred store verifies no premature write, exactly one eventual write, two total retirements and alias reuse without rebuilding. The existing indexed-entry suite covers the underlying loop/window/timer machinery. Independent scoped source review found no blockers; these tests are not a proof for every firmware.

## Disposition

Proceed with the focused implementation PR above the existing stack. Keep these full records in documentation PR #119 and retain the earlier standalone screens under [EX172](../../experiments.md#ex172). No additional mechanism or experiment ID was introduced. The local Node gate's elapsed time is not a benchmark result.
