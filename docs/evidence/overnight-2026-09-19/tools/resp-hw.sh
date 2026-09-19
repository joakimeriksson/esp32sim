#!/bin/bash
# usage: prof.sh <wasm> <outdir>   (CPU profile of the battery in headless Chrome)
set -e
W=$(cd "$(dirname "$1")" && pwd)/$(basename "$1"); OUT=$2
T=/Users/alice/src/a/esp32sim/work/night-timed
R=/Users/alice/src/a/esp32sim/work/night-run
mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)
uv run --no-project --python /Users/alice/src/a/esp32sim/.venv/bin/python - "$W" "$OUT" <<'P'
import json,sys
a=json.load(open('/Users/alice/src/a/esp32sim/work/night-run/assets.json')); a['wasm']=sys.argv[1]
json.dump(a,open(sys.argv[2]+'/assets.json','w'))
P
cd $T
uv run --no-project --python /Users/alice/src/a/esp32sim/.venv/bin/python tools/browser-benchmark/serve.py "$OUT/assets.json" --port 8797 > "$OUT/server.log" 2>&1 & S=$!
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --no-first-run --no-default-browser-check --disable-background-timer-throttling --disable-renderer-backgrounding --remote-debugging-address=127.0.0.1 --remote-debugging-port=9233 --user-data-dir="$OUT/chrome-profile" about:blank > "$OUT/chrome.log" 2>&1 & C=$!
trap "kill $S $C 2>/dev/null; sleep 1; rm -rf '$OUT/chrome-profile'" EXIT
sleep 3
node tools/browser-benchmark/capture-response.mjs http://127.0.0.1:8797/response.html?timing=hw "$OUT" 9233 > "$OUT/capture.log" 2>&1
