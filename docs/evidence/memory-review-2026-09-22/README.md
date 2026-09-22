# Modeled self-modifying code invalidation

The EX110 watched-code optimization missed the cost-model bus wrapper. On parent
`235c3a4a`, a real S3 machine running four instructions under `ApproximateCostModel`
produces register a2 = 2 instead of 6 after rewriting an ADDI immediate through its
DRAM alias. `RecordingBus` now forwards `note_code_page` to the real bus. The trait
method is required, so wrappers must explicitly forward it and buses without
conditional version bookkeeping must explicitly implement a no-op.

The regression is
[`modeled_self_modifying_code_invalidates_through_the_dram_alias`](../../../esp32s3/tests/approximate_timing.rs).
Its bytes encode `addi a2,a2,1; s8i a4,a3,0; j back` at IRAM `0x40380000`, with a3
pointing to DRAM `0x3fc90002` and a4 = 5. The fourth instruction must add 5 and leave
a2 = 6, PC = `0x40380003` and retired instructions = 4. The test failed with a2 = 2
before forwarding was added and passed afterward.

[`next_block_prefix_stores_invalidate_the_watched_previous_page`](../../../esp32s3/src/bus/memory_origin_tests.rs)
covers stores at offsets 0, 1 and 2 in the next 64 KiB block after watching the last
page of the preceding block. Each case first publishes an unwatched mapping, then
checks both page versions and the stored byte. This exercises EX110 neighbor
marking and EX180 previous-page updates with the real bus.

[The real S3 WASM regressions](../../../wasm/src/jit_memory_tests.rs) execute generated
stores against `esp32s3::bus::SocBus`. Three scenarios check direct code watching,
both neighboring 64 KiB groups, existing unwatched TLB entries, previous-page
version updates and an unrelated mapping that must remain unwatched. Stores use
the DRAM alias while decode watches IRAM. Each checked store asserts that generated
WASM executed.

The AArch64 fixture now checks `ldrh w11, [x9, #20]`, matching the writable flag's
native offset. Apple's clang assembled this instruction to `7940292b`; llvm-objdump
reported `ldrh w11, [x9, #0x14]`. The hermetic fixture test passed.

Validation on the revision containing this receipt:

```sh
cargo test -p esp32s3 -p emu-core -p xtensa-lx7 -p esp-soc -p esp32c3 -p esp32c6 --lib --tests
cargo clippy -p emu-core -p esp-soc -p esp32s3 -p xtensa-lx7 -p esp32c3 -p esp32c6 --all-targets -- -D warnings
bash tools/wasm-jit-test.sh
node tools/check-evidence-privacy.mjs
```

Native tests: 331 passed, 3 ignored. The integrated WASM suite passed 81,274 cases
and released 83,917 compiled modules, including the three new real-S3 scenarios.
Clippy and the evidence privacy check passed.
The ignored external-assembler suite was not run; the changed instruction was
independently assembled as described above. These are correctness checks, with no
new performance measurement or renewed claim about the historical EX110 timings.
No personal environment captures or unredacted logs are included.
