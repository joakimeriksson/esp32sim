"""Summarize exclusive CPU samples by function and execution component."""
import collections
import json
import pathlib
import sys

profile = json.loads(pathlib.Path(sys.argv[1]).read_text())
nodes = {n['id']: n for n in profile['nodes']}
functions = collections.Counter()
categories = collections.Counter()
for sample, delta in zip(profile['samples'], profile['timeDeltas'], strict=True):
    frame = nodes[sample]['callFrame']
    name, url = frame['functionName'], frame['url']
    functions[(name, url)] += delta
    if url.startswith('wasm:') and 'esp32sim_wasm' not in url:
        category = 'generated blocks'
    elif 'run_block_inner' in name:
        category = 'block dispatch / interpreted loop'
    elif 'step_blocks' in name:
        category = 'machine block wrapper'
    elif '7Machine' in name and 'E3run' in name:
        category = 'machine run (including remaining inlined code)'
    elif 'exec9exec_insn' in name:
        category = 'interpreter instruction execution'
    elif '10xtensa_lx73pie' in name or name in ('__ashlti3', '__lshrti3'):
        category = 'PIE and 128-bit helpers'
    elif 'jit6native3run' in name:
        category = 'JIT invocation wrapper'
    else:
        category = 'other'
    categories[category] += delta

total = sum(categories.values())
print(f'Sampled interval: {total / 1e6:.3f} s; exclusive samples, not inclusive call-tree totals.')
for name, us in categories.most_common():
    print(f'{us / 1e6:8.3f} s {100 * us / total:5.1f}%  {name}')
print('\nLargest exclusive functions:')
for (name, url), us in functions.most_common(30):
    print(f'{us / 1e6:8.3f} s  {name}  {url}')
