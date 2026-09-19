import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {resolve, join} from 'node:path';
import {createHash} from 'node:crypto';
import {createJitHost} from '../../../web/wasm/jit.mjs';

const [wasmPath, assetsPath, output, seconds = '8'] = process.argv.slice(2);
const out = resolve(output); mkdirSync(out, {recursive:true});
const assets = JSON.parse(readFileSync(assetsPath, 'utf8'));
const wanted = new Set(['40386c2c','403833b0','40384958','40386d37','40384b04','40057065','40383349']);
let w, compiled = 0;
const records = [], logs = [];
const enc = new TextEncoder(), dec = new TextDecoder();
const mem = () => new Uint8Array(w.memory.buffer);
const host = createJitHost(() => w);
const originalCompile = host.imports.host_jit_compile;
const imports = {...host.imports, host_profile_now: () => performance.now(),
  host_log: (p,n) => logs.push(dec.decode(mem().subarray(p,p+n))),
  host_jit_compile(p,n) {
    const bytes = Buffer.from(mem().subarray(p,p+n));
    const name = bytes.toString('latin1').match(/xtensa_([0-9a-f]{8})/)?.[1];
    const id = compiled++;
    if (wanted.has(name)) {
      const file = `${name}-${String(id).padStart(5,'0')}-${n}.wasm`;
      writeFileSync(join(out,file), bytes);
      records.push({id, pc:name, bytes:n, file, sha256:createHash('sha256').update(bytes).digest('hex')});
    }
    return originalCompile(p,n);
  }};
const wasm = readFileSync(wasmPath);
w = (await WebAssembly.instantiate(wasm, {env:imports})).instance.exports;
const withBytes = (bytes, fn) => { const p=w.esp32sim_alloc(bytes.length); mem().set(bytes,p); try { return fn(p,bytes.length); } finally { w.esp32sim_free(p,bytes.length); } };
const emu = withBytes(enc.encode('waveshare-amoled18-v2'), (p,n)=>w.esp32sim_new(p,n,16,8));
if (!emu) throw Error('create failed');
for (const [name,kind] of [['rom',0],['bootloader',1],['ptable',2],['app',3],['elf',4]]) {
  const rc=withBytes(readFileSync(assets[name]), (p,n)=>w.esp32sim_load(emu,kind,p,n));
  if (rc) throw Error(`load ${name}: ${rc}`);
}
if (w.esp32sim_boot(emu,0)) throw Error('boot failed');
w.esp32sim_set_jit(emu,1);
const hz=w.esp32sim_cpu_hz(emu), target=hz*Number(seconds);
while (w.esp32sim_cycles(emu)<target) {
  const rc=w.esp32sim_run(emu,2_000_000,Date.now());
  if (rc) throw Error(`run stopped: ${rc}`);
  w.esp32sim_out_take(emu);
}
const result={wasm:resolve(wasmPath), wasmSha256:createHash('sha256').update(wasm).digest('hex'), guestSeconds:w.esp32sim_cycles(emu)/hz, instructions:w.esp32sim_insns(emu), compiled, stats:host.stats, records, logs};
writeFileSync(join(out,'manifest.json'),JSON.stringify(result,null,2)+'\n');
w.esp32sim_delete(emu);
console.log(JSON.stringify({guestSeconds:result.guestSeconds, compiled, captured:records.length, pcs:[...new Set(records.map(r=>r.pc))]}));
