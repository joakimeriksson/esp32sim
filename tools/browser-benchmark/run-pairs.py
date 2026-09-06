#!/usr/bin/env python3
"""Build two checkouts and run alternating, uninstrumented Chrome battery pairs.

Pass --assets pointing to a JSON map of rom/app/bootloader/ptable/elf paths.
Alternatively --archive accepts an extracted performance-review archive.
Existing release WASM files can replace builds via --baseline-wasm/--candidate-wasm.
All builds finish before timing begins. Each arm snapshots its WASM and web modules.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import platform
from pathlib import Path
import shutil
import socket
import statistics
import subprocess
import sys
import time
import tomllib
import urllib.request

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location('compare_runs', HERE / 'compare-runs.py')
comparator = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(comparator)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, data):
    path.write_text(json.dumps(data, indent=2) + '\n')


def command(argv, **kwargs):
    return subprocess.check_output(argv, text=True, **kwargs).strip()


def validate_timing_build(record):
    diagnostic = {'jit-profile', 'cpu-profile', 'exit-stats', 'jit-tests',
                  'wasm-jit-profile', 'wasm-cpu-profile', 'wasm-jit-tests'}
    features = record.get('buildFeatures', [])
    if isinstance(features, list) and diagnostic.intersection(features):
        raise ValueError(f'diagnostic build cannot be used for timing: {features}')
    if isinstance(record.get('artifactBuild'), dict):
        validate_timing_build(record['artifactBuild'])


def prepare(out, name, tree, wasm, assets, rustflags=None):
    tree = tree.resolve()
    arm = out / name
    arm.mkdir()
    tracked = set(command(['git', 'ls-files'], cwd=tree).splitlines())
    workspace = tomllib.loads((tree / 'Cargo.toml').read_text())
    for member in workspace['workspace']['members']:
        tracked.update(str(path.relative_to(tree)) for path in (tree / member / 'src').rglob('*.rs'))
    sources = {p: sha(tree / p) for p in sorted(tracked)
               if (tree / p).is_file() and (p.endswith(('.rs', '.toml', '.lock', '.mjs', '.js')))}
    record = {'tree': str(tree), 'commit': command(['git', 'rev-parse', 'HEAD'], cwd=tree),
              'sourceSha256': sources, 'suppliedWasm': str(wasm.resolve()) if wasm else None,
              'rustflags': rustflags, 'buildFeatures': [] if wasm is None else 'external artifact; inspect provenance'}
    (arm / 'source.patch').write_text(command(['git', 'diff', 'HEAD', '--'], cwd=tree))
    if wasm is None:
        env = os.environ.copy()
        rustc = command(['rustup', 'which', 'rustc'])
        cargo = command(['rustup', 'which', 'cargo'])
        env['RUSTC'] = rustc
        env['DYLD_FALLBACK_LIBRARY_PATH'] = str(Path(rustc).parent.parent / 'lib') + ':' + env.get('DYLD_FALLBACK_LIBRARY_PATH', '')
        env['CARGO_TARGET_DIR'] = str(arm / 'target')
        # Ignore ambient profiling/instrumentation flags for comparable production builds.
        env.pop('RUSTFLAGS', None)
        env.pop('CARGO_ENCODED_RUSTFLAGS', None)
        if rustflags is not None:
            env['RUSTFLAGS'] = rustflags
        argv = [cargo, 'build', '--release', '--target', 'wasm32-unknown-unknown', '-p', 'esp32sim-wasm']
        record['buildCommand'] = argv
        record['rustc'] = command([rustc, '-Vv'])
        with (arm / 'build.log').open('w') as log:
            subprocess.run(argv, cwd=tree, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        wasm = arm / 'target/wasm32-unknown-unknown/release/esp32sim_wasm.wasm'
        changed = [p for p, digest in sources.items() if not (tree / p).is_file() or sha(tree / p) != digest]
        if changed:
            raise RuntimeError(f'{name} source changed during build: {changed}')
    if wasm is not None:
        origin = wasm.resolve().with_name('build.json')
        if origin.is_file():
            original = json.loads(origin.read_text())
            if original.get('wasmSha256') == sha(wasm):
                validate_timing_build(original)
                record['artifactBuild'] = original
    shutil.copy2(wasm, arm / 'main.wasm')
    shutil.copytree(tree / 'web/wasm', arm / 'web/wasm', ignore=shutil.ignore_patterns('*.wasm'))
    record['wasmSha256'] = sha(arm / 'main.wasm')
    write_json(arm / 'build.json', record)
    write_json(arm / 'assets.json', {**assets, 'wasm': str(arm / 'main.wasm')})
    return arm


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def await_server(url, process):
    for _ in range(150):
        if process.poll() is not None:
            raise RuntimeError(f'process exited before serving {url}')
        try:
            with urllib.request.urlopen(url, timeout=1) as response:
                return json.load(response)
        except OSError:
            time.sleep(.1)
    raise RuntimeError(f'timed out waiting for {url}')


def validate(raw, expected):
    r = raw['result']
    if r.get('instrumented'):
        raise ValueError('diagnostic exports present in timing capture')
    if not r['passed'] or r['status'] != 'completed' or r['stopCode'] != 0:
        raise ValueError('battery did not complete successfully')
    comparator.validate_verdict(r.get('verdict'))
    if r['instructions'] != expected:
        raise ValueError(f'instruction total {r["instructions"]} != {expected}')
    if r['jit']['failed'] != 0 or r['jit']['compiled'] <= 0:
        raise ValueError('JIT failures or JIT not exercised')
    if not r.get('provenance', {}).get('sha256'):
        raise ValueError('missing capture provenance')
    return r


def capture(arm, run, chrome, expected):
    run.mkdir()
    capture_record = {'mode': 'timing', 'platform': platform.platform(),
                      'loadBefore': os.getloadavg(),
                      'harnessSha256': {name: sha(HERE / name) for name in ('run-pairs.py', 'serve.py', 'capture-battery.mjs')}}
    port, debug = free_port(), free_port()
    while debug == port:
        debug = free_port()
    processes = []
    with (run / 'server.log').open('w') as server_log, (run / 'chrome.log').open('w') as chrome_log, (run / 'capture.log').open('w') as capture_log:
        try:
            server = subprocess.Popen([sys.executable, str(HERE / 'serve.py'), str(arm / 'assets.json'), '--web-root', str(arm), '--port', str(port)], stdout=server_log, stderr=subprocess.STDOUT)
            processes.append(server)
            await_server(f'http://127.0.0.1:{port}/provenance.json', server)
            chrome_command = [chrome, '--headless=new', '--no-first-run', '--no-default-browser-check', '--disable-background-timer-throttling', '--disable-renderer-backgrounding', '--remote-debugging-address=127.0.0.1', f'--remote-debugging-port={debug}', f'--user-data-dir={run / "chrome-profile"}', 'about:blank']
            capture_record['chromeCommand'] = chrome_command
            write_json(run / 'capture.json', capture_record)
            browser = subprocess.Popen(chrome_command, stdout=chrome_log, stderr=subprocess.STDOUT)
            processes.append(browser)
            await_server(f'http://127.0.0.1:{debug}/json/version', browser)
            subprocess.run(['node', str(HERE / 'capture-battery.mjs'), f'http://127.0.0.1:{port}/battery.html', str(run), str(debug)], stdout=capture_log, stderr=subprocess.STDOUT, check=True, timeout=660)
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
    capture_record['loadAfter'] = os.getloadavg()
    write_json(run / 'capture.json', capture_record)
    raw = json.loads((run / 'result.json').read_text())
    r = validate(raw, expected)
    if r['provenance']['sha256']['asset/wasm'] != sha(arm / 'main.wasm'):
        raise ValueError('captured WASM differs from arm artifact')
    shutil.rmtree(run / 'chrome-profile')
    checked = comparator.read_run(run)
    return {'consoleSha256': checked['consoleSha256'], 'browser': checked['browser'], 'v8': checked['v8'], 'provenance': checked['provenance'], 'wallSeconds': r['wallSeconds'], 'guestSeconds': r['guestSeconds'], 'realtimeRatio': r['guestSeconds'] / r['wallSeconds'], 'instructions': r['instructions'], 'passed': r['passed'], 'jitFailed': r['jit']['failed'], 'jitBytes': r['jit']['bytes'], 'run': str(run)}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('out', type=Path)
    p.add_argument('--baseline-tree', type=Path, required=True)
    p.add_argument('--candidate-tree', type=Path, required=True)
    p.add_argument('--baseline-wasm', type=Path)
    p.add_argument('--candidate-wasm', type=Path)
    p.add_argument('--baseline-rustflags', help='Explicit compiler experiment; recorded in build provenance')
    p.add_argument('--candidate-rustflags', help='Explicit compiler experiment; recorded in build provenance')
    source = p.add_mutually_exclusive_group(required=True)
    source.add_argument('--assets', type=Path)
    source.add_argument('--archive', type=Path)
    p.add_argument('--pairs', type=int, default=3)
    p.add_argument('--expected-instructions', type=int, default=9819885134)
    p.add_argument('--chrome', default=os.environ.get('CHROME', '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'))
    a = p.parse_args()
    if a.pairs < 1:
        p.error('--pairs must be positive')
    if a.archive:
        paths = json.loads((a.archive / 'assets/paths.json').read_text())
        assets = {k: str((a.archive / paths[k]).resolve()) for k in ('rom', 'app', 'bootloader', 'ptable', 'elf')}
    else:
        paths = json.loads(a.assets.read_text())
        assets = {k: str((a.assets.resolve().parent / paths[k]).resolve()) for k in ('rom', 'app', 'bootloader', 'ptable', 'elf')}
    for path in assets.values():
        if not Path(path).is_file():
            p.error('missing asset ' + path)
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    arms = {}
    for name in ('baseline', 'candidate'):
        print(f'Preparing {name}', flush=True)
        arms[name] = prepare(out, name, getattr(a, name + '_tree'), getattr(a, name + '_wasm'), assets, getattr(a, name + '_rustflags'))
    rows = []
    for pair in range(1, a.pairs + 1):
        order = ('baseline', 'candidate') if pair % 2 else ('candidate', 'baseline')
        for name in order:
            print(f'Running pair {pair}/{a.pairs} {name}', flush=True)
            row = {'pair': pair, 'arm': name, **capture(arms[name], out / f'{pair}-{name}', a.chrome, a.expected_instructions)}
            rows.append(row)
            write_json(out / 'runs.json', rows)
            print(json.dumps({key: row[key] for key in ('pair', 'arm', 'wallSeconds', 'realtimeRatio', 'instructions', 'passed', 'jitFailed')}), flush=True)
    for field in ('consoleSha256', 'browser', 'v8', 'instructions'):
        if len({r[field] for r in rows}) != 1:
            raise ValueError(f'unmatched {field} across campaign')
    for name in arms:
        identities = [r['provenance'] for r in rows if r['arm'] == name]
        if any(identity != identities[0] for identity in identities):
            raise ValueError(f'{name} inputs changed within campaign')
    before = next(r['provenance']['sha256'] for r in rows if r['arm'] == 'baseline')
    after = next(r['provenance']['sha256'] for r in rows if r['arm'] == 'candidate')
    for key in before.keys() | after.keys():
        if key != 'asset/wasm' and not key.startswith('web/wasm/') and before.get(key) != after.get(key):
            raise ValueError(f'undeclared input difference between arms: {key}')
    medians = {name: statistics.median(r['wallSeconds'] for r in rows if r['arm'] == name) for name in arms}
    summary = {'pairs': a.pairs, 'screeningOnly': a.pairs < 3, 'medianWallSeconds': medians, 'wallReductionPercent': 100 * (1 - medians['candidate'] / medians['baseline']), 'pairsWallReductionPercent': [100 * (1 - next(r['wallSeconds'] for r in rows if r['pair'] == i and r['arm'] == 'candidate') / next(r['wallSeconds'] for r in rows if r['pair'] == i and r['arm'] == 'baseline')) for i in range(1, a.pairs + 1)], 'runs': rows}
    write_json(out / 'summary.json', summary)
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
