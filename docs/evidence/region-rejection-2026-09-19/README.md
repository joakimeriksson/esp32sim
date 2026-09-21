# EX136: cached rejection shortcut, September 19

Candidate: `a5b251ef` on `codex/region-rejection-0919`, based on `d549d4b27b927e7bd32bd2e4e9cc4188738c8e03`. Eight added production lines in `xtensa-lx7/src/jit/wasm.rs`. **Correctness suite passes. Later measurement found it slower in seven of seven Pocket Tank pairs; rejected and PR #95 closed. See Outcome below.**

The [existing EX136 active-loop experiment](../../experiments.md#ex136), commit `6b844713`, cached an admitted `(LEND, LBEG)` pair and showed no gain. This candidate leaves that loop test unchanged. It bypasses the cold owner lookup after a cached entry's budget or boundary guard rejects it, but only when its epoch and every code-page version still match.

Static proof in [the dispatcher](https://github.com/joakimeriksson/esp32sim/blob/sanitized-a5b251ef291fe0fbc367ba33b17736f913cdbf09/xtensa-lx7/src/jit/wasm.rs):

- `Hot` is filled only from the cold path's selected owner/chunk after successful admission. Its length and bloom are exact copies of that region's guards.
- Region removal and block reindexing advance `region_epoch`. New regions use `covered.entry(...).or_insert(...)`; they cannot replace a live covering entry. A block with a live covering region cannot form its own region on that path.
- The shortcut checks code-page versions before returning. A stale region still reaches the original cold invalidation path. It does not use cached function or site pointers.
- Failed budget or boundary guards cannot become acceptable through the cold active-loop test. The same block body receives the same CPU, bus, helper table, budget, entry and memory pointers.

An independent static review accepted this argument, specifically checking owner replacement, stale-page invalidation, cache reindexing and active-loop admission. `git diff --check` passes.

After the CPU quiet window ended, `tools/wasm-jit-test.sh` passed 78,861 differential cases with 76,349 compiled modules released ([raw log](differential.log)). Its existing `wasm_tests/regions.rs` programs vary budgets, head/interior probes, hardware loops, code changes and timer deadlines. No native suite was added for this WASM-only branch.

Production was built first with default features and wasm-opt 132 `-O3`, clearing ambient Rust flags and preserving it before the test build replaced the target artifact. [Build provenance](build.json) records the exact command and toolchain. Optimized SHA-256: `420ddf351c6181b303c980da1d8afc77e9bdf3e3ebe274502e7c93fe157feb73` (7,612,255 bytes). Local artifact: `work/region-rejection-0919/work/ex136-validation/production.wasm` from the repository root.

Pending: screen equal-work pocket-tank against the exact d549d4b2 baseline, checking instruction totals and console hashes before interpreting time. No throughput claim is supported yet.

## Outcome

The pending screen ran against `bf36479b` (the PR's merge base) rather than d549d4b2: [one pair, 0.91% slower](pocket-screen.md). Two three-pair confirmation campaigns then lost every pair, with medians 2.11% and 3.86% slower on a busy host: [confirmation](pocket-confirmation.md). The correctness argument above stands; the change is rejected on speed alone.
