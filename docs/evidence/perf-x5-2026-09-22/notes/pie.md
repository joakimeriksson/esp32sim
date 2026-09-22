> Historical worker note, preserved before browser results were available. Predictions and queued status below are historical; see [closeout](../README.md) and [measured results](../results.json).

# x5 worker `pie` — generated-code quality in the hot kernels

Worktree `esp32sim-x5-pie`, branch `x5/pie`, base `x5/base` = `8cbe0da4`.
Base gate references: `results/diff-base.log` = `PASS: 81626 WASM differential cases`,
`results/exact-base-30s.log` = `EXACT ok`, base artifact sha256 `0a5e09eb1d1e...`.

Status: in progress.

## Pre-work: the `jit-profile` build is broken on x5/base

`cargo build --features jit-profile` fails on the base tree: six callers of
`crate::census::note_code_page(vidx, flags)` (`exec.rs:236`, `block.rs:308`,
`wasm_region.rs:200-201`) outlived the function, removed in `17c40923`
("Synchronize profiling census and cover wide PIE shifts").
Worked around locally with an uncommitted no-op stub purely to take the census;
not part of any candidate. Reported here so the coordinator can decide whether to fix it.

## Census (scratch, not committed)

(pending)

## Candidates

(pending)
