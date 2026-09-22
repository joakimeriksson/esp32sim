import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=process.argv[2]||path.dirname(fileURLToPath(import.meta.url));
const read=p=>JSON.parse(fs.readFileSync(path.join(root,p)));
const check=(v,m)=>{if(!v)throw Error(m);};
const close=(a,b,m)=>check(Number.isFinite(a)&&Number.isFinite(b)&&Math.abs(a-b)<1e-9,m);
const median=a=>{a=[...a].sort((x,y)=>x-y);return a.length%2?a[a.length>>1]:(a[a.length/2-1]+a[a.length/2])/2;};
const index=read('results.json'),provenance=read('provenance.json');let count=0,browser=null,platform=null,v8=null;
check(index.jobs.length===4,'job count');
for(const job of index.jobs){
 const x=read(job.name+'.json'),same=job.name.startsWith('control-'),wantPairs=same?3:4;
 check(['pocket-tank','tinydraw'].includes(x.workload),'known workload');
 check(job.workload===x.workload&&job.pairs===x.pairs,'index workload/pairs');
 check(x.pairs===wantPairs&&x.runs.length===wantPairs*2,'pair count');
 const expected=x.workload==='pocket-tank'?{instructions:10073833775,frames:737}:{instructions:9819885134,frames:428};
 let consoleHash=null,inputHashes=null;
 for(let i=0;i<x.runs.length;i++){
  const row=x.runs[i],r=read(row.receipt),pair=(i>>1)+1,arm=(pair%2?['baseline','candidate']:['candidate','baseline'])[i%2];
  check(row.pair===r.pair&&row.arm===r.arm,'row identity');
  check(row.instructions===r.instructions&&row.frames===r.frames&&row.consoleSha256===r.consoleSha256,'row work');
  for(const [key,value] of Object.entries(r.inputHashes)){const name=(x.workload==='pocket-tank'?'pocket':'tinydraw')+'-'+key.slice(6);const expectedInput=Object.entries(provenance.inputs).find(([k])=>k.startsWith('assets/'+name+'.'));check(expectedInput&&value===expectedInput[1].sha256,'input provenance');}
  const inputCount=Object.keys(provenance.inputs).filter(k=>k.startsWith('assets/'+(x.workload==='pocket-tank'?'pocket':'tinydraw')+'-')).length;check(Object.keys(r.inputHashes).length===inputCount,'input count');
  if(browser===null){browser=r.browser;platform=r.platform;v8=r.v8;}else check(browser===r.browser&&platform===r.platform&&v8===r.v8,'environment consistency');
  check(r.pair===pair&&r.arm===arm&&r.workload===x.workload,'order/workload');
  check(r.instructions===expected.instructions&&r.frames===expected.frames,'pinned work');
  check(r.passed&&r.status==='completed'&&r.stopCode===0&&r.jit.failed===0&&r.jit.compiled>0,'completion/JIT');
  check(r.verdictValidation.valid&&r.verdictValidation.passed&&r.verdictValidation.error===null,'verdict');
  check(r.wasmSha256===provenance.builds[same||arm==='baseline'?'baseline':'candidate'].wasmSha256,'artifact identity');
  check(/^[0-9a-f]{64}$/.test(r.consoleSha256),'console digest');
  if(consoleHash===null)consoleHash=r.consoleSha256;else check(consoleHash===r.consoleSha256,'console mismatch');
  if(inputHashes===null)inputHashes=JSON.stringify(r.inputHashes);else check(inputHashes===JSON.stringify(r.inputHashes),'inputs mismatch');
  close(row.wallSeconds,r.wallSeconds,'summary wall');check(r.wallSeconds>0,'wall positive');
  close(r.realtimeRatio,r.guestSeconds/r.wallSeconds,'realtime ratio');count++;
 }
 const med=Object.fromEntries(['baseline','candidate'].map(a=>[a,median(x.runs.filter(r=>r.arm===a).map(r=>r.wallSeconds))]));
 for(const a of Object.keys(med)){close(x.medianWallSeconds[a],med[a],'median');close(job.medianWallSeconds[a],med[a],'index median');}
 check(JSON.stringify(job.pairWallReductionPercent)===JSON.stringify(x.pairWallReductionPercent),'index pair list');
 close(x.wallReductionPercent,100*(1-med.candidate/med.baseline),'reduction');
 close(job.wallReductionPercent,x.wallReductionPercent,'index reduction');
 for(let p=1;p<=wantPairs;p++)close(x.pairWallReductionPercent[p-1],100*(1-x.runs.find(r=>r.pair===p&&r.arm==='candidate').wallSeconds/x.runs.find(r=>r.pair===p&&r.arm==='baseline').wallSeconds),'pair reduction');
}
const exact=read('pocket-exact30.json'),c=exact.contract;
check(c.exact&&c.referencePanics===0&&c.candidatePanics===0&&c.referenceJitFailures===0&&c.candidateJitFailures===0,'exact zero errors');
check(c.referenceAndCandidateInsns==='10073833775'&&c.referenceAndCandidateFrames===737,'exact work');
check(c.referenceAndCandidateConsoleSha256==='b9d9966e5d9d73984203c11846eb7f8c86507cb53ffe133c4e7e92dff66319d7'&&c.referenceAndCandidateFramesSha256==='71a5ccdbf9dccbb35775c0d1ce90a2fad3f46370b901c39e4f3d58798537ca7d','exact hashes');
for(const arm of ['baseline','candidate'])check(exact[arm].wasmSha256===provenance.builds[arm].wasmSha256,'exact artifacts');
const code=read('final-code-comparison.json'),data=read('final-data-comparison.json');
check(code.changedBodies.length===0&&code.beforeFunctions===1362&&code.afterFunctions===1362,'final code bodies');
check(code.sections.filter(s=>!s.identical).length===1&&code.sections.find(s=>!s.identical).id===11,'final data-only section');
check(data.allDifferencesAreSourceLineRecords&&data.dataSegmentLayoutUnchanged&&data.filenamesPointersLengthsAndColumnsUnchanged&&data.changedBytes===28,'final diagnostic locations');
check(code.beforeSha256===provenance.builds.candidate.wasmSha256&&data.beforeSha256===code.beforeSha256&&data.afterSha256===code.afterSha256,'final identities');
check(count===28&&index.totalArms===count,'total arms');
console.log(JSON.stringify({status:'passed',jobs:4,arms:count,checks:['order','pinned instructions and frames','console and input equality','artifact identity','verdict and JIT','medians and pair reductions']},null,2));
