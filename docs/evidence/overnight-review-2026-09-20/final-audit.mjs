import fs from 'node:fs';
import assert from 'node:assert/strict';
const root=process.argv[2];
const b=JSON.parse(fs.readFileSync(root+'/runs/final-pocket-tank/summary.json'));
assert.equal(b.runs.length,4);
const reference=b.runs[0];
for(const r of b.runs){assert.equal(r.instructions,10073833775);assert.equal(r.passed,true);assert.equal(r.jitFailed,0);assert.equal(r.consoleSha256,reference.consoleSha256);assert.equal(r.browser,reference.browser);assert.equal(r.v8,reference.v8);for(const [k,v] of Object.entries(reference.provenance.sha256))if(k!=='asset/wasm')assert.equal(r.provenance.sha256[k],v);}
const builds=Object.fromEntries(['baseline','candidate'].map(arm=>[arm,JSON.parse(fs.readFileSync(`${root}/runs/final-pocket-tank/${arm}/build.json`))]));
for(const r of b.runs)assert.equal(r.provenance.sha256['asset/wasm'],builds[r.arm].wasmSha256);
const n=JSON.parse(fs.readFileSync(root+'/runs/final-native-pocket-tank/summary.json'));
assert.equal(n.rows.length,4);
for(const r of n.rows){assert.deepEqual(r.coreInstructions,n.rows[0].coreInstructions);assert.equal(r.consoleSha256,n.rows[0].consoleSha256);}
const output={passed:true,baselineCommit:builds.baseline.artifactBuild.commit,candidateCommit:builds.candidate.artifactBuild.commit,browser:{medianWallSeconds:b.medianWallSeconds,wallReductionPercent:b.wallReductionPercent,pairsWallReductionPercent:b.pairsWallReductionPercent,throughputRatio:b.medianWallSeconds.baseline/b.medianWallSeconds.candidate},native:{medianWallSeconds:n.medianWallSeconds,wallReductionPercent:n.wallReductionPercent,throughputRatio:n.throughputRatio}};
fs.writeFileSync(root+'/final-audit.json',JSON.stringify(output,null,2)+'\n');console.log(JSON.stringify(output,null,2));
