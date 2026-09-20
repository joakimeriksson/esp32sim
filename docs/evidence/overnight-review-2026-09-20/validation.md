# Validation of the integrated review fixes

The fixes were integrated above pre-review stack tip `661623ea`. Runtime source is `946497a3`; documentation-only changes follow it. Full native tests built `cb2ec043`, before the equivalent RMT Clippy syntax cleanup and comments in `946497a3`. Final production and profiling WASM builds include that cleanup. Local path labels in the retained logs are normalized.

| Check | Result | Receipt |
| --- | --- | --- |
| Native release workspace, including ignored tests except external fixtures | 437 passed, including all 14 golden tests and AArch64 JIT memory tests | [native log](validation/native.log) |
| Native VQ enabled, machine and virtual-stop suites | 30 + 4 passed | [VQ log](validation/vq.log) |
| Workspace all-target Clippy, warnings denied | Passed | [Clippy log](validation/clippy.log) |
| WASM differentials using the shipped inline threshold | 79,628 passed, 82,222 modules released | [WASM log](validation/wasm.log) |
| WASM differentials with jit-tests,jit-profile,cpu-profile | 79,628 passed, 82,222 modules released | [profile log](validation/profile.log) |
| Production WASM timing ABI and firmware smoke tests | Passed: JIT handoff, hello, C3 hello, C6 hello/energy/Contiki, two network fixtures and panel | [production log](validation/production.log) |
| Browser benchmark/touch Node tests | 21 passed | [JS log](validation/js.log) |
| Worker pacing, frame credits, setup failures and display opt-in | Passed | [worker and pacing log](validation/js.log) |
| Browser benchmark Python unit tests, uv virtual environment | 12 passed | [Python log](validation/python.log) |

Commands match the CI workflow, with local ROMs supplied through `ESP32SIM_ROM_DIR`. The profiling command used `cargo build --release --target wasm32-unknown-unknown -p esp32sim-wasm --features jit-tests,jit-profile,cpu-profile` followed by `node tools/wasm-jit-test.mjs`; `tools/wasm-rustflags.sh` supplied the same default threshold as production. Native VQ used `ESP32SIM_VQ_NATIVE=1`. These local results do not claim that unmodified lower PR heads were independently retested.

Benchmark speed, work/output equality and measurement limits are in the [M3 report](README.md). Review dispositions and unresolved qualification requests are in [reviews.md](reviews.md).

The integrated native CLI also completed an ESP-IDF WPA2 join/DHCP and a controlled Linux-on-S3 TCP download of an exact 27-byte marker. See [firmware commands, input hashes and limits](firmware-qualification/result.txt). Separate lwIP application traffic and loss/retransmission stress are not established by that smoke test.

Two pre-existing test-harness Clippy warnings were corrected at `25030721` without changing runtime code. The combined WASM test/profiling configuration now passes warnings-as-errors [Clippy](validation/profile-clippy.log), and all 79,628 differential cases passed again on that final test source ([log](validation/profile-final.log)).
