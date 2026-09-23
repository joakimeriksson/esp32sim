#!/usr/bin/env python3
"""Build the compact x6/x7 results.json from M3 summaries, per-arm result.json captures,
the lab queue and the runner log. Paths, hostnames and debugger URLs are not copied.

Usage: curate.py <out-dir> <lab-root> <m3-copy>
  lab-root: x6 lab (queue/jobs.jsonl, queue/wasm/, results/safari-m3-1/)
  m3-copy:  runner.log plus runs/<job>/{summary.json,<pair>-<arm>/result.json} copied from the M3
"""
import json, os, re, statistics, sys, hashlib
OUT, X, S = sys.argv[1:4]

BASES = {  # frozen artifact prefix -> label, source revision
    'fbb189ac536e': ('base', 'aa353cf3ec48d2ed3e20c27ee7fad617a9ec81d7'),
    'f774ef8b0476': ('base-motion', '067d0e94'),
    '3f5f387002c3': ('base7', 'ff384f5e'),
    '4d9f8fdb7fb5': ('base-motion2', '1f4c247d'),
}
CONTRACTS = {10073833775: 'pocket-30s', 9819885134: 'tinydraw-battery',
             13247635412: 'fluidbox-still', 13329744922: 'fluidbox-motion-v1',
             13335545195: 'fluidbox-motion-v2'}

jobs = {}
for l in open(f'{X}/queue/jobs.jsonl'):
    d = json.loads(l); jobs[d['name']] = d

starts, fails = {}, {}
for l in open(f'{S}/runner.log'):
    m = re.match(r'(\S+) START (\S+) \((\d+) pairs, (\S+)\) load=\{ ([\d.]+) ([\d.]+) ([\d.]+) \}', l)
    if m: starts[m[2]] = (m[1], [float(m[5]), float(m[6]), float(m[7])])
    m = re.match(r'(\S+) FAILED (\S+) ', l)
    if m: fails[m[2]] = m[1]

def pct(b, c): return 100 * (1 - c / b)

out_jobs, problems = [], []
FULL = {}
names = sorted(os.listdir(f'{S}/runs'), key=lambda n: starts.get(n, ('9',))[0])
for n in names:
    sp = f'{S}/runs/{n}/summary.json'
    if not os.path.exists(sp):
        continue
    s = json.load(open(sp)); j = jobs.get(n, {})
    arms, fr, insns, cons, bytes_, checks_ok = [], set(), set(), set(), [], True
    wasm = {}
    for r in s['runs']:
        rp = f"{S}/runs/{n}/{r['pair']}-{r['arm']}/result.json"
        a = json.load(open(rp))['result']
        if abs(a['wallSeconds'] - r['wallSeconds']) > 1e-9: problems.append((n, 'wall mismatch'))
        if a['instructions'] != r['instructions']: problems.append((n, 'insn mismatch'))
        wasm[r['arm']] = r['provenance']['sha256']['asset/wasm']
        fr.add(a['frames']); insns.add(a['instructions']); cons.add(r['consoleSha256'])
        ok = a['passed'] and a['status'] == 'completed' and a['jit']['failed'] == 0 and (all(
            c['count'] >= c['min'] for c in a['checks']) if a['checks'] is not None else True) and (
            a['verdictValidation'] is None or a['verdictValidation']['passed'])
        checks_ok &= ok
        arms.append([r['pair'], r['arm'][0], r['wallSeconds'], a['frames'], a['jit']['bytes'],
                     round(a['jit']['compileMs'], 1)])
    # recompute
    med = {k: statistics.median([x[2] for x in arms if x[1] == k[0]]) for k in ('baseline', 'candidate')}
    red = pct(med['baseline'], med['candidate'])
    pairs = []
    for p in range(1, s['pairs'] + 1):
        b = [x[2] for x in arms if x[0] == p and x[1] == 'b'][0]
        c = [x[2] for x in arms if x[0] == p and x[1] == 'c'][0]
        pairs.append(pct(b, c))
    if abs(red - s['wallReductionPercent']) > 1e-9: problems.append((n, 'median red mismatch'))
    if any(abs(a - b) > 1e-9 for a, b in zip(pairs, s['pairsWallReductionPercent'])): problems.append((n, 'pair mismatch'))
    if len(insns) != 1 or len(cons) != 1 or len(fr) != 1: problems.append((n, f'work differs {insns} {fr} {len(cons)}'))
    bl = BASES.get(wasm['baseline'][:12], (None, None))[0]
    FULL.setdefault(wasm['baseline'][:12], set()).add(wasm['baseline'])
    ins = next(iter(insns))
    st = starts.get(n, (None, None))
    out_jobs.append({
        'name': n,
        'workload': {'pocket-tank': 'pocket', 'tinydraw': 'tinydraw', 'fluidbox': 'fluidbox'}[a['workload']],
        'contract': CONTRACTS.get(ins, 'unknown'),
        'agent': j.get('agent') or 'coordinator',
        'sourceRevision': j.get('rev') or 'aa353cf3ec48d2ed3e20c27ee7fad617a9ec81d7',
        'candidateTree': bool(j.get('tree')),
        'baseline': bl or wasm['baseline'][:12],
        'candidateWasmSha256': wasm['candidate'],
        'startedAt': st[0], 'loadAtStart': st[1],
        'pairs': s['pairs'],
        'medianWallSeconds': s['medianWallSeconds'],
        'wallReductionPercent': round(s['wallReductionPercent'], 4),
        'pairsWallReductionPercent': [round(x, 4) for x in s['pairsWallReductionPercent']],
        'instructions': ins, 'frames': next(iter(fr)), 'consoleSha256': next(iter(cons)),
        'allArmsPass': checks_ok,
        'arms': arms,
    })

# Safari on the M3
SAF = f'{X}/results/safari-m3-1/results-20260922T231854Z'
saf = []
for n in ['warmup-fluidbox', 'fluidbox-base7', 'fluidbox-jsc', 'pocket-base7', 'pocket-jsc']:
    s = json.load(open(f'{SAF}/{n}/summary.json')); runs = json.load(open(f'{SAF}/{n}/runs.json'))
    arms = [[r['pair'], r['arm'][0], r['wallSeconds'], r['jitBytes']] for r in runs]
    ok = all(r['passed'] and r['jitFailed'] == 0 for r in runs)
    insns = {r['instructions'] for r in runs}; cons = {r['consoleSha256'] for r in runs}
    wb = {r['arm']: r['provenance']['sha256']['asset/wasm'] for r in runs}
    med = {k: statistics.median([x[2] for x in arms if x[1] == k[0]]) for k in ('baseline', 'candidate')}
    if abs(pct(med['baseline'], med['candidate']) - s['wallReductionPercent']) > 1e-9: problems.append((n, 'safari red'))
    if len(insns) != 1 or len(cons) != 1: problems.append((n, 'safari work differs'))
    saf.append({'name': n, 'browser': runs[0]['browser'], 'engine': runs[0].get('engine'),
                'baseline': BASES.get(wb['baseline'][:12], (wb['baseline'][:12],))[0],
                'baselineWasmSha256': wb['baseline'], 'candidateWasmSha256': wb['candidate'],
                'pairs': s['pairs'], 'medianWallSeconds': s['medianWallSeconds'],
                'wallReductionPercent': s['wallReductionPercent'],
                'pairsWallReductionPercent': s['pairsWallReductionPercent'],
                'instructions': next(iter(insns)), 'contract': CONTRACTS.get(next(iter(insns))),
                'consoleSha256': next(iter(cons)), 'allArmsPass': ok, 'arms': arms})

for k in BASES:
    if k not in FULL:
        FULL[k] = {hashlib.sha256(open(f'{X}/queue/wasm/{k}.wasm', 'rb').read()).hexdigest()}
if any(len(v) != 1 for v in FULL.values()) or any(k not in BASES for k in FULL): problems.append(('baselines', FULL))
doc = {
    'schema': 'esp32sim-perf-x6-x7-v1',
    'note': 'Positive reduction = less wall time. wallReductionPercent = 100*(1-median(candidate)/median(baseline)). '
            'arms: [pair, b|c, wallSeconds, frames, jitBytes, jitCompileMs] in execution order (Safari: [pair, b|c, wallSeconds, jitBytes]). '
            'Instructions, frames and console hash are equal across every arm of a job (checked when generated). '
            'Agent, start time and load averages per job are in the arm files. Frames and JIT counters come from each arm\'s '
            'capture; raw event logs, Chrome logs and host paths are omitted.',
    'chrome': {'browser': 'Chrome/153.0.8010.53', 'v8': '15.3.76.13', 'mode': 'headless, fresh process per arm'},
    'baselines': {v[0]: {'wasmSha256': sorted(FULL[k])[0], 'sourceRevision': v[1]} for k, v in BASES.items()},
    'contracts': {v: k for k, v in CONTRACTS.items()},
    'excluded': [
        {'name': 'shell-s1-fl, shell-s2, shell-s3r-fl', 'at': '2026-09-22T20:04:07Z/20:08:52Z',
         'reason': 'runner exactness gate reported FAIL within the same second; the identical frozen artifacts passed the gate at 20:14:42Z and were timed then. Gate logs were not retrieved.'},
        {'name': 'atom-s1 (pocket)', 'at': fails.get('atom-s1'),
         'reason': 'harness refused the campaign: "baseline inputs changed within campaign" (M3 tree workloads.json re-pinned for fluidbox-motion-v2 while the job ran); no timing retained; job then pruned.'},
        {'name': 'safari results-20260922T231311Z', 'reason': 'first Safari session stopped at its first warm-up arm: "expected a headless Chrome timing capture" (adapter error); rerun as results-20260922T231854Z.'},
    ],
    'jobs': out_jobs,
    'safariM3': {'session': 'results-20260922T231854Z', 'browser': 'Safari/27.0 (JavaScriptCore), visible page, fresh page and worker per arm',
                 'jobs': saf},
}
if problems:
    print('PROBLEMS', problems, file=sys.stderr); sys.exit(1)
def dump(obj, path, key='{"name"'):
    t = json.dumps(obj, separators=(',', ':')).replace(key, '\n' + key)
    open(path, 'w').write(t + '\n'); return os.path.getsize(path)
X6 = {'base', 'base-motion'}
armfields = ('agent', 'startedAt', 'loadAtStart', 'consoleSha256', 'arms')
runs = {'x6': [], 'x7': [], 'final': []}
for j in out_jobs:
    rnd = ('final' if j['name'].startswith('final') or j['name'] == 'control-aa-fl2'
           else 'x6' if j['baseline'] in X6 or j['name'] in ('control-aa-1', 'control-aa-2', 'control-aa-td') else 'x7')
    j['round'] = rnd
    runs[rnd].append({'name': j['name'], **{k: j.pop(k) for k in armfields}})
    if not j['candidateTree']: del j['candidateTree']
os.makedirs(os.path.join(OUT, 'arms'), exist_ok=True)
hdr = {'note': 'arms: [pair, b|c, wallSeconds, frames, jitBytes, jitCompileMs] in execution order. agent: lab worker that queued the job; startedAt and loadAtStart: 1/5/15-minute load averages, from the runner log. Index and summaries: ../results.json.'}
sizes = {r: dump({**hdr, 'jobs': v}, os.path.join(OUT, 'arms', f'{r}.json')) for r, v in runs.items()}
sizes['safari'] = dump(doc.pop('safariM3'), os.path.join(OUT, 'arms', 'safari-m3.json'))
doc['armFiles'] = ['arms/x6.json', 'arms/x7.json', 'arms/final.json', 'arms/safari-m3.json']
doc['note'] = doc['note'].replace("arms: [pair, b|c, wallSeconds, frames, jitBytes, jitCompileMs] in execution order (Safari: [pair, b|c, wallSeconds, jitBytes]). ", "Per-arm samples are in armFiles. ")
sizes['index'] = dump(doc, os.path.join(OUT, 'results.json'))
print(len(out_jobs), 'jobs', sum(len(r['arms']) for v in runs.values() for r in v), 'arms', sizes)
