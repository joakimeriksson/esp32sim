#!/bin/bash
# EX172 confirmation: crumbs-all versus crumbs-all + cleaned heads-s3.
# Finite, serial and resumable. Run through the M3 env.sh under nohup/caffeinate.
set -euo pipefail
ROOT="$HOME/bench/esp32sim/heads-confirm-0920"
OLD="$HOME/bench/esp32sim"
TREE="$OLD/x2/tree"
cd "$ROOT"
mkdir .runner.lock 2>/dev/null || { echo 'runner already active'; exit 1; }
trap 'rmdir "$ROOT/.runner.lock"' EXIT
mkdir -p runs logs
shasum -a 256 -c artifacts.sha256
while read -r name candidate pairs; do
  [ -n "$name" ] || continue
  [ -e "runs/$name/summary.json" ] && continue
  [ ! -e STOP ] || { echo 'STOP requested'; exit 0; }
  if [ -e "runs/$name" ]; then
    echo "Incomplete run $name exists; inspect it before retrying" >&2
    exit 1
  fi
  while pgrep -f 'run-pairs.py|capture-battery.mjs|cargo build|cargo test|Google Chrome.app/Contents/MacOS|Safari.app/Contents/MacOS' >/dev/null; do
    echo "$(date -u +%FT%TZ) browser, benchmark or build active; waiting"
    sleep 30
  done
  echo "$(date -u +%FT%TZ) START $name ($pairs pairs) load=$(sysctl -n vm.loadavg)"
  uv run --offline --no-project --python "$OLD/.venv/bin/python" python "$TREE/tools/browser-benchmark/run-pairs.py" \
    "$ROOT/runs/$name" --baseline-tree "$TREE" --candidate-tree "$TREE" \
    --baseline-wasm "$ROOT/wasm/baseline.wasm" --candidate-wasm "$ROOT/wasm/$candidate" \
    --assets "$OLD/assets/pocket-tank/assets.json" --pairs "$pairs" > "logs/$name.txt" 2>&1
  node -e 'const d=require(process.argv[1]);console.log(new Date().toISOString()+" DONE "+process.argv[2]+" "+JSON.stringify({reduction:d.wallReductionPercent,pairs:d.pairsWallReductionPercent}));' "$ROOT/runs/$name/summary.json" "$name"
done <<'JOBS'
control-before baseline.wasm 2
heads-on-crumbs candidate.wasm 4
control-after baseline.wasm 2
JOBS
touch DONE
echo "$(date -u +%FT%TZ) ALL DONE"
