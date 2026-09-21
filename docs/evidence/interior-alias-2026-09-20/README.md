# Reuse compiled blocks at interior instruction addresses

EX172 reuses an existing compiled block when an eligible return or mid-block exit lands at one of its interior instructions. It enters the existing guarded body at that instruction instead of decoding and compiling another block head. Source: `e458fe64`, on crumbs-all runtime `2c0bd42e`.

The five-file implementation retains page-version checks, the 4,096-slot alias table and the existing indexed-entry guards. Aliasing is off for native execution, pricing, observers and matching probe boundaries. The sequential-distance heuristic can admit nearby static transfers. Diagnostic census code and production hit/miss counters are excluded. Profile builds attribute sampled work to the selected owner block.

## Incremental M3 result

Pocket Tank, 30 guest seconds, release WASM with inline threshold 4000 and no wasm-opt. Chrome 153.0.8010.53 / V8 15.3.76.13.

- Four balanced pairs against **crumbs-all**, not the original round baseline.
- Median wall time: **29.767488→28.758697 seconds**, **3.39% lower**.
- Paired reductions: **3.71%, 2.80%, 3.20%, 3.39%**.
- Two-pair identical-build controls before and after: −0.87% and +0.34%; individual control pairs ranged from −0.98% to +1.00%.
- Generated WASM: **83,786,506→49,478,716 bytes**.

All 16 benchmark arms preserve pinned instructions, console output and input identities, with zero JIT failures. The result supports this integration for the tested workload; general firmware/device improvement was not measured.

## Validation

317 native tests passed, with two ignored. Both ordinary and profile-enabled WASM differential suites passed 78,905 cases. The strict production gate matches 10,073,833,775 instructions, 3,094 frames, console hash and frame-content hash, with zero panics or JIT failures. Native, production-WASM and profile-WASM Clippy passed with warnings denied. Optional combined test/profile-feature Clippy retains two pre-existing test-harness warnings.

Focused tests cover actual RFE return into an interior, stale code, cache flush, bypass gates and deferred-store redispatch with exactly one eventual write. Independent scoped review found no blockers. Existing indexed-entry guards and their tests remain responsible for loop, window and timer behavior.

Frozen candidate SHA-256: `397e8c940dad3f85fe9803e8797059e17f08629d353a564ca1a9314ead3c4688`.

Full [measurements, artifacts and validation receipts](https://github.com/joakimeriksson/esp32sim/tree/codex/perf-x2-records-0920/docs/evidence/heads-on-crumbs-2026-09-20) live in documentation PR #119. This implementation PR adds only the five source/test files and this focused note.
