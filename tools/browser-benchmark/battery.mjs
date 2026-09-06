import {validateVerdict, completedVerdict, optionalCounter} from './verdict.mjs';
import {createJitHost} from '/web/wasm/jit.mjs';
export async function runBattery(load, emit, jit = true, chain = false) {
  const enc = new TextEncoder(), dec = new TextDecoder();
  const schema = await (await fetch(new URL('./verdict-schema.json', import.meta.url))).json();
  const started = performance.now();
  let w, serial = '', frames = 0;
  const logs = [];
  const blockJit = createJitHost(() => w);
  const mem = () => new Uint8Array(w.memory.buffer);
  const hostLog = (p,n) => { const line = dec.decode(mem().subarray(p,p+n)); logs.push(line); emit({type:'log',line}); };
  const wasmBytes = await load('wasm');
  w = (await WebAssembly.instantiate(wasmBytes, {env:{...blockJit.imports,host_log:hostLog,host_profile_now:()=>performance.now()}})).instance.exports;
  const withBytes = (b,f) => {const p=w.esp32sim_alloc(b.length);mem().set(b,p);try{return f(p,b.length);}finally{w.esp32sim_free(p,b.length);}};
  const emu = withBytes(enc.encode('waveshare-amoled18-v2'),(p,n)=>w.esp32sim_new(p,n,16,8));
  if (!emu) throw Error('create failed');
  for (const [name,kind] of [['rom',0],['bootloader',1],['ptable',2],['app',3],['elf',4]]) {
    const rc=withBytes(await load(name),(p,n)=>w.esp32sim_load(emu,kind,p,n));
    if(rc)throw Error(`load ${name}: ${rc}`);
  }
  if(w.esp32sim_boot(emu,0))throw Error('boot failed');
  w.esp32sim_set_jit(emu, jit ? 1 : 0);
  w.esp32sim_set_block_chaining?.(emu, chain ? 1 : 0);
  const hz=w.esp32sim_cpu_hz(emu), executionStart=performance.now();
  let lastReport=executionStart, status='timeout', stopCode=0;
  function drain(){
    const n=w.esp32sim_out_take(emu);
    for(let i=0;i<n;i++){
      const kind=w.esp32sim_out_kind(emu,i),p=w.esp32sim_out_ptr(emu,i),len=w.esp32sim_out_len(emu,i);
      if(kind!==1){frames++;continue;}
      const msg=JSON.parse(dec.decode(mem().subarray(p,p+len)));
      if(msg.t==='serial' && msg.src==='usb'){serial+=msg.data;emit({type:'serial',data:msg.data,wallMs:performance.now()-executionStart});}
      else if(msg.t==='emu')emit({type:'emu',line:msg.msg});
    }
  }
  try {
    while(w.esp32sim_cycles(emu)<hz*480 && performance.now()-executionStart<1800000){
      stopCode=w.esp32sim_run(emu,2000000,Date.now());drain();
      if(stopCode){status='stopped';break;}
      if(/Guru Meditation|TG1WDT_SYS_RST|stack overflow|task_wdt/.test(serial)||logs.some(l=>/chip reset|panic/i.test(l))){status='firmware-failure';break;}
      if(/TINYDRAW_GATE1_AUTOMATED_DONE[^\r\n]*[\r\n]/.test(serial)){status='completed';break;}
      if(performance.now()-lastReport>2000){lastReport=performance.now();emit({type:'progress',guestSeconds:w.esp32sim_cycles(emu)/hz,wallSeconds:(lastReport-executionStart)/1000,frames,jit:{...blockJit.stats},tail:serial.slice(-400)});await new Promise(r=>setTimeout(r,0));}
    }
    const verdict=completedVerdict(serial, schema);
    const verdictValidation=validateVerdict(verdict, schema);
    const result={status,stopCode,verdict,verdictValidation,passed:status==='completed'&&verdictValidation.passed,guestSeconds:w.esp32sim_cycles(emu)/hz,wallSeconds:(performance.now()-executionStart)/1000,setupSeconds:(executionStart-started)/1000,instructions:w.esp32sim_insns(emu),frames,chainedBackedges:optionalCounter(w, 'esp32sim_chained_backedges', emu),jit:{...blockJit.stats,instructions:w.esp32sim_block_jit_insns(emu)},logs};
    w.esp32sim_profile_report?.(emu);emit({type:'result',...result});return result;
  }finally{w.esp32sim_delete(emu);}
}
