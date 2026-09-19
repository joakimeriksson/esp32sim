// Functional TinyDraw probe. Node/V8 execution is not a browser performance receipt.
import fs from 'node:fs/promises';
import {createJitHost} from '../web/wasm/jit.mjs';
import {completedVerdict,validateVerdict} from './browser-benchmark/verdict.mjs';
const [assetPath,cpiText='1',quantumText='64',secondsText='10',cacheText='0',frontiersText='0',contentionText='0',fillText=(process.env.CACHE_FILL ?? '120'),serviceText=fillText] = process.argv.slice(2);
const assets=JSON.parse(await fs.readFile(assetPath,'utf8'));
const cpi=Number(cpiText), quantum=Number(quantumText), seconds=Number(secondsText), cache=Number(cacheText);
const frontiers=Number(frontiersText);
const contention=Number(contentionText);
const fill=Number(fillText),service=Number(serviceText);
const enc=new TextEncoder(), dec=new TextDecoder();
let w, serial='', frames=0;
const host=createJitHost(()=>w);
const mem=()=>new Uint8Array(w.memory.buffer);
const logs=[];
w=(await WebAssembly.instantiate(await fs.readFile(new URL('../web/wasm/esp32sim.wasm',import.meta.url)),{env:{...host.imports,host_log:(p,n)=>logs.push(dec.decode(mem().subarray(p,p+n))),host_profile_now:()=>performance.now()}})).instance.exports;
const bytes=(b,f)=>{const p=w.esp32sim_alloc(b.length);mem().set(b,p);try{return f(p,b.length);}finally{w.esp32sim_free(p,b.length);}};
const emu=bytes(enc.encode('waveshare-amoled18-v2'),(p,n)=>w.esp32sim_new(p,n,16,8));
for(const [name,kind] of [['rom',0],['bootloader',1],['ptable',2],['app',3],['elf',4]]){
  const rc=bytes(await fs.readFile(assets[name]),(p,n)=>w.esp32sim_load(emu,kind,p,n));if(rc)throw Error(`${name}: ${rc}`);
}
if(cpi && w.esp32sim_set_approximate_jit_timing(emu,cpi,quantum))throw Error('timing config rejected');
if(frontiers && w.esp32sim_set_approximate_jit_frontiers(emu,frontiers))throw Error('frontiers config rejected');
if(cache && w.esp32sim_set_approximate_jit_cache(emu,fill,Number(process.env.CACHE_WRITEBACK ?? 96),cache===3?2:cache===2?1:0))throw Error('cache config rejected');
if(contention && w.esp32sim_set_approximate_cache_contention(emu,1))throw Error('contention config rejected');
if(service!==fill && w.esp32sim_set_approximate_cache_fill_service(emu,service))throw Error('service config rejected');
const pie=Number(process.env.PIE_TIMING ?? 0);
if(pie && w.esp32sim_set_approximate_pie_timing(emu,pie))throw Error('PIE timing config rejected');
if(w.esp32sim_boot(emu,0))throw Error('boot failed');
w.esp32sim_set_jit(emu,1);
const hz=w.esp32sim_cpu_hz(emu),start=performance.now();let stop=0;
let interrupted=false,slices=0;
process.on('SIGTERM',()=>{interrupted=true;});
process.on('SIGINT',()=>{interrupted=true;});
while(!interrupted && w.esp32sim_cycles(emu)<hz*seconds && performance.now()-start<120000){
 stop=w.esp32sim_run(emu,2000000,Date.now());
 const n=w.esp32sim_out_take(emu);
 for(let i=0;i<n;i++){
  if(w.esp32sim_out_kind(emu,i)!==1){frames++;continue;}
  const p=w.esp32sim_out_ptr(emu,i),len=w.esp32sim_out_len(emu,i);
  const msg=JSON.parse(dec.decode(mem().subarray(p,p+len)));
  if(msg.t==='serial' && msg.src==='usb')serial+=msg.data;
 }
 if(stop || /Guru Meditation|stack overflow|task_wdt/.test(serial) || logs.some(l=>/chip reset|panic/i.test(l)))break;
 if(/TINYDRAW_GATE1_AUTOMATED_DONE[^\r\n]*[\r\n]/.test(serial))break;
 if(++slices%64===0)await new Promise(setImmediate);
}
const schema=JSON.parse(await fs.readFile(new URL('./browser-benchmark/verdict-schema.json',import.meta.url),'utf8'));
const verdict=completedVerdict(serial,schema),verdictValidation=validateVerdict(verdict,schema);
const cacheCounters=cache?['hits','fills','writebacks','extraCycles'].map((name,i)=>[name,w.esp32sim_approximate_cache_counter(emu,i)]):[];
const cacheWait=[0,1].map(core=>w.esp32sim_approximate_cache_wait(emu,core));
console.log(JSON.stringify({mode:'Node functional smoke, not performance evidence',cpi,quantum,cache,frontiers,contention,fill,service,cacheWait,pie,pieCounters:pie?{events:w.esp32sim_approximate_pie_counter(emu,0),extraCycles:w.esp32sim_approximate_pie_counter(emu,1)}:null,cacheCounters:Object.fromEntries(cacheCounters),stop,interrupted,verdict,verdictValidation,guestSeconds:w.esp32sim_cycles(emu)/hz,wallSeconds:(performance.now()-start)/1000,instructions:w.esp32sim_insns(emu),jitInstructions:w.esp32sim_block_jit_insns(emu),jit:host.stats,frames,logs,serial},null,2));
w.esp32sim_delete(emu);
