import fs from 'node:fs';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const root=process.argv[2],results={};
const sha=x=>createHash('sha256').update(x).digest('hex');
const median=a=>{a.sort((a,b)=>a-b);return(a[Math.floor((a.length-1)/2)]+a[Math.floor(a.length/2)])/2;};
for(const name of fs.readdirSync(root+'/screens')){
 const dir=root+'/screens/'+name;if(!fs.existsSync(dir+'/summary.json'))continue;
 const d=JSON.parse(fs.readFileSync(dir+'/summary.json'));
 assert.equal(d.rows.length,4);
 for(const r of d.rows){assert.deepEqual(r.coreInstructions,['4446180089','5627653686']);assert.equal(r.consoleSha256,'9e8a66e483f741c80db1bd20c62bc43943697c2e6a0053a6af1f7d52af8c67dd');assert.equal(sha(fs.readFileSync(`${dir}/${r.pair}-${r.arm}.stdout`)),r.consoleSha256);}
 const m=Object.fromEntries(['baseline','candidate'].map(arm=>[arm,median(d.rows.filter(r=>r.arm===arm).map(r=>r.wallSeconds))]));assert.deepEqual(m,d.medianWallSeconds);
 results[name]={medianWallSeconds:m,wallReductionPercent:100*(1-m.candidate/m.baseline),pairsWallReductionPercent:[1,2].map(p=>100*(1-d.rows.find(r=>r.pair===p&&r.arm==='candidate').wallSeconds/d.rows.find(r=>r.pair===p&&r.arm==='baseline').wallSeconds)),binarySha256:Object.fromEntries(Object.entries(d.identities).map(([k,v])=>[k,v.sha256]))};
}
fs.writeFileSync(root+'/audit.json',JSON.stringify({passed:true,screens:results},null,2)+'\n');console.log(JSON.stringify(results,null,2));
