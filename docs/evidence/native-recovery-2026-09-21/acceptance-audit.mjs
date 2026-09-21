import fs from 'node:fs';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const root=process.argv[2];
const sha=x=>createHash('sha256').update(x).digest('hex');
const median=a=>{a.sort((a,b)=>a-b);return(a[Math.floor((a.length-1)/2)]+a[Math.floor(a.length/2)])/2;};
function native(name,pairs,expected=['4446180089','5627653686'],consoleHash='9e8a66e483f741c80db1bd20c62bc43943697c2e6a0053a6af1f7d52af8c67dd'){
 const dir=root+'/acceptance/'+name,d=JSON.parse(fs.readFileSync(dir+'/summary.json'));
 assert.equal(d.rows.length,pairs*2);
 for(const r of d.rows){assert.deepEqual(r.coreInstructions,expected);assert.equal(r.consoleSha256,consoleHash);assert.equal(sha(fs.readFileSync(`${dir}/${r.pair}-${r.arm}.stdout`)),r.consoleSha256);}
 const m=Object.fromEntries(['baseline','candidate'].map(a=>[a,median(d.rows.filter(r=>r.arm===a).map(r=>r.wallSeconds))]));
 return {medianWallSeconds:m,wallReductionPercent:100*(1-m.candidate/m.baseline),throughputRatio:m.baseline/m.candidate,pairsWallReductionPercent:Array.from({length:pairs},(_,i)=>{const p=i+1;return 100*(1-d.rows.find(r=>r.pair===p&&r.arm==='candidate').wallSeconds/d.rows.find(r=>r.pair===p&&r.arm==='baseline').wallSeconds);}),binarySha256:Object.fromEntries(Object.entries(d.identities).map(([k,v])=>[k,v.sha256]))};
}
function browser(name,expected){
 const d=JSON.parse(fs.readFileSync(root+'/acceptance/'+name+'/summary.json'));assert.equal(d.runs.length,4);const first=d.runs[0];
 for(const r of d.runs){assert.equal(r.instructions,expected);assert.equal(r.passed,true);assert.equal(r.jitFailed,0);assert.equal(r.consoleSha256,first.consoleSha256);assert.equal(r.browser,first.browser);assert.equal(r.v8,first.v8);for(const[k,v]of Object.entries(first.provenance.sha256))if(k!=='asset/wasm')assert.equal(r.provenance.sha256[k],v);}
 const m=Object.fromEntries(['baseline','candidate'].map(a=>[a,median(d.runs.filter(r=>r.arm===a).map(r=>r.wallSeconds))]));assert.deepEqual(m,d.medianWallSeconds);assert.equal(d.wallReductionPercent,100*(1-m.candidate/m.baseline));
 return {medianWallSeconds:d.medianWallSeconds,wallReductionPercent:d.wallReductionPercent,throughputRatio:d.medianWallSeconds.baseline/d.medianWallSeconds.candidate,pairsWallReductionPercent:d.pairsWallReductionPercent,consoleSha256:first.consoleSha256,browser:first.browser,v8:first.v8};
}
const control=native('control-main-recovery',2);assert.equal(control.binarySha256.baseline,control.binarySha256.candidate);
const comparison=native('confirm-main-recovery',4);assert.equal(comparison.binarySha256.baseline,control.binarySha256.baseline);
const result={workOutputChecksPassed:true,nativeMedianAtLeastMain:comparison.medianWallSeconds.candidate<=comparison.medianWallSeconds.baseline,control,native:comparison,panel:native('confirm-panel-recovery',2,['260026792','136442769'],'a77aaabb68350611617f518acdf4687b919906b7439ef327d816239c62b683d9'),browserPreservation:{pocketTank:browser('recovery-pocket-tank',10073833775),tinyDraw:browser('recovery-tinydraw',9819885134)},directMainBrowser:{pocketTank:browser('main-recovery-pocket-tank',10073833775),tinyDraw:browser('main-recovery-tinydraw',9819885134)}};
fs.writeFileSync(root+'/acceptance-audit.json',JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result,null,2));
