#!/bin/bash
set -euo pipefail
export RUSTUP_TOOLCHAIN=1.98.1
cd "$HOME/bench/esp32sim/overnight-review"
for n in {1..180}; do [ ! -f BROWSER_DONE ] || break; sleep 10; done
[ -f BROWSER_DONE ]
for arm in baseline candidate; do
 (cd "$arm"; unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS; cargo build --release -p esp32sim --bin esp32sim) > "logs/native-$arm-build.txt" 2>&1
done
node native.mjs > logs/native.txt 2>&1
touch NATIVE_DONE
