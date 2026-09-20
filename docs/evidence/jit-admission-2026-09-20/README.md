# JIT admission descriptor: positive M3 confirmation

Candidate `7d7231b7` reduced median wall time by **2.23% in Pocket Tank** and **2.71% in the TinyDraw benchmark battery**, incrementally against combined + inline 4000. Every candidate pair improved. The Pocket Tank identical-build controls were smaller, at +0.73% and +0.05%. This supports a modest gain in this campaign; it is not a confidence interval or evidence of an iPhone improvement. Adopted locally on `codex/perf-admission-0920`. M3 and local iPhone browser test copies now use the measured artifact; immutable benchmark files are retained. Latest-firmware manual retest remains pending. No push or merge has been performed.

| Measurement | Pairs | Median wall-time reduction | Per-pair reductions |
|---|---:|---:|---|
| [Identical-build Pocket Tank controls](admission-control-aa.json) | 2 | 0.39% | 0.73%, 0.05% |
| [Pocket Tank](combined-inl4000-admission-borrow.json) | 4 | 2.23% | 2.34%, 3.96%, 1.55%, 1.94% |
| [TinyDraw benchmark battery](combined-inl4000-admission-borrow-tinydraw.json) | 3 | 2.71% | 2.50%, 2.68%, 2.84% |

All measured runs passed work/output checks. No TinyDraw-specific identical-build control was collected. The [campaign](campaign.log) completed at 15:05:39 UTC on September 20 with exit 0. Remote source snapshot `3c1adbc7` represents local source `7d7231b7`; the [WASM hash](combined-inl4000-admission-borrow.sha256) identifies the measured artifact.

The ordinary combined + 4000 build copies a 112-byte cached region descriptor before validating it on non-resumed, unobserved entries. The [baseline emitted WASM](baseline-run-inner.wat) shows `i32.const 112; memory.copy` immediately before the epoch check. Candidate `7d7231b7` replaces `Cell<Hot>` with a scoped `RefCell<Hot>` borrow and reads the fields directly. [Source patch](candidate.patch), [candidate emitted WASM](candidate-run-inner.wat).

The candidate retains the epoch, budget, boundary, hardware-loop and page-version checks. It copies only the four values needed after admission, drops the borrow before calling generated code and releases it on every slow-path exit before updating the descriptor. It introduces borrow bookkeeping and grows the compiled block record from 288 to 296 bytes in these builds. The measurements above test the net effect of that tradeoff.

Validation: Rust 1.98.1 release WASM compilation and WASM-target Clippy with `-D warnings` passed locally. On M3, [native tests](combined-inl4000-admission-borrow-native.log), [78,903 WASM differential cases](combined-inl4000-admission-borrow-jit-tests.log) and the [30-guest-second Pocket Tank exactness gate](combined-inl4000-admission-borrow-exact.log) passed. The timed artifact used default features and [inline threshold 4000](combined-inl4000-admission-borrow.rustflags), without diagnostic counters.

The separate diagnostic commit `13d7b331` counts admission paths under `jit-profile`: resumed or observed entry, first failing hot guard, hot/cold region entry and rejection, block-body fallback and retained-loop use. It passed a WASM check with `cpu-profile,jit-profile`. [Diagnostic patch](diagnostic-counters.patch). Counters record frequency, not cost; they must not be enabled for timing comparisons.

The [runner](campaign.sh) used combined-inl4000 as baseline, completed correctness gates, then two identical-build controls and four balanced Pocket Tank pairs. Three old-battery TinyDraw pairs followed. These TinyDraw measurements do not substitute for latest-firmware manual checks.

This follows the [exploratory profiles](../profile-combined-2026-09-20/README.md), which may overlap Lightroom activity. It is distinct from EX030's 36-byte helper-decoration copy and EX136's previously unsuccessful active-loop and rejection shortcuts.

Separate follow-up: the user reported unexpectedly slow performance on an iPhone 17 Pro during manual testing. No cause or numeric result has been established; investigate that separately from this candidate.
