#!/bin/bash
set -euo pipefail
cd "$HOME/bench/esp32sim/overnight-review"
for n in {1..240}; do [ ! -f RECOVERY_DONE ] || break; sleep 10; done
[ -f RECOVERY_DONE ]
node -e 'const d=require("./runs/confirm-main-recovery/summary.json");if(d.medianWallSeconds.candidate>d.medianWallSeconds.baseline)throw Error("Native median exceeds main; pause direct browser follow-up")'
uv run --offline --no-project --python "$HOME/bench/esp32sim/.venv/bin/python" python candidate/tools/browser-benchmark/run-pairs.py runs/main-recovery-pocket-tank --baseline-tree recovery --candidate-tree recovery --baseline-wasm runs/failed-old-glue/baseline/main.wasm --candidate-wasm runs/recovery-pocket-tank/candidate/main.wasm --assets "$HOME/bench/esp32sim/assets/pocket-tank/assets.json" --pairs 2 > logs/main-recovery-pocket-tank.txt 2>&1
uv run --offline --no-project --python "$HOME/bench/esp32sim/.venv/bin/python" python candidate/tools/browser-benchmark/run-pairs.py runs/main-recovery-tinydraw --baseline-tree recovery --candidate-tree recovery --baseline-wasm runs/failed-old-glue/baseline/main.wasm --candidate-wasm runs/recovery-pocket-tank/candidate/main.wasm --assets "$HOME/bench/esp32sim/assets/tinydraw/assets.json" --pairs 2 > logs/main-recovery-tinydraw.txt 2>&1
touch DIRECT_MAIN_BROWSER_DONE
