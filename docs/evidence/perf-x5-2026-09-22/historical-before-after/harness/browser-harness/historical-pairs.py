"""Private common-harness historical firmware comparison; never assumes equal retired work."""
import argparse, hashlib, importlib.util, json, math, re, shutil, statistics
from pathlib import Path
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('pairs', HERE/'run-pairs.py'); pairs=importlib.util.module_from_spec(spec); spec.loader.exec_module(pairs)
def require(ok, message):
    if not ok: raise ValueError(message)
def positive(v): return type(v) in (int,float) and math.isfinite(v) and v>0

def contract_read(path, workload, wasm):
    c=json.loads(path.read_text())
    require(c.get('qualified') is True, 'contract must explicitly be qualified')
    require(c.get('workload')==workload, 'contract workload mismatch')
    require(c.get('mode') in ('current-jit','historical-interpreter'), 'invalid capability mode')
    require(c.get('wasmSha256')==pairs.sha(wasm), 'contract artifact mismatch')
    require(type(c.get('instructions')) is int and c['instructions']>0, 'invalid instruction contract')
    require(type(c.get('frames')) is int and c['frames']>=0, 'invalid frame contract')
    require(isinstance(c.get('consoleSha256'),str) and re.fullmatch('[0-9a-f]{64}',c['consoleSha256']), 'invalid console contract')
    require(isinstance(c.get('inputSha256'),dict) and c['inputSha256'], 'missing qualified input hashes')
    if workload=='tinydraw': pairs.comparator.validate_verdict(c.get('verdict'))
    return c

def validate(raw, c, workload):
    r=raw['result'];require(raw.get('captureMode')=='timing', 'not timing capture')
    require(r.get('workload')==workload and r.get('instrumented') is False, 'wrong workload or diagnostic artifact')
    require(r.get('passed') is True and r.get('status')=='completed' and type(r.get('stopCode')) is int and r['stopCode']==0, 'firmware did not complete cleanly')
    require(r.get('instructions')==c['instructions'] and r.get('frames')==c['frames'], 'per-artifact work/frame contract mismatch')
    require(positive(r.get('wallSeconds')) and positive(r.get('guestSeconds')), 'invalid elapsed time')
    verdict_status=r.get('verdictValidation',{})
    require(verdict_status.get('passed') is True and verdict_status.get('valid') is True and verdict_status.get('error') is None and verdict_status.get('schema')==pairs.WORKLOADS[workload]['schema'], 'firmware verdict invalid')
    if workload=='tinydraw':
        pairs.comparator.validate_verdict(r.get('verdict'));require(r['verdict']==c['verdict'], 'qualified verdict mismatch')
    else:
        for wanted in pairs.WORKLOADS[workload]['checks']:
            got=next((v for v in r.get('checks',[]) if v.get('name')==wanted['name']),{})
            require(got.get('min')==wanted['min'] and type(got.get('count')) is int and got['count']>=wanted['min'], 'firmware check mismatch')
    j=r.get('jit',{});require(type(j.get('failed')) is int and j['failed']==0, 'missing/nonzero JIT failure count')
    cap=r.get('capabilities',{});require(cap.get('mode')==c['mode'], 'capability mode mismatch')
    if c['mode']=='current-jit':
        require(cap.get('hasJitSetter') is True and cap.get('hasJitCounter') is True and cap.get('executionMode')=='jit', 'production JIT unavailable')
        require(type(j.get('compiled')) is int and j['compiled']>0 and positive(j.get('instructions')), 'production JIT unused')
    else:
        require(cap.get('hasJitSetter') is False and cap.get('hasJitCounter') is False and cap.get('executionMode')=='interpreter', 'historical interpreter mislabeled')
        require(type(j.get('compiled')) is int and j['compiled']==0 and j.get('instructions') is None, 'invented interpreter JIT counters')
    hashes=r.get('provenance',{}).get('sha256',{})
    require(hashes.get('asset/wasm')==c['wasmSha256'], 'artifact identity mismatch')
    for name, expected in c['inputSha256'].items():
        require(hashes.get('asset/'+name)==expected, 'qualified input identity mismatch: '+name)
    return r
pairs.validate=validate

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('out',type=Path);p.add_argument('--assets',type=Path,required=True);p.add_argument('--common-tree',type=Path,required=True);p.add_argument('--chrome',required=True);p.add_argument('--pairs',type=int,default=3);p.add_argument('--warmup-pairs',type=int,default=1)
    for n in ('baseline','candidate'):
        p.add_argument('--'+n+'-tree',type=Path,required=True);p.add_argument('--'+n+'-wasm',type=Path,required=True);p.add_argument('--'+n+'-contract',type=Path,required=True)
    a=p.parse_args();require(a.pairs>0 and a.warmup_pairs>=0,'invalid pair counts');out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    paths=json.loads(a.assets.read_text());workload=paths.get('workload');require(workload in ('pocket-tank','tinydraw'),'only qualified canonical workloads')
    assets={k:str((a.assets.resolve().parent/paths[k]).resolve()) for k in pairs.WORKLOADS[workload]['assets']};assets['workload']=workload
    expected_inputs={k:pairs.sha(Path(v)) for k,v in assets.items() if k!='workload'}
    arms={};contracts={}
    for n in ('baseline','candidate'):
        wasm=getattr(a,n+'_wasm').resolve();c=contract_read(getattr(a,n+'_contract'),workload,wasm);require(c['inputSha256']==expected_inputs,'qualified firmware input hashes differ');contracts[n]=c
        # Reuse source/artifact preparation, then supply identical current runtime modules.
        arm=pairs.prepare(out,n,getattr(a,n+'_tree'),wasm,assets);arms[n]=arm
        for f in (arm/'web/wasm').rglob('*'):
            if f.is_file() and f.suffix in ('.js','.mjs'): f.rename(f.with_suffix(f.suffix+'.source-original'))
        for f in (a.common_tree/'web/wasm').rglob('*'):
            if f.is_file() and f.suffix in ('.js','.mjs'):
                target=arm/'web/wasm'/f.relative_to(a.common_tree/'web/wasm');target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(f,target)
        pairs.write_json(arm/'contract.json',c)
    rows=[];reference=None;identity={}
    for stage,count in [('warmup',a.warmup_pairs),('primary',a.pairs)]:
        for i in range(1,count+1):
            for n in (('baseline','candidate') if i%2 else ('candidate','baseline')):
                run=out/f'{stage}-{i}-{n}';print(f'{stage} pair {i}/{count} {n}',flush=True)
                row={'stage':stage,'pair':i,'arm':n,**pairs.capture(arms[n],run,a.chrome,contracts[n],workload)}
                require(row['consoleSha256']==contracts[n]['consoleSha256'],'per-artifact qualified console hash mismatch')
                hashes=row['provenance']['sha256'];nonwasm={k:v for k,v in hashes.items() if k!='asset/wasm'}
                if reference is None:reference=nonwasm
                require(nonwasm==reference,'non-WASM runtime/firmware/harness inputs changed')
                if n not in identity:identity[n]=row['provenance']
                require(row['provenance']==identity[n],'artifact provenance changed during campaign')
                require(not rows or (row['browser'],row['v8'])==(rows[0]['browser'],rows[0]['v8']),'browser engine changed')
                rows.append(row);pairs.write_json(out/'runs.json',rows)
    primary=[r for r in rows if r['stage']=='primary'];med={n:statistics.median(r['wallSeconds'] for r in primary if r['arm']==n) for n in arms}
    summary={'scope':'Same firmware inputs and validated per-artifact output contracts; instruction totals, frame cadence and console text may differ across historical versions. Not equal-retired-work speedup or isolated JIT attribution.','pairs':a.pairs,'warmupPairs':a.warmup_pairs,'medianWallSeconds':med,'wallReductionPercent':100*(1-med['candidate']/med['baseline']),'throughputRatioForFirmwareTask':med['baseline']/med['candidate'],'pairsWallReductionPercent':[100*(1-next(r['wallSeconds'] for r in primary if r['pair']==i and r['arm']=='candidate')/next(r['wallSeconds'] for r in primary if r['pair']==i and r['arm']=='baseline')) for i in range(1,a.pairs+1)],'contracts':contracts,'runs':rows}
    pairs.write_json(out/'summary.json',summary);print(json.dumps(summary,indent=2))
if __name__=='__main__':main()
