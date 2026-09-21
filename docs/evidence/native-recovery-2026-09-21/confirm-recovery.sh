#!/bin/bash
set -euo pipefail
export RUSTUP_TOOLCHAIN=1.98.1
cd "$HOME/bench/esp32sim/overnight-review"
(cd recovery; unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS; cargo build --release -p esp32sim --bin esp32sim) > logs/recovery-build.txt 2>&1
node screen-native.mjs "$PWD/baseline/target/release/esp32sim" "$PWD/baseline/target/release/esp32sim" control-main-recovery 2 > logs/control-main-recovery.txt 2>&1
node screen-native.mjs "$PWD/baseline/target/release/esp32sim" "$PWD/recovery/target/release/esp32sim" confirm-main-recovery 4 > logs/confirm-main-recovery.txt 2>&1
touch NATIVE_RECOVERY_DONE
uv run --offline --no-project --python "$HOME/bench/esp32sim/.venv/bin/python" python candidate/tools/browser-benchmark/run-pairs.py runs/recovery-pocket-tank --baseline-tree recovery --candidate-tree recovery --baseline-wasm runs/final-pocket-tank-source-glue/candidate/main.wasm --candidate-rustflags=-Cllvm-args=-inline-threshold=4000 --assets "$HOME/bench/esp32sim/assets/pocket-tank/assets.json" --pairs 2 > logs/recovery-pocket-tank.txt 2>&1
uv run --offline --no-project --python "$HOME/bench/esp32sim/.venv/bin/python" python candidate/tools/browser-benchmark/run-pairs.py runs/recovery-tinydraw --baseline-tree recovery --candidate-tree recovery --baseline-wasm runs/final-pocket-tank-source-glue/candidate/main.wasm --candidate-wasm runs/recovery-pocket-tank/candidate/main.wasm --assets "$HOME/bench/esp32sim/assets/tinydraw/assets.json" --pairs 2 > logs/recovery-tinydraw.txt 2>&1
touch RECOVERY_DONE
