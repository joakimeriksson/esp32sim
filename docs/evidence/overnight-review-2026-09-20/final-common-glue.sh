#!/bin/bash
set -euo pipefail
cd "$HOME/bench/esp32sim/overnight-review"
uv run --offline --no-project --python "$HOME/bench/esp32sim/.venv/bin/python" python candidate/tools/browser-benchmark/run-pairs.py runs/final-pocket-tank --baseline-tree final --candidate-tree final --baseline-wasm runs/failed-old-glue/candidate/main.wasm --candidate-wasm runs/final-pocket-tank-source-glue/candidate/main.wasm --assets "$HOME/bench/esp32sim/assets/pocket-tank/assets.json" --pairs 2 > logs/final-common-glue.txt 2>&1
