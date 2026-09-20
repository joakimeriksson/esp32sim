#!/usr/bin/env node
// EX170: drive esp32sim_test_fma_sweep (jit-tests build) over many seeds. usage: fma-sweep.mjs [millions=300]
import { readFileSync } from 'node:fs';
import { createJitHost } from '../web/wasm/jit.mjs';
let w; const host = createJitHost(() => w);
const bytes = readFileSync(process.env.ESP32SIM_WASM || new URL('../target/wasm32-unknown-unknown/release/esp32sim_wasm.wasm', import.meta.url));
let halfway = 0;
w = (await WebAssembly.instantiate(bytes, { env: { ...host.imports, host_profile_now: () => performance.now(), host_log(p, n) {
  const line = new TextDecoder().decode(new Uint8Array(w.memory.buffer, p, n)); const m = / halfway=(\d+)/.exec(line); if (m) halfway += Number(m[1]); else console.error(line); } } })).instance.exports;
const millions = Number(process.argv[2] || 300); let bad = 0; const t0 = Date.now();
for (let i = 0; i < millions; i++) { bad += w.esp32sim_test_fma_sweep(0x9e3779b97f4a7 + i * 0x1000193, 1_000_000); if (i % 25 === 24) console.log(`${i + 1}M triples x2 ops, mismatches=${bad}, halfway=${halfway}, ${((Date.now() - t0) / 1000).toFixed(0)}s`); }
console.log(`${bad === 0 ? 'PASS' : 'FAIL'}: ${millions}M triples (MADD.S and MSUB.S each), mismatches=${bad}, halfway-class triples=${halfway}, jit=${JSON.stringify(host.stats)}`);
process.exit(bad === 0 ? 0 : 1);
