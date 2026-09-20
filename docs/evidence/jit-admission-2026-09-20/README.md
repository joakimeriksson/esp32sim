# JIT admission descriptor: candidate awaiting confirmation

The ordinary combined + 4000 build copies a 112-byte cached region descriptor before validating it on non-resumed, unobserved entries. The [baseline emitted WASM](baseline-run-inner.wat) shows `i32.const 112; memory.copy` immediately before the epoch check. Candidate `7d7231b7` replaces `Cell<Hot>` with a scoped `RefCell<Hot>` borrow and reads the fields directly. [Source patch](candidate.patch), [candidate emitted WASM](candidate-run-inner.wat).

The candidate retains the epoch, budget, boundary, hardware-loop and page-version checks. It copies only the four values needed after admission, drops the borrow before calling generated code and releases it on every slow-path exit before updating the descriptor. It introduces borrow bookkeeping and grows the compiled block record from 288 to 296 bytes in these builds. Avoiding the bulk copy may or may not offset those costs; no speedup is claimed.

Validation so far: Rust 1.98.1 release WASM compilation and WASM-target Clippy with `-D warnings` passed. Differential tests, full-workload exactness and performance confirmation have not run for this candidate. The ordinary browser artifact remains unchanged. The M3 is reserved for other work and has not been contacted for this investigation.

The separate diagnostic commit `13d7b331` counts admission paths under `jit-profile`: resumed or observed entry, first failing hot guard, hot/cold region entry and rejection, block-body fallback and retained-loop use. It passed a WASM check with `cpu-profile,jit-profile`. [Diagnostic patch](diagnostic-counters.patch). Counters record frequency, not cost; they must not be enabled for timing comparisons.

The [staged runner](campaign.sh), not launched, will use combined-inl4000 as baseline, run native tests, WASM differential and exactness gates, then two identical-build controls and four balanced Pocket Tank pairs. Three old-battery TinyDraw pairs follow only if every Pocket Tank pair improves. Before launch, transfer a source snapshot with usable Git metadata into `admission-confirm`; the runner relies on it for provenance. A positive screen still needs interpretation against controls and manual latest-firmware checks.

This follows the [exploratory profiles](../profile-combined-2026-09-20/README.md), which may overlap Lightroom activity. It is distinct from EX030's 36-byte helper-decoration copy and EX136's previously unsuccessful active-loop and rejection shortcuts.

Separate follow-up: the user reported unexpectedly slow performance on an iPhone 17 Pro during manual testing. No cause or numeric result has been established; investigate that separately from this candidate.
