# Review fixes and regression coverage

This follow-up addresses the pre-merge findings against stack tip `5167baad`.
It adds correctness coverage and changes the compiler default using existing
EX154 evidence. It makes no new performance claim.

| Finding | Resolution |
| --- | --- |
| #86: peripheral alarm during virtual batching | SYSTIMER deadlines on both cores compare batched and single-round execution, including the handler's captured CCOUNT and final architectural state. Native CI already runs this suite with `ESP32SIM_VQ_NATIVE=1`. |
| #91: SPI2 DMA without a bound channel stays busy | Abort an unbound command with TRANS_DONE so a later CPU transfer succeeds. A stopped channel with a descriptor remains eligible for RESTART. Tests cover timing enabled and disabled, stopped-channel restart and protection of a parked transfer. |
| #97: symbol names evade the ELF copy limit | Charge names before allocation, including both symbol maps, lossy UTF-8 expansion and a per-record allowance. Repeated records consume the same 256 MiB budget as segment/section payloads. A directed fixture checks duplicate names and invalid UTF-8. |
| #99: UDP counters never appear in output | The normal SoC report includes UDP evictions and send errors alongside flow counts. |
| #114: inline threshold choice | Set the shared production/test default to 2000; retain explicit RUSTFLAGS overrides. [Historical size and speed basis](../inline-size-review-2026-09-20/README.md#september-21-selection). |
| #118: FMA fallback absent from CI | Two directed generated-WASM cases force the halfway fallback for MADD.S and MSUB.S, count helper calls and distinguish its result from incorrect double rounding. The ordinary differential suite runs them. This does not change the known subnormal libm/hardware discrepancy in EX170. |
| #120: arrival inside a three-byte instruction | Execute RFE to the instruction boundary, each interior byte and the next boundary. Compare fresh interpreter decoding to compiled execution; only actual boundaries may reuse the owner. |

Evidence redactions retain numeric measurements and artifact identities. The
[manifest](../privacy-review-2026-09-21/redactions.json) records 205 sanitized files,
including three compressed profiles. The [retention policy](../README.md) and
tracked-evidence CI check cover future captures. Historical hashes identify the
original captures; sanitized hashes identify the retained files.

## Validation

Run on macOS with the installed Rust toolchain and Node. These are correctness
checks, not firmware throughput measurements:

- `cargo test --release -p esp32s3 --lib -p esp-soc -p esp-periph`: 178 passed.
- `cargo test --release -p esp-soc --test parsers`: 12 passed.
- `ESP32SIM_VQ_NATIVE=1 cargo test --release -p esp32s3 --test virtual_stops`: 5 passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `tools/wasm-jit-test.sh` with the 2000 default: 79,631 differential cases; 82,232 compiled modules released.
- Shared flag policy checked with unset, empty and explicit RUSTFLAGS.

The native commands used an external Cargo target directory; no source overrides
or alternate compiler flags were supplied. The final privacy and provenance
checks verify retained JSON/gzip readability and sanitized-file hashes.

Standalone owning-PR checks also passed: #86's native-opt-in virtual scheduler
suite (3 tests) and #91's SPI2-filtered unit suite (3 tests). The alarm test uses
only APIs present at #86, while retaining the handler CCOUNT comparison.

Final assembled-tree checks verified all 205 sanitized-file hashes, parsed 490
tracked evidence JSON files and decompressed 15 gzip files. The privacy pattern
check passed across 1,144 tracked evidence files. All 30 PR branches retain their
previous published heads as ancestors and contain their updated dependencies.
