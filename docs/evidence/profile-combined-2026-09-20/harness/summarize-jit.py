"""Join diagnostic guest-PC CPU samples and sampled block coverage to ELF symbols.
Usage: summarize-jit.py EVENTS_JSON CPU_PROFILE NM_SYMBOLS OUTPUT_DIRECTORY
NM_SYMBOLS is `xtensa-esp-elf-nm -n -C` output for the measured app and ROM.
"""
import bisect
import collections
import json
import gzip
from pathlib import Path
import re
import sys

events_path, cpu_path, symbol_path, output = map(Path, sys.argv[1:])
output.mkdir(parents=True, exist_ok=True)
symbols = []
for line in symbol_path.read_text().splitlines():
    p = line.split(' ', 2)
    if len(p) == 3 and p[1] in ('t', 'T', 'w', 'W'):
        try:
            symbols.append((int(p[0], 16), p[2]))
        except ValueError:
            pass
symbols.sort()
addresses = [a for a, _ in symbols]
def symbol(pc):
    i = bisect.bisect_right(addresses, pc) - 1
    return symbols[i][1] if i >= 0 and pc - addresses[i] < 65536 else '?'

rows = []
events = json.loads(gzip.decompress(events_path.read_bytes()) if events_path.suffix == '.gz' else events_path.read_text())
for event in events if isinstance(events, list) else []:
    if event.get('type') != 'log':
        continue
    for line in event['line'].splitlines():
        p = line.split('\t')
        if len(p) != 7 or p[0] == 'pc':
            continue
        pc = int(p[0], 16)
        missing = p[5].split(',') if p[5] else []
        ops = p[6].split(',')
        # Returns are admitted as final helpers; the raw emitter-support column
        # alone is not a compilation rejection reason.
        if len(ops) > 1 and ops[-1] in ('Ret', 'RetN', 'Retw', 'RetwN'):
            missing.remove(ops[-1])
        rows.append(dict(pc=p[0], symbol=symbol(pc), jit=p[1] == 'true', samples=int(p[2]),
                         instructions=int(p[3]), missing=missing, ops=ops))
(output / 'coverage.json').write_text(json.dumps(rows, indent=2))
profile = json.loads(cpu_path.read_text())
nodes = {n['id']: n for n in profile['nodes']}
parents = {c: n['id'] for n in profile['nodes'] for c in n.get('children', [])}
self_pc, inclusive_pc = collections.Counter(), collections.Counter()
for sample, delta in zip(profile['samples'], profile['timeDeltas'], strict=True):
    at = sample
    while at in nodes:
        match = re.fullmatch(r'xtensa_([0-9a-f]{8})', nodes[at]['callFrame']['functionName'])
        if match:
            pc = int(match[1], 16)
            inclusive_pc[pc] += delta
            if at == sample:
                self_pc[pc] += delta
            break
        at = parents.get(at)
functions = collections.Counter()
for pc, us in inclusive_pc.items():
    functions[symbol(pc)] += us
cpu_rows = [dict(pc=f'{pc:08x}', symbol=symbol(pc), selfSeconds=self_pc[pc]/1e6,
                 includingHelpersSeconds=us/1e6) for pc, us in inclusive_pc.most_common()]
(output / 'generated-cpu.json').write_text(json.dumps(cpu_rows, indent=2))
lines = ['CPU samples in named generated blocks, including descendants (not dispatch):']
lines += [f'{us/1e6:.3f} s {name}' for name, us in functions.most_common(20)]
if rows:
    total = sum(r['instructions'] for r in rows)
    rejected = [r for r in rows if not r['jit']]
    missing_sets = collections.Counter()
    funcs = collections.Counter()
    for r in rejected:
        missing_sets[','.join(sorted(set(r['missing']))) or '(supported/cold/single)'] += r['instructions']
        funcs[r['symbol']] += r['instructions']
    lines += [f'\nSampled guest instructions: {total}; interpreted: {sum(r["instructions"] for r in rejected)}',
              'Top interpreted missing-opcode sets; percentages are guest frequency, NOT CPU time:']
    lines += [f'{count:8d} {count/total:6.2%} {name}' for name, count in missing_sets.most_common(25)]
    floating = sum(r['instructions'] for r in rejected if any(op.endswith('S') or op in ('Wfr', 'Rfr') for op in r['missing']))
    lines.append(f'Blocks with missing floating-point operations: {floating} ({floating/total:.2%} of sampled instructions); may have other blockers')
    lines += ['\nTop interpreted guest functions:']
    lines += [f'{count:8d} {count/total:6.2%} {name}' for name, count in funcs.most_common(20)]
    small = {'Abs', 'Sll', 'Srl', 'Sra', 'Muluh', 'Mulsh', 'Src'}
    for label, bundle in [('small integer ops', small), ('Entry', {'Entry'}), ('Entry + small integer ops', small | {'Entry'})]:
        count = sum(r['instructions'] for r in rejected if r['missing'] and set(r['missing']) <= bundle and len(r['ops']) > 1)
        lines.append(f'Potential newly eligible with {label}: {count} ({count/total:.2%} of sampled instructions); no speedup prediction')
text = '\n'.join(lines) + '\n'
(output / 'summary.txt').write_text(text)
print(text)
