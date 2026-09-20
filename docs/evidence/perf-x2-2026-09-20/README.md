# x2 Pocket Tank screens and recovery handoff

**25 first-wave screens are complete; 16 wave-2 jobs were synced and queued/running at recovery. Subsequently, crumbs-all was integrated locally at the user's request after review and fresh correctness checks.** See the [integration record](integration/README.md) for source scope, tests and the selected local browser artifact; other x2 candidates remain unadopted. The largest first-wave screen is crumbs-all: 32.498→29.635 s, **8.81% less wall time**. This remains a two-pair screen, not statistical confirmation; the later local adoption is a separate decision ([raw summary](runs/crumbs-all.json)). Mechanisms, all candidate screens and dispositions live in the single [experiment catalog, EX167–EX172](../../experiments.md#ex167); retries also update EX028, EX030, EX042, EX095, EX117, EX136, EX150, EX152 and EX158.

## Scope

This documentation-only change is separate from the implementation in PR #118. That PR contains nine implementation/test files plus a [focused validation note](../crumbs-all-2026-09-20/README.md); this archive retains the wider experiment catalog, timing summaries, recovered notes and validation records.

## Conditions and uncertainty

M3 Chrome 153.0.8010.53 / V8 15.3.76.13, Pocket Tank 30 guest seconds, base `414c6e07`, release inline threshold 4000, no wasm-opt. Frozen baseline SHA-256: `cfbf86c5538646a47d9614469463ffbc666e391e1c69ee24f3ecb9e737567117`. Each screen has **two balanced pairs**, `screeningOnly: true`. Positive percentages mean wall reduction, computed as `100 × (1 − median(candidate)/median(baseline))`, not necessarily the median paired reduction. Pair values are retained in the catalog and every raw summary; no confidence interval or broad-workload confirmation is claimed.

A/A controls: [1](runs/control-aa-1.json) −0.18% (pairs −0.58%, 0.21%), [2](runs/control-aa-2.json) −0.31% (−0.38%, −0.24%), [3](runs/control-aa-3.json) 0.37% (0.38%, 0.35%). These show observed variation, not a universal noise threshold. Small singles and stack interactions remain uncertain.

[Recovery audit](recovery-wave1-audit.json) verifies all **100 arms**: 10,073,833,775 instructions, matching browser console SHA, passing work checks, frozen WASM hashes and zero JIT failures. The [25 raw summaries](runs/) preserve per-arm wall times, browser versions and full hashes for firmware/app, model, ROM, bootloader, partition table, WASM, workload manifest, harness and worker/glue inputs. [Frozen jobs](jobs.jsonl), [runner log](wave1-runner.log) and [harness snapshot](harness/) retain the run context. Browser console SHA is `9e8a66e483f741c80db1bd20c62bc43943697c2e6a0053a6af1f7d52af8c67dd`; it is not the separate Node exact-run console hash. Strict Node frame-content gates described in the notes do not turn the browser work/console checks into browser frame-hash validation.

## Wave 2 and provenance

[Sync receipt](audit/recovery-sync.log), [repeat-sync receipt](audit/recovery-sync-repeat.log) and [remote status](audit/recovery-remote-status.txt) cover nine dispatch2 jobs, three heads jobs and four spec2 jobs, with remote SHA checks. Pending is not a benchmark outcome.

- **Dispatch2:** [audited notes](notes/dispatch2.md) and [machine-readable audit](audit/audit.json). Stack jobs include wave-1 s1–s6 and compare against plain base; their eventual reductions are **not incremental wave-2 gains**. t5 stays unqueued: workload gates passed but flash-invalidation proof and directed tests remain incomplete.
- **Heads:** [provenance audit](audit/recovery-heads-spec-audit.md) and [recovered test receipts](audit/recovered-jit-test-receipts.json). s1 was built from `14819b5a` plus a frozen patch equivalent to canonical `36bf6a8e`, not a clean build of that commit. Its 78,904-case receipt tests the s1 policy on later test-bearing source with sequential aliasing disabled. Native-suite-before-PR work remains open.
- **Spec2:** [engine/gates](notes/spec.md) and the same provenance audit. K4/16/64 are exact conservative engines; tax1 is only a leader-mark lower-bound control, not matched engine overhead. The prior interruption was not negative evidence. Restore counts are not measured speedups.

## Reading the preserved notes

[Notes](notes/) are historical snapshots, including predictions, old “queued” headings and suggested next steps; this README/catalog records the recovered status, not approval to adopt those suggestions. Relative lab paths normally resolve under `/Users/alice/src/a/esp32sim-x2`; `notes/spec.md` instead uses `/Users/alice/src/a/esp32sim-x2-spec`, as its opening states. Referenced source trees, sessions and lab files are not all copied into this evidence directory. The audits describe recovered receipts, not new test runs. The FMA subnormal libm/hardware discrepancy in [crumbs](notes/crumbs.md) is pre-existing and unresolved; helper-equivalent candidates do not fix it.
