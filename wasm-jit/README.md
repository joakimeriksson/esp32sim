# ESP32-S3 WebAssembly JIT

This first slice compiles the seven-instruction TinyDraw SRAM kernel into a
self-contained WebAssembly module. Instruction costs come from the same
receipt-backed `Esp32S3SramCostModel` used by the interpreter.

The compiler currently accepts only straight-line `l32i`, `l32i.n`, `movi.n`,
`memw`, `sub` and `saltu` blocks whose data accesses are explicitly SRAM. It
returns a named error for every other instruction or timing class.

The integration test runs the emitted module under Node and compares all 16
address registers, the program counter, the cycle total and SRAM bytes with
the interpreter result:

```sh
cargo test -p esp32sim-wasm-jit
```

Measure the emitted kernel under Node with a fixed five-sample harness:

```sh
cargo run --release -p esp32sim-wasm-jit --example sram_kernel_speed
```

The committed checkpoint receipt is in
`docs/evidence/wasm-jit-sram-kernel-2026-09-04`.

The browser exposes this compiler through the external
[`esp32sim_jit_prepare` / `esp32sim_jit_commit` handoff](../wasm/src/browser_jit.rs).
Generated modules import the emulator's memory and write results to a handoff
record; commit validates the result before updating architectural state.

The browser's normal block dispatcher uses the separate
[LX7 WebAssembly JIT](../xtensa-lx7/src/jit/wasm.rs), installed by
[`web/wasm/jit.mjs`](../web/wasm/jit.mjs). This crate remains the limited
receipt-priced SRAM compiler and equivalence checkpoint.
