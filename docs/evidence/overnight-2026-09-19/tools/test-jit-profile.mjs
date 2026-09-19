import {readFileSync} from 'node:fs';
import {createJitHost} from '../../../web/wasm/jit.mjs';
let w;
const host=createJitHost(()=>w);
w=(await WebAssembly.instantiate(readFileSync(process.argv[2]), {env:{...host.imports,
  host_profile_now:()=>performance.now(),
  host_log:(p,n)=>console.error(new TextDecoder().decode(new Uint8Array(w.memory.buffer,p,n))),
}})).instance.exports;
const count=w.esp32sim_test_block_jit();
if (!count || host.stats.failed || !host.stats.compiled || host.stats.compiled!==host.stats.released) throw Error(JSON.stringify(host.stats));
console.log(`PASS: ${count} WASM differential cases with jit-profile enabled; ${host.stats.compiled} modules released`);
