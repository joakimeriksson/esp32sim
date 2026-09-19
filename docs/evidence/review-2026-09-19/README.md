# Review-fix validation — September 19, 2026

The recorded correctness gates passed: 371 native release tests including 14 firmware golden tests, 8 WASM firmware manifests plus JIT handoff, 78,861 default JIT differential cases, workspace Clippy, 16 Node tests, 12 Python tests and worker pacing/channel scripts. The [machine-readable receipt](validation.json) contains the exact commands, exit statuses, suite counts, toolchain versions, source and input identities and artifact hashes. The [review synthesis](../../reviews/2026-09-19-synthesis.md) explains the fixes and remaining gaps.

The receipt identifies source revision `d549d4b27b927e7bd32bd2e4e9cc4188738c8e03` and source tree `f9947f5b8f7173f16e69119ae0f4ec08a0685ae7`. The production WASM build preceded three behavior-preserving Clippy rewrites; the differential rebuild includes them. The [follow-up patch](production-build-followup.patch) records those changes. Production firmware results and differential results therefore refer to distinct artifact hashes, both preserved in the receipt.

Reproduce the checks using the receipt's `commands` entries and matching ROM hashes. Its absolute ROM and Python interpreter paths describe this host; substitute local paths to the same inputs and a local virtual environment. The run used Darwin arm64, Rust 1.98.1 and Node 26.8.2. Native generated-code coverage depends on the arm64 target. [Recorded environment and inputs](validation.json), [native JIT test target](../../../xtensa-lx7/tests/jit_memory.rs)

The scope has explicit limits:

- The native command included ignored tests but filtered 12 tests whose names matched `external_`; no executed test failed or remained ignored.
- `UPDATE_GOLDENS` was not set. Two RV32 reboot-count fixtures were deliberately corrected after invariant checks.
- This aggregate receipt covers the default differential configuration, not a final-source alternate no-inline/profile run.
- The checks establish the recorded emulator behavior, not silicon timing fidelity or a Pocket Tank speedup.

These limits and counts are recorded in [validation.json](validation.json). Full logs remain in the ignored local `work/validation-final-0919/` directory. The receipt preserves their SHA-256 hashes and byte lengths, but there is no public raw-log archive yet. Retain the local copies until any required external archive is accessible, following the [evidence retention policy](../README.md).
