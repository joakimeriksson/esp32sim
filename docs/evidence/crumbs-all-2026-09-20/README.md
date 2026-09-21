# DMA, panel, FMA and scheduler overhead reduction

PR #118 integrates the measured EX170 `crumbs-all` candidate on top of PR #117. Its runtime/test changes are in source commit `2c0bd42e`: DMA mapped-run copying with exact page-version bumps, AMOLED row writes, helper-equivalent FMA inlining and a cached display-push interval. GPIO-mask and frame-publication experiments are excluded.

## Pocket Tank screen

M3 Pro, Chrome 153.0.8010.53 / V8 15.3.76.13, 30 guest seconds, release WASM with inline threshold 4000 and no wasm-opt. Baseline source: `414c6e07`.

| Measurement | Result |
| --- | --- |
| Baseline median wall time | 32.497740 s |
| Candidate median wall time | 29.635458 s |
| Reduction in median wall time | 8.81% |
| Individual paired reductions | 8.74%, 8.87% |
| Three identical-build control screens | −0.18%, −0.31%, 0.37% |

These are two balanced pairs per screen, not statistical confirmation or broad-firmware validation. Lower wall time is better. The controls describe observed variation, not a universal noise threshold.

## Validation

- 314 native tests passed across `esp32s3`, `xtensa-lx7`, `esp-soc` and `esp-periph`; zero failures and one ignored test.
- 78,903 WASM differential cases passed; all 81,884 compiled modules were released.
- Strict Pocket Tank gate: 10,073,833,775 instructions, 3,094 frames, matching console and frame-content hashes, zero panics and zero JIT failures.
- Workspace/all-targets and WASM Clippy passed with warnings denied. The DMA differential test passed again after a test-only divisibility-syntax fix.
- Independent scoped reviews found no blockers. Rebuilt non-custom WASM sections match the measured candidate byte-for-byte.

Artifact SHA-256 values:

- Baseline: `cfbf86c5538646a47d9614469463ffbc666e391e1c69ee24f3ecb9e737567117`
- Measured candidate: `0d0a0603b0fa5e739df429421625250c0625975ec898e450e4fac538d3e62fdf`
- Rebuilt production module: `2e6f80f4205fd76595dbf97d589083f2273ef56ac0b161d65727255d3e15671d`
- Shared non-custom sections: `6e7a22b7c7a7a9d90d0b0f950bca1be4b46409eb084177811904c81ac64ce853`

The full [test receipts](https://github.com/joakimeriksson/esp32sim/tree/codex/perf-x2-records-0920/docs/evidence/perf-x2-2026-09-20/integration) and [benchmark summary](https://github.com/joakimeriksson/esp32sim/blob/codex/perf-x2-records-0920/docs/evidence/perf-x2-2026-09-20/runs/crumbs-all.json) are supplementary records in the separate documentation PR, not additional runtime changes in #118.

## Limits

The FMA path preserves the existing helper's behavior, including its pre-existing subnormal-result discrepancy with hardware FMA. The interval cache relies on the current fixed-rate boards: default 50 Hz and AMOLED 120 Hz. Arbitrary runtime rate increases through a custom native board are not immediately observed. Neither limitation is fixed here. No new performance benchmark was run during integration.
