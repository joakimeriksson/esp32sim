#!/usr/bin/env python3
"""Build the x8 evidence files from the lab queue, M3 summaries, per-arm result.json captures, the runner
log, the q64/q256 validation arms, the native screens and the lab CPU profiles. Host names, login names,
home paths, debugger URLs, serial logs, Chrome logs and raw profiles are not copied.

Usage: curate.py <out-dir> <lab-root> <m3-copy> <native-copy>
  lab-root:    x8 lab (BRIEF.md, tasks/, notes/, queue/jobs.jsonl, results/)
  m3-copy:     from the M3 runner dir: runner.log, jobs.txt, runs/<job>/{summary.json,runs.json,
               <pair>-<arm>/result.json}, qv/<arm>/{result.json,events.json}, qt/<q>/{result.json,events.json},
               prof/<name>/result.json, m3-prof*.py
  native-copy: runs/<screen>/{summary.json,runs.json} and tools/screen-*.mjs from the native bench dir
"""
import collections, hashlib, json, os, re, statistics, subprocess, sys

OUT, X, S, N = sys.argv[1:5]

BASES = {  # frozen artifact prefix -> label, source revision, quantum
    'c2144b73362f': ('base8', '85c639dbb826e12dd2d832ec7241ff4936020a46', 64),
    '249330578f4a': ('int-c', 'ad7c98530d', 64),
    '97c686eb0ede': ('w2-all', 'c567017ceb', 64),
    '648ae421fcd1': ('w4base', '693a76f5', 256),
}
CONTRACTS = {  # instructions -> contract label (30 guest s)
    10073833775: 'pocket-q64', 9819885134: 'tinydraw-q64', 13335545195: 'fluidbox-motion-v2-q64',
    11565467394: 'pocket-q256', 9816141790: 'tinydraw-q256', 13334523179: 'fluidbox-motion-v2-q256',
    13353431060: 'fluidbox-motion-v2-q128',
}
WL = {'pocket-tank': 'pocket', 'tinydraw': 'tinydraw', 'fluidbox': 'fluidbox'}

def sha(path):
    return hashlib.sha256(open(path, 'rb').read()).hexdigest()

def pct(b, c):
    return 100 * (1 - c / b)

def median(xs):
    return statistics.median(xs)

problems = []

# ---------------------------------------------------------------- queue and runner log
jobs = {}
for l in open(f'{X}/queue/jobs.jsonl'):
    d = json.loads(l)
    jobs[d['name']] = d
rev_by_cand = {os.path.basename(d['candidate'])[:12]: d['rev'] for d in jobs.values() if d.get('rev')}

starts, gate_fail, gate_ok, skips = {}, [], [], []
for l in open(f'{S}/runner.log'):
    m = re.match(r'(\S+) START (\S+) \((\d+) pairs, (\S+)\) load=\{ ([\d.]+) ([\d.]+) ([\d.]+) \}', l)
    if m:
        starts[m[2]] = (m[1], [float(m[5]), float(m[6]), float(m[7])])
    m = re.match(r'(\S+) GATE EXACT FAIL (\S+)\.wasm', l)
    if m:
        gate_fail.append((m[1], m[2]))
    m = re.match(r'(\S+) SKIP (\S+): exactness gate failed', l)
    if m and m[1] >= '2026-09-23':  # the runner log also holds x6/x7 history
        skips.append((m[1], m[2]))

# ---------------------------------------------------------------- browser jobs
def round_of(n):
    if n.startswith('w4-'):
        return 'w4'
    if n.startswith(('ship', 'control-aa-fl256')):
        return 'ship'
    if n.startswith(('final8', 'control-aa-fl8b')):
        return 'final'
    if n.startswith(('w2-', 'w3-', 'int8-cnt')):
        return 'w2w3'
    if n.startswith('int8-'):
        return 'integration'
    return 'w1'

out_jobs, arm_rows = [], collections.defaultdict(list)
names = sorted(os.listdir(f'{S}/runs'), key=lambda n: starts.get(n, ('9',))[0])
for n in names:
    sp = f'{S}/runs/{n}/summary.json'
    if not os.path.exists(sp):
        continue
    s = json.load(open(sp))
    j = jobs.get(n, {})
    arms, fr, insns, cons, ok_all, wasm = [], set(), set(), set(), True, {}
    for r in s['runs']:
        a = json.load(open(f"{S}/runs/{n}/{r['pair']}-{r['arm']}/result.json"))['result']
        if abs(a['wallSeconds'] - r['wallSeconds']) > 1e-9:
            problems.append((n, 'wall mismatch'))
        if a['instructions'] != r['instructions']:
            problems.append((n, 'insn mismatch'))
        wasm[r['arm']] = r['provenance']['sha256']['asset/wasm']
        fr.add(a['frames']); insns.add(a['instructions']); cons.add(r['consoleSha256'])
        ok = (a['passed'] and a['status'] == 'completed' and a['jit']['failed'] == 0
              and (all(c['count'] >= c['min'] for c in a['checks']) if a['checks'] is not None else True)
              and (a['verdictValidation'] is None or a['verdictValidation']['passed']))
        ok_all &= ok
        arms.append([r['pair'], r['arm'][0], r['wallSeconds'], a['frames'], a['instructions'], a['jit']['bytes'],
                     round(a['jit']['compileMs'], 1), r['consoleSha256']])
    med = {k: median([x[2] for x in arms if x[1] == k[0]]) for k in ('baseline', 'candidate')}
    red = pct(med['baseline'], med['candidate'])
    pairs = [pct([x[2] for x in arms if x[0] == p and x[1] == 'b'][0], [x[2] for x in arms if x[0] == p and x[1] == 'c'][0])
             for p in range(1, s['pairs'] + 1)]
    if abs(red - s['wallReductionPercent']) > 1e-9:
        problems.append((n, 'median red mismatch'))
    if any(abs(a - b) > 1e-9 for a, b in zip(pairs, s['pairsWallReductionPercent'])):
        problems.append((n, 'pair mismatch'))
    if len(insns) != 1 or len(cons) != 1 or len(fr) != 1:
        problems.append((n, f'work differs {insns} {fr} {len(cons)}'))
    b12 = wasm['baseline'][:12]
    if b12 not in BASES:
        problems.append((n, 'unknown baseline ' + b12))
    ins = next(iter(insns))
    st = starts.get(n, (None, None))
    rev = j.get('rev') or rev_by_cand.get(wasm['candidate'][:12])
    rnd = round_of(n)
    out_jobs.append({
        'name': n, 'round': rnd, 'workload': WL[a['workload']], 'contract': CONTRACTS.get(ins, 'unknown'),
        'quantum': int(CONTRACTS.get(ins, 'q0').rsplit('-q', 1)[1]),  # both arms run the contract's quantum
        'agent': j.get('agent') or 'coordinator', 'sourceRevision': rev, 'note': j.get('note') or None,
        'baseline': BASES.get(b12, (b12,))[0], 'baselineWasmSha256': wasm['baseline'],
        'candidateWasmSha256': wasm['candidate'], 'pairs': s['pairs'],
        'medianWallSeconds': s['medianWallSeconds'], 'wallReductionPercent': round(s['wallReductionPercent'], 4),
        'pairsWallReductionPercent': [round(x, 4) for x in s['pairsWallReductionPercent']],
        'instructions': ins, 'frames': next(iter(fr)), 'consoleSha256': next(iter(cons)), 'allArmsPass': ok_all,
    })
    arm_rows[rnd].append({'name': n, 'agent': j.get('agent') or 'coordinator', 'startedAt': st[0],
                          'loadAtStart': st[1], 'arms': arms})

m3_list = [l.split()[0] for l in open(f'{S}/jobs.txt') if l.strip() and not l.startswith('#')]
pending = [n for n in m3_list if not os.path.exists(f'{S}/runs/{n}/summary.json')]
pruned = [n for n in jobs if n not in m3_list and not os.path.exists(f'{S}/runs/{n}/summary.json')]

# ---------------------------------------------------------------- q64/q256 validation and quantum ceiling
def serial(d):
    ev = json.load(open(f'{d}/events.json'))
    return ''.join(e['data'] for e in ev if e['type'] == 'serial')

def guest_metrics(workload, text):
    lines = text.split('\r\n')
    if workload == 'fluidbox':
        rep = [re.search(r'I \((\d+)\) fluidbox: ([\d.]+) fps \| ([\d.]+) steps/s', l) for l in lines]
        rep = [[int(m[1]), float(m[2]), float(m[3])] for m in rep if m]
        return {'reports': len(rep), 'fpsStepsPerReport': rep}
    if workload == 'pocket-tank':
        st = [re.search(r'I \((\d+)\) pocket-tank: t=(\d+)s .*\| asks (\d+) decisions (\d+) last (\d+) ms ([\d.]+) tok/s', l)
              for l in lines]
        st = [[int(m[2]), int(m[3]), int(m[4]), float(m[6])] for m in st if m]
        acts = [re.search(r'-> (\w+) urgency', l)[1] for l in lines if 'advisor: zone' in l and '->' in l]
        return {'status_t_asks_decisions_tokps': st, 'advisorLines': len(acts), 'actions': acts}
    return {}

qv = []
for d in sorted(os.listdir(f'{S}/qv'), key=lambda n: os.path.getmtime(f'{S}/qv/{n}/result.json')):
    x = json.load(open(f'{S}/qv/{d}/result.json'))['result']
    q = dict(x['appliedExports']).get('esp32sim_set_quantum', 64)
    qv.append({'arm': d, 'workload': WL[x['workload']], 'quantum': q, 'wasmSha256': x['provenance']['sha256']['asset/wasm'],
               'wallSeconds': x['wallSeconds'], 'instructions': x['instructions'], 'contract': CONTRACTS.get(x['instructions'], 'unknown'),
               'frames': x['frames'], 'jitBytes': x['jit']['bytes'], 'jitFailed': x['jit']['failed'], 'passed': x['passed'],
               'checks': x['checks'], 'verdictValidation': x['verdictValidation'],
               'verdictItems': (len(re.findall(r'=1\b', x['verdict'] or '')) if x['verdict'] else None),
               'guest': guest_metrics(x['workload'], serial(f'{S}/qv/{d}'))})
qsum = {}
for w in ('fluidbox', 'pocket', 'tinydraw'):
    m = {q: median([a['wallSeconds'] for a in qv if a['workload'] == w and a['quantum'] == q]) for q in (64, 256)}
    qsum[w] = {'arms': 3, 'medianWallSeconds': {'q64': m[64], 'q256': m[256]},
               'wallChangePercent': round(100 * (m[256] / m[64] - 1), 4),
               'instructions': {f'q{q}': sorted({a['instructions'] for a in qv if a['workload'] == w and a['quantum'] == q}) for q in (64, 256)},
               'frames': {f'q{q}': sorted({a['frames'] for a in qv if a['workload'] == w and a['quantum'] == q}) for q in (64, 256)},
               'guestIdenticalWithinQuantum': {f'q{q}': len({json.dumps(a['guest']) for a in qv if a['workload'] == w and a['quantum'] == q}) == 1
                                               for q in (64, 256)},
               'guestIdenticalAcrossQuanta': len({json.dumps(a['guest']) for a in qv if a['workload'] == w}) == 1}
    g = {q: next(a['guest'] for a in qv if a['workload'] == w and a['quantum'] == q) for q in (64, 256)}
    if w == 'fluidbox':
        qsum[w]['guestMeans'] = {f'q{q}': {'reports': g[q]['reports'],
                                           'fps': round(statistics.mean(r[1] for r in g[q]['fpsStepsPerReport']), 2),
                                           'stepsPerSecond': round(statistics.mean(r[2] for r in g[q]['fpsStepsPerReport']), 2)} for q in (64, 256)}
    if w == 'pocket':
        qsum[w]['guestStatus'] = {f'q{q}': {'status_t_asks_decisions_tokps': g[q]['status_t_asks_decisions_tokps'],
                                            'advisorLines': g[q]['advisorLines']} for q in (64, 256)}
    if not all(a['passed'] and a['jitFailed'] == 0 for a in qv if a['workload'] == w):
        problems.append(('qv', w, 'arm failed'))
# headline: base8 at its q64 default against the shipped stack at q256, single alternating arms (different work)
hv = []
for d in sorted(os.listdir(f'{S}/hv'), key=lambda n: os.path.getmtime(f'{S}/hv/{n}/result.json')):
    x = json.load(open(f'{S}/hv/{d}/result.json'))['result']
    q = dict(x['appliedExports']).get('esp32sim_set_quantum', 64)
    hv.append({'arm': d, 'workload': WL[x['workload']], 'stack': d.rsplit('-', 2)[1], 'quantum': q,
               'wasmSha256': x['provenance']['sha256']['asset/wasm'], 'wallSeconds': x['wallSeconds'],
               'instructions': x['instructions'], 'contract': CONTRACTS.get(x['instructions'], 'unknown'), 'frames': x['frames'],
               'jitBytes': x['jit']['bytes'], 'jitFailed': x['jit']['failed'], 'passed': x['passed'], 'checks': x['checks'],
               'verdictValidation': x['verdictValidation'], 'guest': guest_metrics(x['workload'], serial(f'{S}/hv/{d}'))})
hsum = {}
for w in ('fluidbox', 'pocket', 'tinydraw'):
    m = {k: median([a['wallSeconds'] for a in hv if a['workload'] == w and a['stack'] == k]) for k in ('base', 'ship')}
    hsum[w] = {'arms': 3, 'medianWallSeconds': {'base8-q64': m['base'], 'ship-q256': m['ship']},
               'wallChangePercent': round(100 * (m['ship'] / m['base'] - 1), 4), 'speedup': round(m['base'] / m['ship'], 4)}
    if not all(a['passed'] and a['jitFailed'] == 0 for a in hv if a['workload'] == w):
        problems.append(('hv', w, 'arm failed'))
qt = []
for q in ('q64', 'q128', 'q256'):
    x = json.load(open(f'{S}/qt/{q}/result.json'))['result']
    qt.append({'quantum': int(q[1:]), 'wasmSha256': x['provenance']['sha256']['asset/wasm'], 'wallSeconds': x['wallSeconds'],
               'instructions': x['instructions'], 'frames': x['frames'], 'jitBytes': x['jit']['bytes'], 'passed': x['passed'],
               'checks': x['checks']})

refs = {}
for f in sorted(os.listdir(f'{X}/notes')):
    if f.startswith('exact-ref-') and f.endswith('.json'):
        refs[f] = {k: v for k, v in json.load(open(f'{X}/notes/{f}')).items() if k != 'wallSeconds'}  # laptop wall time: not a timing result

# ---------------------------------------------------------------- native screens
native = []
for d in sorted(os.listdir(f'{N}/runs')):
    p = f'{N}/runs/{d}/summary.json'
    if not os.path.isfile(p):
        continue
    s = json.load(open(p))
    rows = [[r['pair'], r['arm'][0], r['wallSeconds'], [int(c) for c in r['coreInstructions']],
             re.sub(r'\[emu\] ', '', r['jitLine']), r['consoleSha256'], [round(v, 2) for v in r['loadBefore']]] for r in s['rows']]
    npairs = max(r[0] for r in rows)
    med = {k: median([r[2] for r in rows if r[1] == k[0]]) for k in ('baseline', 'candidate')}
    red = pct(med['baseline'], med['candidate'])
    if abs(red - s['wallReductionPercent']) > 1e-9:
        problems.append((d, 'native red'))
    work = {a: {json.dumps(r[3]) for r in rows if r[1] == a} for a in 'bc'}
    cons = {a: {r[5] for r in rows if r[1] == a} for a in 'bc'}
    if any(len(v) != 1 for v in list(work.values()) + list(cons.values())):
        problems.append((d, 'native work differs within an arm kind'))
    native.append({'name': d, 'workload': {'pocket': 'pocket', 'panel': 'panel-sid-7s', 'fluid': 'fluidbox-still-30s'}[d.split('-')[1]],
                   'baseline': os.path.basename(s['identities']['baseline']['path']), 'baselineSha256': s['identities']['baseline']['sha256'],
                   'candidate': os.path.basename(s['identities']['candidate']['path']), 'candidateSha256': s['identities']['candidate']['sha256'],
                   'inputs': {os.path.basename(k): v for k, v in s['inputs'].items()}, 'pairs': npairs, 'medianWallSeconds': s['medianWallSeconds'],
                   'wallReductionPercent': round(s['wallReductionPercent'], 4),
                   'pairsWallReductionPercent': [round(pct([r[2] for r in rows if r[0] == p and r[1] == 'b'][0],
                                                           [r[2] for r in rows if r[0] == p and r[1] == 'c'][0]), 4) for p in range(1, npairs + 1)],
                   'equalWork': work['b'] == work['c'] and cons['b'] == cons['c'], 'rows': rows})

# ---------------------------------------------------------------- CPU profiles (shares only)
def profile_shares(path):
    p = json.load(open(path))
    nodes = {n['id']: n for n in p['nodes']}
    self_t = collections.Counter()
    for s_, dt in zip(p['samples'], p['timeDeltas']):
        self_t[s_] += dt
    raw = collections.Counter()
    for i, t in self_t.items():
        cf = nodes[i]['callFrame']
        raw[(cf['functionName'] or '(anonymous)', cf.get('url', ''))] += t
    mangled = sorted({f for f, _ in raw if f.startswith('_R')})
    dem = dict(zip(mangled, subprocess.run(['c++filt'], input='\n'.join(mangled), capture_output=True, text=True).stdout.split('\n')))

    def short(d):
        for _ in range(4):
            d = re.sub(r'<[^<>]*>', '', d)
        parts = [x for x in re.sub(r'\[[0-9a-f]+\]', '', d).split('::') if x]
        return parts[-1] if parts else d
    cat, rust, gen = collections.Counter(), collections.Counter(), collections.Counter()
    for (f, u), t in raw.items():
        s_ = t / 1e6
        if f in ('(idle)', '(program)', '(garbage collector)', '(root)'):
            cat[f] += s_
        elif u.startswith('wasm://') and 'esp32sim_wasm' not in u:
            cat['generated'] += s_
            if f != 'wasm-function[0]':
                gen[f.replace('xtensa_', '')] += s_
        elif 'esp32sim_wasm' in u:
            cat['rust'] += s_
            rust[short(dem.get(f, f))] += s_
        else:
            cat['js'] += s_
    tot = sum(cat.values())
    return {'profiledSeconds': round(tot, 2), 'busySeconds': round(tot - cat['(idle)'], 2),
            'generated': round(cat['generated'], 2), 'rust': round(cat['rust'], 2), 'js': round(cat['js'], 2),
            'idle': round(cat['(idle)'], 2), 'rustTop': [[k, round(v, 2)] for k, v in rust.most_common(10)],
            'modulesTop': [[k, round(v, 2)] for k, v in gen.most_common(6)]}

PROFILES = [  # lab dir, label, build, workload, quantum
    ('base8-fl', 'step 0 base8', 'cpu-profile', 'fluidbox', 64), ('base8-prod-fl', 'step 0 base8', 'production', 'fluidbox', 64),
    ('base8-pocket', 'step 0 base8', 'cpu-profile', 'pocket', 64),
    ('int8-c-fl', 'int-c', 'cpu-profile', 'fluidbox', 64), ('int8-c-prod-fl', 'int-c', 'production', 'fluidbox', 64),
    ('w3all-cp-fl', 'w3-all', 'cpu-profile', 'fluidbox', 64), ('w3all-prod-fl', 'w3-all', 'production', 'fluidbox', 64),
    ('w3all-q256-cp', 'w3-all q256', 'cpu-profile', 'fluidbox', 256), ('w3all-q256-prod', 'w3-all q256', 'production', 'fluidbox', 256),
]
profiles = []
for d, label, build, w, q in PROFILES:
    rj = f'{S}/prof/{d}/result.json'
    x = json.load(open(rj))['result']
    profiles.append({'profile': d, 'stack': label, 'build': build, 'workload': w, 'quantum': q,
                     'wasmSha256': x['provenance']['sha256']['asset/wasm'], 'instructions': x['instructions'],
                     'frames': x['frames'], 'wallSecondsUnderProfiler': round(x['wallSeconds'], 3),
                     **profile_shares(f'{X}/results/prof/{d}/battery.cpuprofile')})

# ---------------------------------------------------------------- sources
def rel_files(root, sub, pred=lambda p: True):
    outl = []
    for dp, dn, fn in os.walk(os.path.join(root, sub)):
        dn[:] = sorted(x for x in dn if x not in ('chrome-profile', 'arm'))
        for f in sorted(fn):
            p = os.path.join(dp, f)
            if pred(p):
                outl.append(os.path.relpath(p, root))
    return outl

lab = (['BRIEF.md'] + rel_files(X, 'tasks') + rel_files(X, 'notes') + ['queue/jobs.jsonl']
       + rel_files(X, 'results', lambda p: p.endswith(('.txt', '.cpuprofile')) or p.endswith('result.json')))
m3 = (['runner.log', 'jobs.txt', 'm3-prof.py', 'm3-prof2.py'] + rel_files(S, 'runs', lambda p: p.endswith(('summary.json', 'runs.json')))
      + rel_files(S, 'qv') + rel_files(S, 'qt') + rel_files(S, 'hv') + rel_files(S, 'prof'))
nat = rel_files(N, 'runs', lambda p: p.endswith('.json')) + rel_files(N, 'tools')
arm_caps = sorted(rel_files(S, 'runs', lambda p: p.endswith('result.json')))
dig = hashlib.sha256()
for p in arm_caps:
    dig.update(f'{p} {sha(os.path.join(S, p))}\n'.encode())
sources = {
    'note': 'SHA-256 and size of the files this evidence was curated from. lab/: the x8 lab (briefs, task files, worker and review '
            'notes, Node exactness refs, the job queue, census receipts, CPU profiles). m3/: the M3 runner directory (runner log, '
            'job list, per-job summaries, validation and ceiling arms, profile results). m3-native/: the native bench directory. '
            'perArmCaptures: digest over "<path> <sha256>" lines of every per-arm result.json. None of these files is committed.',
    'lab': {p: [sha(os.path.join(X, p)), os.path.getsize(os.path.join(X, p))] for p in lab},
    'm3': {p: [sha(os.path.join(S, p)), os.path.getsize(os.path.join(S, p))] for p in m3},
    'm3-native': {p: [sha(os.path.join(N, p)), os.path.getsize(os.path.join(N, p))] for p in nat},
    'perArmCaptures': {'files': len(arm_caps), 'sha256': dig.hexdigest()},
}

# ---------------------------------------------------------------- write
if problems:
    print('PROBLEMS', problems, file=sys.stderr)
    sys.exit(1)

def dump(obj, path, key='{"name"'):
    t = json.dumps(obj, separators=(',', ':'), ensure_ascii=False).replace(key, '\n' + key)
    open(path, 'w').write(t + '\n')
    return os.path.getsize(path)

os.makedirs(os.path.join(OUT, 'arms'), exist_ok=True)
hdr = {'note': 'arms: [pair, b|c, wallSeconds, frames, instructions, jitBytes, jitCompileMs, consoleSha256] in execution order. '
               'agent: lab lane that queued the job. startedAt and loadAtStart (1/5/15-minute load averages) come from the runner log '
               'and are per job, not per arm. Index and summaries: ../results.json.'}
sizes = {r: dump({**hdr, 'jobs': v}, os.path.join(OUT, 'arms', f'{r}.json')) for r, v in arm_rows.items()}
index = {
    'schema': 'esp32sim-perf-x8-v1',
    'note': 'Positive reduction = less wall time. wallReductionPercent = 100*(1-median(candidate)/median(baseline)). Each job compares one '
            'frozen candidate with one frozen baseline; jobs against different baselines must not be added. Instructions, frames and console '
            'hash are equal across every arm of a job (checked when generated). quantum: the scheduling quantum both arms ran, from the pinned '
            'contract; the ship jobs and control-aa-fl256 run base8 at 256 through the workload exports (its default is 64). '
            'Per-arm samples are in armFiles.',
    'chrome': {'browser': 'Chrome/153.0.8010.53', 'v8': '15.3.76.13', 'mode': 'headless, fresh process per arm'},
    'baselines': {v[0]: {'wasmSha256': next(j['baselineWasmSha256'] for j in out_jobs if j['baseline'] == v[0]),
                         'sourceRevision': v[1], 'defaultQuantum': v[2]} for k, v in BASES.items()},
    'contracts': {v: k for k, v in CONTRACTS.items()},
    'excluded': [
        {'names': [n for _, n in skips], 'at': skips[0][0] if skips else None,
         'reason': 'runner exactness gate still held the q64 Pocket refs when the first q256 (w4) artifacts arrived; the identical frozen '
                   'artifacts passed the q256 gate after the switch and were timed then. No timing was discarded.'},
    ],
    'pendingAtCuration': pending,
    'prunedNotTimed': pruned,
    'armFiles': [f'arms/{r}.json' for r in arm_rows],
    'jobs': out_jobs,
}
sizes['index'] = dump(index, os.path.join(OUT, 'results.json'))
sizes['q256'] = dump({
    'note': 'q64 vs q256 validation on the M3 (Chrome 153): the w3-all artifact with esp32sim_set_quantum 256 through the workload '
            'exports (q64 arms run the default). Single alternating arms, three per workload and quantum, in the listed order; not a paired '
            'job. q256 changes the guest work (instructions, and for TinyDraw frames), so wall times are not equal-work comparisons. guest: '
            'parsed from each arm\'s serial console (fluidbox: [ms, fps, steps/s] per report; Pocket: [t s, asks, decisions, tok/s] per '
            'status line and the advisor actions in order). guestIdentical*: whether the parsed guest output matches exactly. quantumCeiling: one arm each at q64/q128/q256. nodeRefs: the lab Node '
            'exactness references (30 s unless named 6s; laptop wall times removed: not timing results).',
    'validation': qsum, 'arms': qv, 'quantumCeiling': qt,
    'headline': {'note': 'base8 (c2144b73, its q64 default, M3 tree without a quantum export) against x8/ship (38e599d0) at q256 through the '
                         'workload exports; three alternating single arms per workload, in the listed order; different work.',
                 'summary': hsum, 'arms': hv},
    'nodeRefs': refs}, os.path.join(OUT, 'q256.json'), key='{"arm"')
sizes['native'] = dump({
    'note': 'Native CLI screens on the M3 Pro, runner paused. rows: [pair, b|c, wallSeconds, [core0, core1 instructions], JIT line, '
            'consoleSha256, load before the arm]. Pairs alternate. final8-* compare equal work; q256-* compare a q64 binary with a q256 '
            'binary, so work and console differ (wall only).',
    'screens': native}, os.path.join(OUT, 'native.json'))
sizes['profiles'] = dump({'note': 'Self time in seconds by category and top functions, from Chrome .cpuprofile captures (not committed).',
                          'profiles': profiles}, os.path.join(OUT, 'profiles.json'), key='{"profile"')
open(os.path.join(OUT, 'sources.json'), 'w').write(json.dumps(sources, indent=1) + '\n')
print(len(out_jobs), 'jobs', sum(len(r['arms']) for v in arm_rows.values() for r in v), 'arms;', len(qv), 'validation arms;',
      len(native), 'native screens; pending', pending, 'pruned', pruned, 'sizes', sizes)
