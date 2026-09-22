# Direct region edge validation (EX181)

The review fixes pin the presence of direct forward branches and both explicit and hardware-loop backedges. Test-only emitter counters are sampled around each existing differential program, so reverting to the semantically correct dispatcher path fails the corresponding assertion. Builds without the `wasm-jit-tests` feature omit these counters. Debug and differential-test builds also assert that chunk heads are unique, the invariant that makes the head count match the dispatch-block nesting. The hardware-loop encoding check now includes offset 5.

EX167 used a shared jump cache and cross-module `return_call_indirect`; EX181 emits intra-function `br` edges inside one region. EX112's successor-ordering portion is first tested independently here. No new performance experiment was run for these review fixes.

Historical measurements from the [x5 closeout](https://github.com/joakimeriksson/esp32sim/pull/139) compare each variant with x5 base `8cbe0da4` and report Pocket Tank wall reductions of 1.81% for s1, 2.48% for s2 and 2.15% for s3. The measured s3 reduction was 0.33 percentage points smaller than s2’s; s3's TinyDraw gain was 5.20%. Excluded finer chunks s4a gained 4.06% on TinyDraw but regressed Pocket Tank 1.93%; s4b regressed Pocket Tank 7.36%. These are measured bundle comparisons, not isolated attribution of each added stage.

Initial A/A controls had only two pairs: +0.75% Pocket Tank and +0.70% TinyDraw, with individual reductions spanning −0.27% to +1.65%. They ran first and are cold-start sensitive; dropping their first pair gives −0.06% and −0.27%. These sparse controls do not establish a stable noise floor.

Measured s3 source is `510cfc20`. The published edge layer includes subsequent x4 integration and these review fixes; the measured result is not a fresh timing claim for that head. Native tests do not compile the wasm32 emitter. Its correctness gate is `tools/wasm-jit-test.sh`, which compares CPU state, memory, traps and retirement against the interpreter and checks every emitted module with the JS WASM validator.

Validation commands use `CARGO_BUILD_JOBS=4` with no `nice` scheduling override. The [compact validation receipt](validation.json) records results and source hashes; scratch logs are retained outside Git. No raw host/process inventory is retained in this evidence.

The revised layer passes **81,629 WASM differential cases**, releases all **84,309 compiled modules** and passes wasm32 Clippy with warnings denied. Disabling only forward direct branches fails with “forward-edge region emitted no direct forward branch”; disabling only self-loop direct branches fails with “bnez self-loop emitted no direct backedge”. Both mutations retain correct dispatcher fallback and were restored before the final passing run. The receipt identifies the tested sources by base revision plus file hashes. The optional two-byte unused hardware-loop wrapper cleanup is left unchanged.
