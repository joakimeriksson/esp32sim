import fs from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import os from 'node:os';
const root=process.cwd(), assets=root+'/../assets/pocket-tank';
const args=['--rom',assets+'/rom.elf','--board','waveshare-amoled18-v2','--boot','rom','--no-dump','--console','uart0','--flash-mb','16','--psram-mb','8','--bootloader',assets+'/bootloader.bin','--ptable',assets+'/ptable.bin','--app',assets+'/app.bin','--flash-at','0x290000='+assets+'/model.bin','--max-seconds','30'];
const sha=x=>createHash('sha256').update(x).digest('hex');
const bins={baseline:root+'/candidate/target/release/esp32sim',candidate:root+'/final/target/release/esp32sim'};
const inputs=Object.fromEntries(['rom.elf','bootloader.bin','ptable.bin','app.bin','model.bin'].map(n=>[n,sha(fs.readFileSync(assets+'/'+n))]));
const identities=Object.fromEntries(Object.entries(bins).map(([n,p])=>[n,{path:p,sha256:sha(fs.readFileSync(p))}]));
const out=root+'/runs/final-native-pocket-tank';fs.mkdirSync(out);
const env={...process.env};for(const key of Object.keys(env))if(key.startsWith('ESP32SIM_')||key.startsWith('ESP_EMU_'))delete env[key];
const rows=[];
for(let pair=1;pair<=2;pair++)for(const arm of pair%2?['baseline','candidate']:['candidate','baseline']){
 const loadBefore=os.loadavg(),start=performance.now();
 const r=spawnSync(bins[arm],args,{env,encoding:'utf8',timeout:300000,maxBuffer:8e6});
 const wallSeconds=(performance.now()-start)/1000;
 fs.writeFileSync(`${out}/${pair}-${arm}.stdout`,r.stdout||'');fs.writeFileSync(`${out}/${pair}-${arm}.stderr`,r.stderr||'');
 if(r.status!==0)throw Error('exit '+r.status+' '+r.error);
 const work=r.stderr.match(/core0 (\d+) \+ core1 (\d+) insns/), jit=r.stderr.match(/\[emu\] blocks: .*jit: (\d+) compiled, (\d+) KB code/);
 if(!work||!jit||Number(jit[1])===0||r.stderr.includes('APPROXIMATE timing'))throw Error('missing work/JIT or approximate timing');
 const row={pair,arm,wallSeconds,loadBefore,coreInstructions:work.slice(1),jitLine:jit[0],consoleSha256:sha(r.stdout)};rows.push(row);
 fs.writeFileSync(out+'/runs.json',JSON.stringify({args,inputs,identities,rows},null,2));console.log(JSON.stringify(row));
 for(const key of ['coreInstructions','consoleSha256'])if(JSON.stringify(rows[0][key])!==JSON.stringify(row[key]))throw Error('unmatched '+key);
}
const medians=Object.fromEntries(Object.keys(bins).map(arm=>[arm,rows.filter(r=>r.arm===arm).reduce((s,r)=>s+r.wallSeconds,0)/2]));
fs.writeFileSync(out+'/summary.json',JSON.stringify({args,inputs,identities,rows,medianWallSeconds:medians,wallReductionPercent:100*(1-medians.candidate/medians.baseline),throughputRatio:medians.baseline/medians.candidate},null,2)+'\n');
