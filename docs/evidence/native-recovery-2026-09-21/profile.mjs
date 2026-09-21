import fs from 'node:fs';
import {spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
const root=process.cwd(),out=root+'/profiles';fs.mkdirSync(out,{recursive:true});
const prev=JSON.parse(fs.readFileSync(root+'/runs/native-pocket-tank/summary.json'));
const sha=x=>createHash('sha256').update(x).digest('hex');
const env={...process.env};for(const k of Object.keys(env))if(k.startsWith('ESP32SIM_')||k.startsWith('ESP_EMU_'))delete env[k];
const records=[];
for(const arm of ['baseline','final']){
 const binary=root+'/'+arm+'/target/release/esp32sim';
 const child=spawn(binary,prev.args,{env});let stdout='',stderr='';child.stdout.on('data',x=>stdout+=x);child.stderr.on('data',x=>stderr+=x);
 const prof=spawn('/usr/bin/sample',[String(child.pid),'65','1','-mayDie','-file',out+'/'+arm+'.sample.txt']);let profilerLog='';prof.stdout.on('data',x=>profilerLog+=x);prof.stderr.on('data',x=>profilerLog+=x);
 const exit=await new Promise(r=>child.on('close',r));const profilerExit=await new Promise(r=>prof.on('close',r));
 fs.writeFileSync(out+'/'+arm+'.stdout',stdout);fs.writeFileSync(out+'/'+arm+'.stderr',stderr);fs.writeFileSync(out+'/'+arm+'.profiler.txt',profilerLog);
 const coreInstructions=stderr.match(/core0 (\d+) \+ core1 (\d+) insns/)?.slice(1);
 if(exit!==0||JSON.stringify(coreInstructions)!==JSON.stringify(prev.rows[0].coreInstructions)||sha(stdout)!==prev.rows[0].consoleSha256)throw Error('profile exactness failed '+arm);
 records.push({arm,binary,binarySha256:sha(fs.readFileSync(binary)),exit,profilerExit,coreInstructions,consoleSha256:sha(stdout)});
 fs.writeFileSync(out+'/summary.json',JSON.stringify({diagnosticOnly:true,args:prev.args,records},null,2)+'\n');
}
