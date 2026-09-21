#!/bin/sh
# Build the WebAssembly emulator and put it where web/run.html?wasm expects it.
#   tools/wasm-build.sh            -> web/wasm/esp32sim.wasm
#   python3 -m http.server -d web 8790 ; open http://127.0.0.1:8790/run.html?wasm
set -e
cd "$(dirname "$0")/.."
rustup target list --installed | grep -q wasm32-unknown-unknown || rustup target add wasm32-unknown-unknown
# a non-rustup cargo/rustc on PATH (Homebrew) has no wasm32 std: build with rustup's toolchain
RUSTC="$(rustup which rustc)"; export RUSTC
CARGO="$(rustup which cargo)"
. ./tools/wasm-rustflags.sh
# rust-lld looks for libLLVM.dylib next to itself; the toolchain keeps it in lib/
DYLD_FALLBACK_LIBRARY_PATH="$(dirname "$(dirname "$RUSTC")")/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"; export DYLD_FALLBACK_LIBRARY_PATH
"$CARGO" build --release --target wasm32-unknown-unknown -p esp32sim-wasm
cp target/wasm32-unknown-unknown/release/esp32sim_wasm.wasm web/wasm/esp32sim.wasm
# Keep the measured artifact by default; Binaryen is a separate experiment.
if [ "${WASM_OPT:-0}" = 1 ]; then wasm-opt -O3 -o web/wasm/esp32sim.wasm web/wasm/esp32sim.wasm; fi
ls -la web/wasm/esp32sim.wasm
