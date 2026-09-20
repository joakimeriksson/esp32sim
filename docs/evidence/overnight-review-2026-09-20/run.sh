#!/bin/bash
set -euo pipefail
export RUSTUP_TOOLCHAIN=1.98.1
cd "$HOME/bench/esp32sim/overnight-review"
ROOT="$PWD"
mkdir -p logs
system_profiler SPHardwareDataType | head -12 > host.txt
rustc -Vv > toolchain.txt
uv run --offline --no-project --python "$HOME/bench/esp32sim/.venv/bin/python" python candidate/tools/browser-benchmark/run-pairs.py runs/pocket-tank --baseline-tree candidate --candidate-tree candidate --baseline-wasm runs/failed-old-glue/baseline/main.wasm --candidate-wasm runs/failed-old-glue/candidate/main.wasm --assets "$HOME/bench/esp32sim/assets/pocket-tank/assets.json" --pairs 2 > logs/pocket-tank.txt 2>&1
uv run --offline --no-project --python "$HOME/bench/esp32sim/.venv/bin/python" python candidate/tools/browser-benchmark/run-pairs.py runs/tinydraw --baseline-tree candidate --candidate-tree candidate --baseline-wasm runs/pocket-tank/baseline/main.wasm --candidate-wasm runs/pocket-tank/candidate/main.wasm --assets "$HOME/bench/esp32sim/assets/tinydraw/assets.json" --pairs 2 > logs/tinydraw.txt 2>&1
touch BROWSER_DONE
