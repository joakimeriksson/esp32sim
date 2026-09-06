#!/usr/bin/env python3
"""Compare matched, uninstrumented capture-battery runs and their console milestones."""
import argparse
import hashlib
import json
import pathlib
import statistics
import re

SCHEMA = json.loads(pathlib.Path(__file__).with_name('verdict-schema.json').read_text())


def validate_verdict(line):
    if not isinstance(line, str) or '\n' in line or '\r' in line:
        raise ValueError('missing or incomplete verdict')
    tokens = line.split()
    if not tokens or tokens.pop(0) != SCHEMA['marker']:
        raise ValueError('unexpected verdict marker')
    allowed = set(SCHEMA['gates']) | set(SCHEMA['receipts'])
    fields = {}
    for token in tokens:
        match = re.fullmatch(r'([a-z][a-z0-9_]*)=([a-z0-9]+)', token)
        if not match:
            raise ValueError(f'malformed field: {token}')
        key, value = match.groups()
        if key not in allowed or key in fields:
            raise ValueError(f'unknown or duplicate field: {key}')
        fields[key] = value
    if fields.keys() != allowed:
        raise ValueError('missing verdict fields')
    for key, value in fields.items():
        if value not in SCHEMA['receipts'].get(key, ['0', '1']):
            raise ValueError(f'invalid value: {key}')
    if any(fields[key] != '1' for key in SCHEMA['gates']):
        raise ValueError('firmware correctness gate failed')

# Intervals include all work between the preceding marker and this marker, including
# setup and console delivery. They are not isolated function or guest-device timings.
MILESTONES = (
    ('boot_and_native_kernels', 'TINYDRAW_GATE1_NATIVE_KERNELS'),
    ('cold_render_and_initial_pan', 'TINYDRAW_GATE1_RING_LOCAL'),
    ('pan_sequences', 'TINYDRAW_GATE1_PAN_BOUNDARY'),
    ('cache_tour', 'TINYDRAW_GATE1_CACHE_TOUR'),
    ('mixed_drawing', 'TINYDRAW_GATE1_MIXED_DRAW_SUMMARY'),
    ('hairlines', 'TINYDRAW_GATE1_HAIRLINE'),
    ('export', 'TINYDRAW_GATE1_EXPORT'),
    ('history', 'TINYDRAW_GATE1_HISTORY_SUMMARY'),
    ('settling', 'TINYDRAW_GATE1_AUTOMATED_DONE'),
)


def read_run(directory, *, legacy=False):
    directory = pathlib.Path(directory)
    capture = json.loads((directory / 'result.json').read_text())
    result, version = capture['result'], capture['version']
    if not legacy:
        if capture.get('captureMode') != 'timing':
            raise ValueError(f'{directory}: missing explicit timing capture mode')
        if result.get('stopCode') != 0 or result.get('jit', {}).get('failed') != 0:
            raise ValueError(f'{directory}: missing clean stop/JIT status or reported failure')
        if not (result.get('provenance') or {}).get('sha256', {}).get('asset/wasm'):
            raise ValueError(f'{directory}: missing build provenance')
    if not result['passed'] or result['status'] != 'completed':
        raise ValueError(f'{directory}: firmware did not complete successfully')
    validate_verdict(result.get('verdict'))
    validation = result.get('verdictValidation')
    if validation is not None and validation.get('schema') != SCHEMA['version']:
        raise ValueError(f'{directory}: unsupported verdict schema')
    if 'HeadlessChrome/' not in version['User-Agent']:
        raise ValueError(f'{directory}: expected a headless Chrome timing capture')
    events = json.loads((directory / 'events.json').read_text())
    if any(event.get('type') in ('emu', 'log')
           and any(marker in event.get('line', '') for marker in ('jit-profile', '[wasm-profile]'))
           for event in events):
        raise ValueError(f'{directory}: profiling captures cannot establish production speed')
    serial, pending, times = '', '', {}
    markers = {marker for _, marker in MILESTONES}
    for event in events:
        if event.get('type') != 'serial':
            continue
        serial += event['data']
        pending += event['data']
        lines = pending.split('\n')
        pending = lines.pop()
        for line in lines:
            marker = line.split(' ', 1)[0].strip()
            if marker in markers:
                times[marker] = event['wallMs'] / 1000
    verdicts = [line.rstrip('\r') for line in serial.split('\n')[:-1]
                if line.startswith(SCHEMA['marker'])]
    if verdicts != [result['verdict']] or SCHEMA['marker'] in pending:
        raise ValueError(f'{directory}: missing, duplicate or mismatched console verdict')
    if re.search(r'Guru Meditation|TG1WDT_SYS_RST|stack overflow|task_wdt', serial) or any(
            re.search(r'chip reset|panic', event.get('line', ''), re.I)
            for event in events if event.get('type') in ('emu', 'log', 'error')):
        raise ValueError(f'{directory}: firmware failure in captured output')
    intervals, previous = {}, 0
    for name, marker in MILESTONES:
        end = times.get(marker)
        if end is None or end < previous:
            raise ValueError(f'{directory}: missing or out-of-order milestone {marker}')
        intervals[name] = end - previous
        previous = end
    return {
        'directory': str(directory),
        'verdictSchema': SCHEMA['version'],
        'provenance': result.get('provenance'),
        'wallSeconds': result['wallSeconds'],
        'intervalSeconds': intervals,
        'instructions': result['instructions'],
        'jitInstructions': result['jit']['instructions'],
        'consoleSha256': hashlib.sha256(serial.encode()).hexdigest(),
        'verdict': result['verdict'],
        'browser': version['Browser'],
        'v8': version['V8-Version'],
    }


def comparison(baseline, candidate, *, legacy=False, allowed_changes=('asset/wasm',)):
    runs = baseline + candidate
    if not baseline or not candidate:
        raise ValueError('both arms require captures')
    if not legacy:
        for name, arm in (('baseline', baseline), ('candidate', candidate)):
            identities = [run.get('provenance', {}).get('sha256', {}) if run.get('provenance') else {} for run in arm]
            if any(not identity.get('asset/wasm') for identity in identities):
                raise ValueError(f'{name}: missing build provenance')
            if any(identity != identities[0] for identity in identities):
                raise ValueError(f'{name}: mixed build or input provenance')
        before, after = (arm[0]['provenance']['sha256'] for arm in (baseline, candidate))
        for key in before.keys() | after.keys():
            if key not in allowed_changes and before.get(key) != after.get(key):
                raise ValueError(f'undeclared changed input: {key}')
    for field in ('instructions', 'consoleSha256', 'verdict', 'browser', 'v8'):
        if len({run[field] for run in runs}) != 1:
            raise ValueError(f'Unmatched {field}; inspect the runs before comparing performance')
    # Legacy captures have no provenance. New captures must use identical guest
    # inputs; WASM and JavaScript hashes are retained because those may vary.
    if any(run.get('provenance') is not None for run in runs):
        for name in ('rom', 'bootloader', 'ptable', 'app', 'elf'):
            hashes = [(run.get('provenance') or {}).get('sha256', {}).get(f'asset/{name}')
                      for run in runs]
            if None in hashes or len(set(hashes)) != 1:
                raise ValueError(f'Missing or unmatched firmware hash: {name}')

    def metric(get):
        before, after = [get(run) for run in baseline], [get(run) for run in candidate]
        a, b = statistics.median(before), statistics.median(after)
        return {'baseline': before, 'candidate': after, 'baselineMedian': a,
                'candidateMedian': b, 'lessWallTimePercent': (1 - b / a) * 100 if a else None}

    return {
        'baseline': baseline,
        'candidate': candidate,
        'total': metric(lambda run: run['wallSeconds']),
        'intervals': {name: metric(lambda run: run['intervalSeconds'][name]) for name, _ in MILESTONES},
        'scope': 'Host wall time between firmware console milestones; includes setup and console delivery. '
                 'Not isolated function timings, input latency or silicon cycle accuracy. '
                 'Matching firmware/build inputs and absence of profiling must also be verified from capture provenance.',
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', nargs='+', required=True, help='Capture directories, one per run')
    parser.add_argument('--candidate', nargs='+', required=True, help='Capture directories, one per run')
    parser.add_argument('--legacy', action='store_true', help='Inspect older captures without certifying their capture mode or build identity')
    parser.add_argument('--screening', action='store_true', help='Allow fewer than three pairs; not a production speed claim')
    parser.add_argument('--allow-change', action='append', default=['asset/wasm'], help='Explicit provenance key allowed to differ between arms')
    args = parser.parse_args()
    if not args.screening and (len(args.baseline) != len(args.candidate) or len(args.baseline) < 3):
        parser.error('production comparison requires at least three matched pairs; use --screening for exploration')
    try:
        result = comparison([read_run(p, legacy=args.legacy) for p in args.baseline], [read_run(p, legacy=args.legacy) for p in args.candidate], legacy=args.legacy, allowed_changes=args.allow_change)
        result['legacy'] = args.legacy
        result['screeningOnly'] = args.screening
    except (ValueError, KeyError) as error:
        parser.error(str(error))
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
