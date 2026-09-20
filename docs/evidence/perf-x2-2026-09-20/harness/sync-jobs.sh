#!/bin/bash
# Run on the LAPTOP: push newly queued candidates (queue/jobs.jsonl) to the M3 and append them to its jobs.txt.
# Usage: m3/sync-jobs.sh [pairs=2]      Idempotent. New jobs go BEFORE the tier-3 section so they run ahead of it.
set -euo pipefail
cd "$(dirname "$0")"; P=${1:-2}; H=${M3_HOST:-alice@alice-m3p.local}; S="ssh -o BatchMode=yes -o ConnectTimeout=8 -o HostKeyAlias=alice-m3p.local"
[[ "$P" =~ ^[1-9][0-9]*$ ]] || { echo "pairs must be a positive integer" >&2; exit 1; }
[ -d ../.venv ] || uv venv ../.venv
$S $H 'cat ~/bench/esp32sim/x2/jobs.txt' > jobs.remote.txt
uv run --offline --no-project --python ../.venv/bin/python python - "$P" <<'PY'
import hashlib,json,sys
from pathlib import Path
have={l.split()[0] for l in open('jobs.remote.txt') if l.strip() and not l.startswith('#')}
new=[json.loads(l) for l in open('../queue/jobs.jsonl')]; new=[d for d in new if d['name'] not in have]
open('new-wasm.txt','w').write(''.join(d['candidate']+'\n' for d in new))
checks=[]
bases={hashlib.sha256(Path(d['baseline']).read_bytes()).hexdigest() for d in new}
if len(bases)>1: raise SystemExit('New jobs do not share one baseline')
if bases: checks.append(f'{next(iter(bases))}  wasm/base.wasm\n')
for d in new:
    p=Path(d['candidate']); digest=hashlib.sha256(p.read_bytes()).hexdigest()
    if p.stem != digest[:12]: raise SystemExit(f'Frozen hash mismatch: {p}')
    checks.append(f'{digest}  wasm/{p.name}\n')
open('new-wasm.sha256','w').writelines(checks)
lines=open('jobs.remote.txt').read().splitlines()
add=[f"{d['name']} {d['candidate'].split('/')[-1]} {sys.argv[1]}" for d in new]
i=next((k for k,l in enumerate(lines) if l.startswith('# tier 3')),len(lines))
open('jobs.next.txt','w').write('\n'.join(lines[:i]+add+lines[i:])+'\n'); print('new jobs:',[d['name'] for d in new])
PY
[ -s new-wasm.txt ] || { echo "nothing new"; exit 0; }
rsync -a -e "$S" $(cat new-wasm.txt) $H:bench/esp32sim/x2/wasm/
rsync -a -e "$S" new-wasm.sha256 $H:bench/esp32sim/x2/new-wasm.sha256
$S $H 'cd ~/bench/esp32sim/x2 && shasum -a 256 -c new-wasm.sha256'
rsync -a -e "$S" ../queue/jobs.jsonl $H:bench/esp32sim/x2/jobs-meta.jsonl
rsync -a -e "$S" jobs.next.txt $H:bench/esp32sim/x2/jobs.txt.new && $S $H 'cd ~/bench/esp32sim/x2 && mv jobs.txt.new jobs.txt && grep -vc "^#" jobs.txt'
