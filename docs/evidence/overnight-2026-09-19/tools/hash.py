import importlib.util,sys
from pathlib import Path
spec=importlib.util.spec_from_file_location('c','/Users/sarah/src/a/esp32sim/work/night/tools/browser-benchmark/compare-runs.py')
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
for d in sys.argv[1:]:
    try:
        r=m.read_run(Path(d)); print(d.split('/')[-1], r['consoleSha256'][:16], r.get('instructions'))
    except Exception as e: print(d, 'ERR', e)
