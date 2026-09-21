# Turnover confirmation on the combined runtime

EX152 s1, original patch 5593ca35, applied as b3d5bbf4 on aaea620e. Both artifacts use Rust 1.98.1 and LLVM inline threshold 4000 without Binaryen. Baseline is frozen combined-inl4000.wasm. This measures additional benefit over the integrated runtime, unlike the original standalone single-pair 4.45% screen.

Native esp-soc, esp32s3 and xtensa-lx7 tests passed. WASM differential: 78,903 cases, 81,884 modules released. The 30-second exactness gate passed with 10,073,833,775 instructions, 3,094 frames and zero JIT failures. Logs are preserved in /Users/alice/src/a/esp32sim-exp/results/m3/logs/combined-inl4000-turnover-s1*.log.

[Four balanced Pocket Tank pairs](combined-inl4000-turnover-s1.json): median wall reduction -0.6384%; individual reductions +0.0997%, -1.6458%, -0.8979% and +0.0351%. [Identical-build controls](turnover-control-aa.json): +2.5388% and -1.2037%. These controls describe observed variation, not a statistical confidence bound.

No consistent incremental improvement. Do not adopt this candidate; retain combined + inline4000. TinyDraw was skipped by the positive-candidate gate. [Campaign completed successfully](campaign.log). Other turnover variants are not evaluated by this result.
