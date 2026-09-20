import fs from 'node:fs';
import assert from 'node:assert/strict';
const root=process.argv[2];
const runs={};
for(const name of ['pocket-tank','tinydraw']){
 const d=JSON.parse(fs.readFileSync(`${root}/runs/${name}/summary.json`));
 const expected=name==='pocket-tank'?10073833775:9819885134;
 assert.equal(d.runs.length,4);
 const first=d.runs[0];
 for(const r of d.runs){assert.equal(r.instructions,expected);assert.equal(r.passed,true);assert.equal(r.jitFailed,0);assert.equal(r.consoleSha256,first.consoleSha256);assert.equal(r.browser,first.browser);assert.equal(r.v8,first.v8);for(const [k,v] of Object.entries(first.provenance.sha256))if(k!=='asset/wasm')assert.equal(r.provenance.sha256[k],v);}
 const build=Object.fromEntries(['baseline','candidate'].map(arm=>[arm,JSON.parse(fs.readFileSync(`${root}/runs/failed-old-glue/${arm}/build.json`))]));
 for(const r of d.runs)assert.equal(r.provenance.sha256['asset/wasm'],build[r.arm].wasmSha256);
 runs[name]={medianWallSeconds:d.medianWallSeconds,wallReductionPercent:d.wallReductionPercent,throughputRatio:d.medianWallSeconds.baseline/d.medianWallSeconds.candidate,pairsWallReductionPercent:d.pairsWallReductionPercent,instructions:expected,consoleSha256:first.consoleSha256,browser:first.browser,v8:first.v8,baselineCommit:build.baseline.commit,candidateCommit:build.candidate.commit,baselineWasm:build.baseline.wasmSha256,candidateWasm:build.candidate.wasmSha256};
}
const n=JSON.parse(fs.readFileSync(`${root}/runs/native-pocket-tank/summary.json`));assert.equal(n.rows.length,4);for(const r of n.rows){assert.deepEqual(r.coreInstructions,n.rows[0].coreInstructions);assert.equal(r.consoleSha256,n.rows[0].consoleSha256);}
runs.nativePocketTank={medianWallSeconds:n.medianWallSeconds,wallReductionPercent:n.wallReductionPercent,throughputRatio:n.throughputRatio,coreInstructions:n.rows[0].coreInstructions,consoleSha256:n.rows[0].consoleSha256};
fs.writeFileSync(root+'/audit.json',JSON.stringify({passed:true,runs},null,2)+'\n');console.log(JSON.stringify(runs,null,2));
