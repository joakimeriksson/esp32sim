#!/bin/bash
set -euo pipefail
ROOT=/Users/sarah/src/a/esp32sim
OUT="$ROOT/work/night-run/pocket-combined3-profile"
mkdir -p "$OUT"
node - "$ROOT" "$OUT" <<'NODE'
const fs=require('fs'),path=require('path');
const [root,out]=process.argv.slice(2);
const a=JSON.parse(fs.readFileSync(path.join(root,'web/wasm/fw/local/pocket-tank-assets.json'),'utf8'));
a.wasm=path.join(root,'work/night-run/wasm/combined3-cpuprof.wasm');
fs.writeFileSync(path.join(out,'assets.json'),JSON.stringify(a,null,2));
NODE
if [ ! -d "$ROOT/.venv" ]; then uv venv "$ROOT/.venv"; fi
cd "$ROOT"
uv run --no-project --python "$ROOT/.venv/bin/python" tools/browser-benchmark/serve.py "$OUT/assets.json" --port 8797 > "$OUT/server.log" 2>&1 & S=$!
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --no-first-run --no-default-browser-check --disable-background-timer-throttling --disable-renderer-backgrounding --remote-debugging-address=127.0.0.1 --remote-debugging-port=9233 --user-data-dir="$OUT/chrome-profile" about:blank > "$OUT/chrome.log" 2>&1 & C=$!
cleanup() { kill "$S" "$C" 2>/dev/null || true; wait "$S" "$C" 2>/dev/null || true; }
trap cleanup EXIT
sleep 3
node tools/browser-benchmark/capture-cpu.mjs http://127.0.0.1:8797/battery.html "$OUT" 9233 > "$OUT/capture.log" 2>&1
uv run --no-project --python "$ROOT/.venv/bin/python" tools/browser-benchmark/summarize-cpu.py "$OUT/battery.cpuprofile" > "$OUT/cpu-summary.txt"
head -45 "$OUT/cpu-summary.txt"
