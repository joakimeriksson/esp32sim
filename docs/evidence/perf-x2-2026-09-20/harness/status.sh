#!/bin/bash
# Pull round-2 M3 results and print the table. Usage: m3/status.sh
set -euo pipefail
cd "$(dirname "$0")"
H=${M3_HOST:-alice@alice-m3p.local}
[ -d ../.venv ] || uv venv ../.venv
mkdir -p results
rsync -a -e "ssh -o BatchMode=yes -o ConnectTimeout=8 -o HostKeyAlias=alice-m3p.local" --include='*/' --include='summary.json' --exclude='*' "$H":bench/esp32sim/x2/runs/ results/runs/
rsync -a -e "ssh -o BatchMode=yes -o ConnectTimeout=8 -o HostKeyAlias=alice-m3p.local" "$H":bench/esp32sim/x2/runner.log results/
uv run --offline --no-project --python ../.venv/bin/python python - <<'P'
import json,glob,os
rows=[]
for f in sorted(glob.glob('results/runs/*/summary.json'), key=os.path.getmtime):
    d=json.load(open(f)); rows.append((os.path.basename(os.path.dirname(f)), d.get('wallReductionPercent'), d.get('pairsWallReductionPercent')))
print(f"{'run':28} {'median red.%':>12}  pairs")
for n,r,p in rows: print(f"{n:28} {r:12.2f}  {', '.join(f'{x:.2f}' for x in p)}")
P
tail -3 results/runner.log
