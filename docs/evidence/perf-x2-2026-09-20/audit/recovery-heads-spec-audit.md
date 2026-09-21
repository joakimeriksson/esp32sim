# Recovery audit: heads and spec2 → M3

**Verdict: APPROVED for the seven frozen M3 timing candidates, with provenance/test-record corrections below. No strict 30-second exact gate is missing.** This is an artifact/provenance audit, not adoption approval or a correctness proof. No implementation changes, builds, runtime tests, experiments or remote sync were performed.

Paths are relative to `/Users/alice/src/a/esp32sim-x2`; **S** means `../esp32sim-x2-spec/runs`. Queue scope: `queue/jobs.jsonl`, rows named heads-s1/s2/s3 and spec2-ex150-k4/k16/k64/tax1.

## Frozen artifacts and strict gates

SHA-256 was recomputed from every queued candidate and its named `wasm/` artifact: all seven pairs are identical and match their frozen filename prefixes. Every job uses the same frozen baseline, byte-identical to `wasm/base.wasm`:

`cfbf86c5538646a47d9614469463ffbc666e391e1c69ee24f3ecb9e737567117`

| Job | Full frozen SHA-256 | Source | Strict 30s receipt |
|---|---|---|---|
| heads-s1 | 7a7e1c98ed21ec10a34eca887596ea440a679ff011bd9daf47751345f8d91526 | 14819b5a + frozen patch = 36bf6a8e | runs/heads/exact-heads-s1.txt |
| heads-s2 | 37b9d5aa44f0be92363e163cf519ac11cd04af824967da4b002dd50391fa9007 | 7f178131, empty patch | runs/heads/exact-heads-s2.txt |
| heads-s3 | af124c493c9a58f7c63bc93c7ca3db7fa4d4c68ddf6ab41bea53f1623479624e | 1d7a9179, empty patch | runs/heads/exact-heads-s3.txt |
| spec2-ex150-k4 | 5e7ba389ea2f6661b6a6b549fbe2daaf62fd4dd6286b4fa9d4b6d553a9ceebbe | af2f1f23, empty patch; SPEC_K=4 | S/spec2-k4-exact30.txt |
| spec2-ex150-k16 | 21ec74c7339f916274a3c2eb64d0d05fc526753eadc3e047217e64c8178876d0 | af2f1f23, empty patch; SPEC_K=16 | S/spec2-k16-exact30.txt |
| spec2-ex150-k64 | b9835afcac59204c54f7716976127f002ad728cc0b32a0fc0976d25912909114 | af2f1f23, empty patch; SPEC_K=64 | S/spec2-k64-exact30.txt |
| spec2-ex150-tax1 | 2c1fae3c544f03aef75510fe100e03f2085dd172b39a6440db0dd412cdb01ba9 | 4b2ac321 + summary-script-only patch | S/spec2-tax1-exact30.txt |

All seven receipts explicitly record secs=30, EXACT=true, **10,073,833,775 instructions, 3,094 frames, zero panics and zero JIT failures**, with these full hashes matching `notes/exact-ref-pocket-tank-30s.json`:

- Console: `b9d9966e5d9d73984203c11846eb7f8c86507cb53ffe133c4e7e92dff66319d7`
- Frame content: `847b5478d0a7b2a6648804b7ada0c0f5f4a756c427e36020779104f14e6f4fdf`

This checks the actual strict fields, not merely an EXACT-ok sentence or profiling summary. Gate implementation: `bin/exact.mjs`, final comparison. Receipts identify artifact paths rather than embedding artifact hashes; current file equality and the saved build receipts establish the available linkage, not a tamper-proof execution attestation.

## Provenance resolution

- **heads-s1:** `wasm/heads-s1.rev` and the queue correctly retain build-time HEAD `14819b5ab4aa1df45f21ac8c0ea476b7c9dcc983`. Its 203-line `wasm/heads-s1.dirty.patch` is byte-for-byte identical to `git -C ../esp32sim-x2-heads diff 14819b5a 36bf6a8e`. Patch SHA-256: `6e7bf905e6b13b2ea8e4216d7ca375b1266c2b9ec8bec38a1286d56410e6c918`. Thus `notes/heads.md` names the correct committed source, but the queue revision alone omits the implementation. **Carry both build provenance (parent + patch) and canonical source 36bf6a8e in the sync metadata; do not relabel this as a clean build of either revision.** No rebuild is needed to resolve this discrepancy.
- **spec2 engine:** exact receipts name `spec2-k*.wasm`, while the queue names `spec2-final-k*.wasm`. Recomputed hashes prove original = final = frozen for each K. `S/spec2-clean-artifacts.txt` independently records identical clean rebuilds; final `.rev` files name af2f1f23 and final dirty patches are empty. All 11 production-source hashes in `S/spec2-production-sources.sha256` match af2f1f23; all 11 production postimage blob IDs in each original K/force dirty patch also match that commit.
- **spec2 tax1:** `wasm/spec2-tax1.dirty.patch` only changes `tools/spec-summary.mjs` dispatch reporting, not compiled Rust. Preserve it as build metadata; production source is 4b2ac321. Build/hash receipt: `S/spec2-tax1-build.txt`. This is the leader-only tracking control, not the full engine's matched overhead control (`notes/spec.md` §2).
- **Forced restore, validation only:** `S/spec2-force-exact30.txt` passes the same strict fields. Original and final artifacts are identical at `897542c248d3768cf7d499b742b4c809173657d6f11b9db30cf562f6856cdfbd`. It is not one of the seven queued jobs; do not add it to the timing batch.

## Test receipts and required record corrections

- **spec2:** `S/spec2-jit-tests.txt` records 78,922 differential cases for each K and 78,903 default-off; `S/spec2-tax1-tests.txt` records 78,903. The K test headers retain earlier dirty-tree revisions, not clean af2f1f23 runs; the frozen production-source manifest and clean artifact comparison above supply the production-source linkage, not a hash of the test executable. Invocation details are in `S/spec2-test-commands.txt`. `S/spec2-focused-tests.txt`, `S/spec2-native-tests.txt` and `S/spec2-soc-tests.txt` contain passing native results. No tests were rerun for this audit.
- **heads:** `notes/heads.md` and queue notes claim 78,904 cases for each variant, but no raw matching differential-test log was located in shared `runs/`, the heads worktree or its target logs. Commit ff2184da contains the directed test; that is not a test-execution receipt. **Recover the raw receipts or label the 78,904 claims as note-reported/unverified.** This does not erase the three independently available strict exact30 receipts. The note's native-suite-before-PR requirement remains open, separate from M3 timing readiness.

**Coordinator follow-up:** recovered the original heads session's three tool-result PASS lines and corresponding commands into `runs/heads/recovered-jit-test-receipts.json`. At 19:06:52Z the s2 configuration passed 78,904 cases; at 19:07:36Z the s1-policy configuration (`ALIAS_SEQ=false` on the test-bearing s2 source) passed 78,904; at 19:13:34Z s3 passed 78,904 after its committed production build. This resolves the missing execution receipts, with the limitation that the s1 test run disables sequential aliasing on later test-bearing source rather than checking out the exact s1 commit. Outputs were filtered by the original shell pipelines, but retain the explicit suite PASS lines. No test rerun was needed.

**Handoff:** sync only the existing frozen candidates and baseline, accompanied by the provenance correction and recovered heads test receipts. No missing exact30 gate requires a new run. This audit did not change `queue/jobs.jsonl` or `notes/`.
