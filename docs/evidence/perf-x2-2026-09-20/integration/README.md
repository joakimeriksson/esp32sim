# Local crumbs-all integration

At the user's request, the measured **EX170 crumbs-all** source was integrated on base `414c6e07` and committed as `2c0bd42e` on `codex/perf-crumbs-0920`, directly above PR #117. It combines candidate commits `dbe39b3d`, `2485760a`, `ca74919d`, `b0fcf56c` and `6f6db8fc`: DMA run copying, AMOLED panel row writes, helper-equivalent FMA inlining and the cached display-push interval. GPIO masks and frame-publication changes are excluded. Publication preparation changed only the DMA test's divisibility syntax to satisfy Clippy; production source remains identical to the measured candidate.

## Validation

- [Native tests](native.txt): **314 passed**, zero failures, one ignored across `esp32s3`, `xtensa-lx7`, `esp-soc` and `esp-periph`.
- [Workspace/all-targets Clippy](clippy-workspace.txt) and [WASM Clippy](clippy-wasm.txt): passed with warnings denied. The [DMA differential test](dma-test-after-lint.txt) passed again after the test-only lint fix.
- [WASM differential suite](jit-tests.txt): **78,903 passed**, with all 81,884 compiled modules released.
- [Strict Pocket Tank check](exact.txt): 30 guest seconds, **10,073,833,775 instructions**, 3,094 frames, matching console and frame-content SHA-256, zero panics and zero JIT failures.
- [Production build](build.txt), [test build](jit-build.txt) and [artifact comparison](validation.json): production SHA-256 `2e6f80f4205fd76595dbf97d589083f2273ef56ac0b161d65727255d3e15671d`. Its non-custom WASM sections are byte-identical to the M3-measured `crumbs-all` artifact `0d0a0603b0fa5e739df429421625250c0625975ec898e450e4fac538d3e62fdf`; differences are confined to custom sections. The local ignored `web/wasm/esp32sim.wasm` was refreshed with this production build, not the jit-tests build.

Independent source reviews found no blockers in DMA/panel or FMA/scheduler scope. Reviewer checks include [119 S3 native tests](review-dma-panel-tests.txt) and an additional [1M-triple FMA sweep](review-fma-sweep.txt). Remaining coverage opportunities include deterministic extreme panel windows, RTC/MMIO DMA spans, halfway FMA cases and publication boundaries across run/reboot. No broad firmware or interactive-input confirmation is claimed.

The interval cache assumes the existing fixed-rate board behavior: default 50 Hz and AMOLED 120 Hz. An arbitrary native board that increases its rate during execution would not be observed immediately; no current production caller doing this was found. The pre-existing libm/hardware subnormal FMA discrepancy remains unchanged ([original finding](../notes/crumbs.md)).

## Performance and scope

The original M3 [two-pair screen](../runs/crumbs-all.json) remains **8.81% lower wall time**, pairs 8.74% and 8.87%. Local correctness-run wall times are not benchmarks. Adoption here means source integration and local browser-artifact selection, not statistical confirmation, an upstream merge or deployment. The publication branch depends on PR #117; existing stack history must be retained. The separate M3 wave-2 queue and frozen baseline were not changed by this integration.
