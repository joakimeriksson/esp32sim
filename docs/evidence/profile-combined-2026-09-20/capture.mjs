import fs from 'node:fs/promises';
import {spawn} from 'node:child_process';
const root='/Users/alice/src/a/esp32sim-exp/results/profile-combined-0920';
const tree='/Users/alice/src/a/esp32sim-profile-0920';
const mode=process.argv[2] || 'release';
const wasm=root+'/'+mode+'.wasm';
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function run(cmd,args,out){const f=await fs.open(out,'w');try{await new Promise((resolve,reject)=>{const p=spawn(cmd,args,{cwd:tree,stdio:['ignore',f.fd,f.fd]});p.on('error',reject);p.on('exit',c=>c===0?resolve():reject(Error(cmd+' exit '+c)));});}finally{await f.close();}}
const base='/Users/alice/src/a/esp32sim-exp/m3-transfer';
const pocket=JSON.parse(await fs.readFile(base+'/assets/pocket-tank/assets.json','utf8'));
for(const k of Object.keys(pocket))if(k!=='workload')pocket[k]=base+'/assets/pocket-tank/'+pocket[k];
pocket.wasm=wasm;
const tiny={wasm,rom:pocket.rom,bootloader:base+'/handtest-assets/tinydraw-2.3.0/bootloader.bin',ptable:base+'/handtest-assets/tinydraw-2.3.0/ptable.bin',app:base+'/handtest-assets/tinydraw-2.3.0/app.bin',elf:'/Users/alice/src/a/tinydraw/out/build/release-230-esp32-demo/tinydraw_esp32.elf'};
for(const [name,assets,captureMode,page] of [['pocket-tank',pocket,'battery','battery.html'],['tinydraw-latest',tiny,'drawing','response.html']]){
 if(process.env.ONLY_WORKLOAD && process.env.ONLY_WORKLOAD!==name)continue;
 const out=root+'/'+mode+'-'+name+(process.env.RUN_SUFFIX||'');await fs.mkdir(out,{recursive:true});await fs.writeFile(out+'/assets.json',JSON.stringify(assets,null,2));
 const server=spawn('uv',['run','--offline','--no-project','--python',root+'/.venv/bin/python','python',root+'/harness/serve.py',out+'/assets.json','--port','8796','--web-root',tree],{stdio:'ignore'});
 const chrome=spawn('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',['--headless=new','--no-first-run','--disable-background-timer-throttling','--disable-renderer-backgrounding','--remote-debugging-address=127.0.0.1','--remote-debugging-port=9236','--user-data-dir='+out+'/chrome','about:blank'],{stdio:'ignore'});
 try{let ready=false;for(let i=0;i<100;i++){try{await fetch('http://127.0.0.1:9236/json/version');await fetch('http://127.0.0.1:8796/assets.json');ready=true;break;}catch{await sleep(200);}}if(!ready)throw Error('startup timeout');
 console.log('Capturing '+mode+' '+name);await run(process.execPath,[root+'/harness/capture-cpu.mjs','http://127.0.0.1:8796/'+page,out,'9236',captureMode],out+'/capture.log');
 await run('uv',['run','--offline','--no-project','--python',root+'/.venv/bin/python','python',root+'/harness/summarize-cpu.py',out+'/'+captureMode+'.cpuprofile'],out+'/summary.txt');console.log(await fs.readFile(out+'/summary.txt','utf8'));
 }finally{chrome.kill();server.kill();await sleep(1500);}
}
