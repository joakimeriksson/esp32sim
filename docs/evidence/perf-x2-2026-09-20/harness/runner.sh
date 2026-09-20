#!/bin/bash
# Round-2 M3 bench runner. Serial, resumable, re-reads jobs.txt every loop (append/reorder while it runs).
# jobs.txt line:  <run-name> <candidate-wasm-file-in-wasm/> <pairs> [workload=pocket-tank]     ('#' = comment)
# A job is done when runs/<run-name>/summary.json exists. `touch STOP` ends the loop after the current job.
set -uo pipefail
ROOT="$HOME/bench/esp32sim/x2"; OLD="$HOME/bench/esp32sim"
cd "$ROOT"
mkdir .runner.lock 2>/dev/null || { echo "runner already active (.runner.lock)"; exit 1; }
trap 'rmdir "$ROOT/.runner.lock"' EXIT
mkdir -p runs logs
while :; do
  [ -e STOP ] && { echo "$(date -u +%FT%TZ) STOP file present; exiting"; break; }
  job=""
  while read -r name wasm pairs workload; do
    case "$name" in ''|'#'*) continue;; esac
    [ -e "runs/$name/summary.json" ] && continue
    [ -e "runs/$name.failed" ] && continue
    job="$name"; break
  done < jobs.txt
  if [ -z "$job" ]; then sleep 60; continue; fi
  workload="${workload:-pocket-tank}"
  if [ ! -f "wasm/$wasm" ]; then echo "$(date -u +%FT%TZ) $name: wasm/$wasm missing; waiting"; sleep 60; continue; fi
  while pgrep -f 'run-pairs.py|capture-battery.mjs|cargo build|cargo test' >/dev/null; do echo "foreign bench/build running; waiting"; sleep 30; done
  if pgrep -f 'Google Chrome.app/Contents/MacOS|Safari.app/Contents/MacOS' >/dev/null; then echo "$(date -u +%FT%TZ) interactive browser running; waiting"; sleep 60; continue; fi
  echo "$(date -u +%FT%TZ) START $name ($pairs pairs, $workload) load=$(sysctl -n vm.loadavg)"
  rm -rf "runs/$name"
  if uv run --offline --no-project --python "$OLD/.venv/bin/python" python tree/tools/browser-benchmark/run-pairs.py \
      "$ROOT/runs/$name" --baseline-tree "$ROOT/tree" --candidate-tree "$ROOT/tree" \
      --baseline-wasm "$ROOT/wasm/base.wasm" --candidate-wasm "$ROOT/wasm/$wasm" \
      --assets "$OLD/assets/$workload/assets.json" --pairs "$pairs" > "logs/$name.log" 2>&1 \
     && [ -e "runs/$name/summary.json" ]; then
    node -e 'const d=require(process.argv[1]); console.log(new Date().toISOString()+" DONE "+process.argv[2]+" "+JSON.stringify({reduction:d.wallReductionPercent,pairs:d.pairsWallReductionPercent}));' "$ROOT/runs/$name/summary.json" "$name"
  else
    echo "$(date -u +%FT%TZ) FAILED $name (see logs/$name.log)"; tail -3 "logs/$name.log"; touch "runs/$name.failed"
  fi
done
