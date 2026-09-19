#!/bin/bash
# usage: pair.sh <name> <candidate.wasm> [pairs] [baseline.wasm]  -> timed alternating pairs, prints summary
R=/Users/sarah/src/a/esp32sim/work/night-run; T=/Users/sarah/src/a/esp32sim/work/night
N=$1; C=$(cd "$(dirname "$2")" && pwd)/$(basename "$2"); P=${3:-1}; B=${4:-$R/wasm/base.wasm}
touch $R/QUIET
cd $T && python3 tools/browser-benchmark/run-pairs.py $R/runs/$N --baseline-tree . --candidate-tree . --baseline-wasm $B --candidate-wasm $C --assets ${ASSETS:-$R/assets.json} --pairs $P > $R/runs/$N.log 2>&1
rc=$?
rm -f $R/QUIET
python3 - $R/runs/$N/summary.json <<'P'
import json,sys
try:
    d=json.load(open(sys.argv[1]))
    for r in d['runs']: print(r['pair'], r['arm'][:4], round(r['wallSeconds'],2), r['instructions'], r['passed'], r['consoleSha256'][:8], r['jitBytes'])
    print('median', d['medianWallSeconds'], 'reduction%', round(d['wallReductionPercent'],2))
except Exception as e: print('FAILED', e)
P
[ $rc -ne 0 ] && tail -5 $R/runs/$N.log
exit 0
