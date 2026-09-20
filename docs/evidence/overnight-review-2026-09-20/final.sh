#!/bin/bash
set -euo pipefail
export RUSTUP_TOOLCHAIN=1.98.1
cd "$HOME/bench/esp32sim/overnight-review"
(cd final; unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS; cargo build --release -p esp32sim --bin esp32sim) > logs/final-native-build.txt 2>&1
node final-native.mjs > logs/final-native.txt 2>&1
uv run --offline --no-project --python "$HOME/bench/esp32sim/.venv/bin/python" python candidate/tools/browser-benchmark/run-pairs.py runs/final-pocket-tank --baseline-tree candidate --candidate-tree final --baseline-wasm runs/failed-old-glue/candidate/main.wasm --candidate-rustflags=-Cllvm-args=-inline-threshold=4000 --assets "$HOME/bench/esp32sim/assets/pocket-tank/assets.json" --pairs 2 > logs/final-pocket-tank.txt 2>&1
touch FINAL_DONE
