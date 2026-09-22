#!/usr/bin/env python3
"""Firefox ordinary-page fallback: no WebDriver, AppleScript or permission changes.

Start after Chrome finishes. Open the printed token URL in Firefox. Each arm uses
a fresh page and worker, but the same browser process may retain caches. The
page must remain visible; hidden-page runs fail rather than quietly publishing.
"""
import argparse
import hashlib
import http.server
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import secrets
import statistics
import threading
import urllib.parse

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('pairs', HERE / 'run-pairs.py')
pairs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pairs)
WORKLOADS = pairs.WORKLOADS

PAGE = r'''<!doctype html><meta charset="utf-8"><title>M1 Firefox paired benchmark</title>
<pre id="status">Starting a fresh benchmark worker...</pre><script>
const token=TOKEN,index=INDEX,label=LABEL;
const status=document.getElementById('status');
const events=[];let hidden=document.hidden,finished=false;
document.addEventListener('visibilitychange',()=>{if(document.hidden)hidden=true});
const worker=new Worker('/battery-worker.mjs',{type:'module'});
const timeout=setTimeout(()=>finish({type:'error',line:'Firefox battery timeout after 660 seconds'}),660000);
async function finish(result){
  if(finished)return;finished=true;clearTimeout(timeout);worker.terminate();
  status.textContent=label+' — checking output';
  try {
    const r=await fetch('/submit?token='+token,{method:'POST',headers:{'Content-Type':'application/json'},
      body:JSON.stringify({index,result,events,userAgent:navigator.userAgent,hidden})});
    const answer=await r.json();if(!r.ok)throw Error(answer.error);
    if(answer.done){status.textContent='All Firefox pairs completed. Results saved on disk.';return}
    location.replace('/?token='+token+'&index='+answer.next);
  }catch(e){status.textContent='Stopped: '+e.message}
}
worker.onmessage=({data:e})=>{events.push(e);
  if(e.type==='progress')status.textContent=label+'\n'+JSON.stringify(e,null,2);
  if(e.type==='result'||e.type==='error')finish(e);
};
worker.onerror=e=>finish({type:'error',line:e.message});
if(hidden)finish({type:'error',line:'The Firefox benchmark page must be visible'});
else worker.postMessage({start:true,jit:true});
</script>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, default=8794)
    parser.add_argument('--out', type=Path, default=HERE / 'results')
    parser.add_argument('--campaign-root', type=Path, required=True, help='Contains artifacts/{before,after}/main.wasm and assets-{pocket,tinydraw}.json')
    parser.add_argument('--tree', type=Path, required=True, help='Source checkout providing matching web/wasm modules')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    plan, jobs = [], {}
    for label, asset in [('pocket', 'assets-pocket.json'), ('tinydraw', 'assets-tinydraw.json')]:
        assets = json.loads((args.campaign_root / asset).read_text())
        assets = {key: str((args.campaign_root / value).resolve()) if key != 'workload' else value for key, value in assets.items()}
        workload = assets['workload']
        for kind, count in [('warmup', 1), ('control', 2), ('before-after', 4)]:
            name = f'{kind}-{label}'
            job = args.out / name
            job.mkdir()
            arms = {}
            for arm in ['baseline', 'candidate']:
                artifact = 'before' if arm == 'baseline' or kind == 'control' else 'after'
                arms[arm] = pairs.prepare(job, arm, args.tree, args.campaign_root / 'artifacts' / artifact / 'main.wasm', assets)
            jobs[name] = {'path': job, 'pairs': count, 'rows': [], 'workload': workload}
            for pair in range(1, count + 1):
                order = ['baseline', 'candidate'] if pair % 2 else ['candidate', 'baseline']
                for arm in order:
                    plan.append({'job': name, 'pair': pair, 'arm': arm, 'root': arms[arm], 'workload': workload})
    pairs.write_json(args.out / 'plan.json', [{**p, 'root': str(p['root'])} for p in plan])
    state = {'index': 0, 'failed': False, 'started': False}
    token = secrets.token_urlsafe(24)
    lock = threading.Lock()
    server_origin = f'http://127.0.0.1:{args.port}'

    class Handler(http.server.BaseHTTPRequestHandler):
        def reply(self, status, data, kind='application/json'):
            if not isinstance(data, bytes):
                data = json.dumps(data).encode()
            self.send_response(status)
            for key, value in [('Content-Type', kind), ('Cache-Control', 'no-store'),
                               ('Cross-Origin-Opener-Policy', 'same-origin'),
                               ('Cross-Origin-Embedder-Policy', 'require-corp'),
                               ('Content-Length', str(len(data)))]:
                self.send_header(key, value)
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            parsed = urllib.parse.urlsplit(self.path)
            query = urllib.parse.parse_qs(parsed.query)
            if parsed.path == '/':
                with lock:
                    if query.get('token') != [token] or query.get('index', ['0']) != [str(state['index'])] or state['failed'] or state['index'] >= len(plan) or state['started']:
                        return self.reply(409, {'error': 'Invalid, duplicate or completed arm'})
                    state['started'] = True
                    state['loadBefore'] = os.getloadavg()
                    item = plan[state['index']]
                    label = f"{item['job']}, pair {item['pair']}, {item['arm']} ({state['index'] + 1}/{len(plan)})"
                    page = PAGE.replace('TOKEN', json.dumps(token)).replace('INDEX', str(state['index'])).replace('LABEL', json.dumps(label))
                    print('RUNNING ' + label, flush=True)
                    return self.reply(200, page.encode(), 'text/html')
            if state['failed'] or state['index'] >= len(plan):
                return self.reply(409, {'error': 'Campaign is not active'})
            item = plan[state['index']]
            arm_root = item['root']
            assets = json.loads((arm_root / 'assets.json').read_text())
            if parsed.path == '/provenance.json':
                files = {f'asset/{key}': Path(value) for key, value in assets.items() if key != 'workload'}
                files.update({str(p.relative_to(arm_root)): p for p in (arm_root / 'web/wasm').rglob('*') if p.is_file() and p.suffix in ('.js', '.mjs')})
                files.update({f'harness/{p.name}': p for p in HERE.iterdir() if p.suffix in ('.mjs', '.json')})
                return self.reply(200, {'sha256': {key: pairs.sha(p) for key, p in sorted(files.items())}})
            if parsed.path == '/workload.json':
                workload = item['workload']
                return self.reply(200, {**WORKLOADS[workload], 'name': workload, 'key': workload})
            if parsed.path.startswith('/asset/'):
                key = parsed.path[len('/asset/'):]
                if key not in assets or key == 'workload':
                    return self.reply(404, {'error': 'Unknown asset'})
                path, kind = Path(assets[key]), 'application/octet-stream'
            elif parsed.path.startswith('/web/wasm/'):
                path, kind = (arm_root / parsed.path.lstrip('/')).resolve(), 'text/javascript'
                if not path.is_relative_to(arm_root / 'web/wasm'):
                    return self.reply(404, {'error': 'Unknown module'})
            elif parsed.path in ['/battery-worker.mjs', '/battery.mjs', '/verdict.mjs', '/verdict-schema.json']:
                path = HERE / parsed.path.lstrip('/')
                kind = 'application/json' if path.suffix == '.json' else 'text/javascript'
            else:
                return self.reply(404, {'error': 'Unknown path'})
            try:
                self.reply(200, path.read_bytes(), kind)
            except FileNotFoundError:
                self.reply(404, {'error': 'Missing file'})

        def do_POST(self):
            parsed = urllib.parse.urlsplit(self.path)
            query = urllib.parse.parse_qs(parsed.query)
            if parsed.path != '/submit' or query.get('token') != [token] or self.headers.get('Origin') != server_origin:
                return self.reply(403, {'error': 'Invalid submission origin or token'})
            with lock:
                if state['failed'] or state['index'] >= len(plan):
                    return self.reply(409, {'error': 'Campaign is not active'})
                item = plan[state['index']]
                job = jobs[item['job']]
                run = job['path'] / f"{item['pair']}-{item['arm']}"
                try:
                    data = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                    if data['index'] != state['index'] or not state['started']:
                        raise ValueError('Wrong or duplicate arm index')
                    run.mkdir()
                    ua = data['userAgent']
                    match = re.search(r'Firefox/([\d.]+)', ua)
                    if not match or any(s in ua for s in ('Chrome/', 'Chromium/', 'Edg/')):
                        raise ValueError('This campaign requires Firefox')
                    raw = {'captureMode': 'timing', 'captureTransport': 'Firefox ordinary browser page',
                           'version': {'Browser': 'Firefox/' + match[1], 'User-Agent': ua, 'Engine': 'SpiderMonkey'}, 'result': data['result']}
                    pairs.write_json(run / 'result.json', raw)
                    pairs.write_json(run / 'events.json', data['events'])
                    pairs.write_json(run / 'capture.json', {'mode': 'timing', 'browserFamily': 'Firefox',
                        'method': 'Ordinary visible page; fresh page and worker per arm; browser process and caches may persist',
                        'pageWasHidden': data['hidden'], 'loadBefore': state['loadBefore'], 'loadAfter': os.getloadavg(),
                        'harnessSha256': {'server.py': pairs.sha(Path(__file__)), 'compare-runs.py': pairs.sha(HERE / 'compare-runs.py')}})
                    if data['hidden']:
                        raise ValueError('Firefox page was hidden during this arm')
                    r = pairs.validate(raw, WORKLOADS[item['workload']]['expectedInstructions'], item['workload'])
                    if type(r['wallSeconds']) not in (int, float) or not math.isfinite(r['wallSeconds']) or r['wallSeconds'] <= 0:
                        raise ValueError('Invalid wall time')
                    if r['provenance']['sha256']['asset/wasm'] != pairs.sha(item['root'] / 'main.wasm'):
                        raise ValueError('Wrong WASM artifact')
                    checked = pairs.comparator.read_run(run)
                    row = {'pair': item['pair'], 'arm': item['arm'], 'consoleSha256': checked['consoleSha256'],
                        'browser': checked['browser'], 'v8': None, 'engine': 'SpiderMonkey',
                        'provenance': checked['provenance'], 'wallSeconds': r['wallSeconds'],
                        'guestSeconds': r['guestSeconds'], 'realtimeRatio': r['guestSeconds'] / r['wallSeconds'],
                        'instructions': r['instructions'], 'passed': r['passed'], 'jitFailed': r['jit']['failed'],
                        'jitBytes': r['jit']['bytes'], 'run': str(run)}
                    job['rows'].append(row)
                    pairs.write_json(job['path'] / 'runs.json', job['rows'])
                    if len(job['rows']) == 2 * job['pairs']:
                        groups = [[pairs.comparator.read_run(row['run']) for row in job['rows'] if row['arm'] == arm] for arm in ('baseline', 'candidate')]
                        pairs.comparator.comparison(*groups)
                        medians = {arm: statistics.median(row['wallSeconds'] for row in job['rows'] if row['arm'] == arm) for arm in ('baseline', 'candidate')}
                        summary = {'pairs': job['pairs'], 'medianWallSeconds': medians,
                            'wallReductionPercent': 100 * (1 - medians['candidate'] / medians['baseline']),
                            'pairsWallReductionPercent': [100 * (1 - next(r['wallSeconds'] for r in job['rows'] if r['pair'] == p and r['arm'] == 'candidate') / next(r['wallSeconds'] for r in job['rows'] if r['pair'] == p and r['arm'] == 'baseline')) for p in range(1, job['pairs'] + 1)], 'runs': job['rows']}
                        pairs.write_json(job['path'] / 'summary.json', summary)
                        print('COMPLETED ' + item['job'] + ': ' + str(summary['wallReductionPercent']) + '% less wall time', flush=True)
                    state['index'] += 1
                    state['started'] = False
                    done = state['index'] == len(plan)
                    pairs.write_json(args.out / 'status.json', {'completedArms': state['index'], 'totalArms': len(plan), 'status': 'completed' if done else 'running'})
                    self.reply(200, {'done': done, 'next': state['index']})
                except Exception as error:
                    state['failed'] = True
                    pairs.write_json(args.out / 'error.json', {'index': state['index'], 'error': str(error)})
                    print('FAILED: ' + str(error), flush=True)
                    self.reply(422, {'error': str(error)})

        def log_message(self, *_):
            pass

    print(f'Open in Firefox after Chrome finishes: {server_origin}/?token={token}&index=0', flush=True)
    http.server.ThreadingHTTPServer(('127.0.0.1', args.port), Handler).serve_forever()


if __name__ == '__main__':
    main()
