#!/bin/bash
# round x5 M3 runner. Serial, resumable, re-reads jobs.txt every loop (append/reorder while it runs).
# jobs.txt line:  <run-name> <candidate-wasm-file-in-wasm/> <pairs> [workload=pocket-tank]     ('#' = comment)
# Phase 1 (gate, batched 4 at a time): every wasm named in jobs.txt gets the strict 30 s exactness gate once
#   (gates/<wasm>.ok | gates/<wasm>.fail, output in logs/gate-<wasm>.txt). Failing candidates are never benched.
# Phase 2 (bench, strictly serial): run-pairs.py in Chrome. Done when runs/<run-name>/summary.json exists.
# `touch STOP` ends the loop after the current job.
set -uo pipefail
# Portable historical scheduler. Supply absolute paths; no remote target is assumed.
# X5_ROOT contains jobs.txt, wasm/, tree/, exact.mjs and exact-contract.mjs.
# Copy both exact gate modules from the evidence directory into X5_ROOT.
# X5_SHARED_ROOT contains assets/<workload>/assets.json and .venv/.
# Also supply TREE, FW_DIR and BASE_WASM for the portable exact.mjs gate.
: "${X5_ROOT:?supply an absolute campaign directory}"
: "${X5_SHARED_ROOT:?supply an absolute shared assets/runtime directory}"
: "${TREE:?source tree for exact.mjs}" "${FW_DIR:?firmware directory for exact.mjs}" "${BASE_WASM:?baseline artifact for exact.mjs}"
: "${TMPDIR:?supply a persistent scratch directory}"
export TMPDIR TMP="$TMPDIR" TEMP="$TMPDIR" TREE FW_DIR BASE_WASM
ROOT="$X5_ROOT"; OLD="$X5_SHARED_ROOT"
case "$ROOT:$OLD" in /*:/*) ;; *) echo "X5_ROOT and X5_SHARED_ROOT must be absolute" >&2; exit 1;; esac
cd "$ROOT" || exit 1
[ -d "$OLD/.venv" ] || uv venv "$OLD/.venv" || exit 1
mkdir .runner.lock 2>/dev/null || { echo "runner already active (.runner.lock)"; exit 1; }
trap 'rmdir "$ROOT/.runner.lock"' EXIT
mkdir -p runs logs gates
ungated() {
  while read -r name wasm pairs workload; do
    case "$name" in
      ''|'#'*) continue;;
    esac
    if [ -f "wasm/$wasm" ] && [ ! -e "gates/$wasm.ok" ] && [ ! -e "gates/$wasm.fail" ]; then echo "$wasm"; fi
  done < jobs.txt
}
while :; do
  [ -e STOP ] && { echo "$(date -u +%FT%TZ) STOP file present; exiting"; break; }
  # ---- phase 1: gate everything not gated yet
  togate=$(ungated | sort -u)
  if [ -n "$togate" ]; then
    echo "$(date -u +%FT%TZ) GATE batch: $(echo $togate | wc -w | tr -d ' ') candidates"
    echo "$togate" | xargs -P 4 -I{} /bin/bash -c 'w=$1; if node exact.mjs "wasm/$w" > "logs/gate-$w.txt" 2>&1; then touch "gates/$w.ok"; r="EXACT ok"; else touch "gates/$w.fail"; r="EXACT FAIL"; fi; echo "$(date -u +%FT%TZ) GATE $r $w"' _ {}
    continue
  fi
  # ---- phase 2: next bench job
  job=""
  while read -r name wasm pairs workload; do
    case "$name" in ''|'#'*) continue;; esac
    [ -e "runs/$name/summary.json" ] && continue
    [ -e "runs/$name.failed" ] && continue
    [ -e "gates/$wasm.fail" ] && { [ -e "runs/$name.failed" ] || { touch "runs/$name.failed"; echo "$(date -u +%FT%TZ) SKIP $name: exactness gate failed (logs/gate-$wasm.txt)"; }; continue; }
    [ -e "gates/$wasm.ok" ] || continue
    job="$name"; break
  done < jobs.txt
  if [ -z "$job" ]; then sleep 60; continue; fi
  workload="${workload:-pocket-tank}"
  while pgrep -f 'run-pairs.py|capture-battery.mjs|cargo build|cargo test' >/dev/null; do echo "foreign bench/build running; waiting"; sleep 30; done
  if pgrep -f 'Google Chrome.app/Contents/MacOS|Safari.app/Contents/MacOS' >/dev/null; then echo "$(date -u +%FT%TZ) interactive browser running; waiting"; sleep 60; continue; fi
  echo "$(date -u +%FT%TZ) START $name ($pairs pairs, $workload) load=$(sysctl -n vm.loadavg)"
  # Preserve incomplete outputs for later inspection rather than deleting them.
  if [ -e "runs/$name" ]; then
    prior="runs/$name.incomplete-$(date -u +%Y%m%dT%H%M%SZ)-$"
    [ ! -e "$prior" ] && mv "runs/$name" "$prior" || exit 1
  fi
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
