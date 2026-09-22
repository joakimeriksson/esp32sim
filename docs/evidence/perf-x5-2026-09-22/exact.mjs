#!/usr/bin/env node
// Non-timing exactness check under Node: runs pocket-tank (default) on a wasm build and prints the exact
// instruction total + console SHA-256, and compares them with a freshly run supplied base build.
//   TREE=<source-tree> FW_DIR=<firmware-dir> BASE_WASM=<baseline.wasm> node exact.mjs <candidate.wasm> [manifest=pocket-tank]
//   SECS=30 (guest seconds; 30 = the pinned benchmark run); TREE provides web/wasm/jit.mjs
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { matchesReference } from './exact-contract.mjs';
const { TREE, FW_DIR, BASE_WASM } = process.env;
const fwDir = FW_DIR;
const [wasmPath, name = 'pocket-tank'] = process.argv.slice(2);
if (!TREE || !FW_DIR || !BASE_WASM || !wasmPath) {
  throw new Error('Required: TREE=<source-tree> FW_DIR=<firmware-dir> BASE_WASM=<baseline.wasm> node exact.mjs <candidate.wasm> [manifest=pocket-tank]');
}
const secs = Number(process.env.SECS || 30);
const { createJitHost } = await import(pathToFileURL(resolve(TREE, 'web/wasm/jit.mjs')).href);
const enc = new TextEncoder(), dec = new TextDecoder();
async function run(path) {
  const m = JSON.parse(readFileSync(join(fwDir, `${name}.json`), 'utf8'));
  const logs = []; let w;
  const jit = createJitHost(() => w);
  const { instance } = await WebAssembly.instantiate(readFileSync(path), { env: { ...jit.imports, host_profile_now: () => performance.now(), host_log: (p, n) => logs.push(dec.decode(mem().subarray(p, p + n))) } });
  w = instance.exports;
  const mem = () => new Uint8Array(w.memory.buffer);
  const withBytes = (b, f) => { const p = w.esp32sim_alloc(b.length); mem().set(b, p); try { return f(p, b.length); } finally { w.esp32sim_free(p, b.length); } };
  const emu = withBytes(enc.encode(m.board), (p, n) => w.esp32sim_new(p, n, m.flash_mb | 0, m.psram_mb | 0));
  const kinds = { rom: 0, bootloader: 1, ptable: 2, app: 3, flash: 5, script: 6, picture: 7 };
  for (const [k, v] of Object.entries(m.files || {})) for (const rel of [].concat(v))
    if (withBytes(new Uint8Array(readFileSync(join(fwDir, rel))), (p, n) => w.esp32sim_load(emu, k === 'elf' ? 4 : kinds[k], p, n)) !== 0) throw new Error(`load ${rel}: ${logs.join('|')}`);
  for (const [off, rel] of Object.entries(m.flash_at || {})) withBytes(new Uint8Array(readFileSync(join(fwDir, rel))), (p, n) => w.esp32sim_load_at(emu, Number(off) >>> 0, p, n));
  for (const s of m.stubs || []) { const [sym, val] = s.split('='); withBytes(enc.encode((m.symbols || {})[sym] || sym), (p, n) => w.esp32sim_stub(emu, p, n, Number(val ?? 0) >>> 0)); }
  w.esp32sim_set_jit(emu, 1);
  if (w.esp32sim_boot(emu, 0) !== 0) throw new Error(`boot failed: ${logs.join('|')}`);
  const hz = w.esp32sim_cpu_hz(emu); let text = '', frames = 0; const fh = createHash('sha256');
  const drain = () => { const n = w.esp32sim_out_take(emu); for (let i = 0; i < n; i++) { const kind = w.esp32sim_out_kind(emu, i), p = w.esp32sim_out_ptr(emu, i), len = w.esp32sim_out_len(emu, i); if (kind !== 1) { frames++; fh.update(mem().subarray(p, p + len)); continue; } const msg = JSON.parse(dec.decode(mem().subarray(p, p + len))); if (msg.t === 'serial') text += msg.data; } };
  const t0 = Date.now();
  while (w.esp32sim_cycles(emu) < hz * secs) { const rc = w.esp32sim_run(emu, 2_000_000, 0); if (rc !== 0) throw new Error(`run stopped rc=${rc}: ${logs.slice(-3).join('|')}`); drain(); }
  drain();
  const out = { framesSha256: fh.digest('hex'), insns: String(w.esp32sim_insns(emu)), consoleSha256: createHash('sha256').update(text).digest('hex'), frames, panics: logs.filter(l => l.includes('panic')).length, jitFailures: jit.stats?.failed ?? null, wallSeconds: (Date.now() - t0) / 1000 };
  return out;
}
const ref = await run(BASE_WASM);
const got = await run(wasmPath);
const same = matchesReference(got, ref);
console.log(JSON.stringify({ wasm: wasmPath, secs, ...got, refInsns: ref.insns, refSha: ref.consoleSha256.slice(0, 8), refFrames: ref.frames, refFramesSha: ref.framesSha256.slice(0, 8), EXACT: same }, null, 1));
console.log(same ? `EXACT ok (${name}, ${secs} guest s; Node wall time is NOT a benchmark)` : 'EXACT FAIL: instruction total, console hash, frame count or frame-content hash differs from base (or a panic / JIT failure)');
process.exit(same ? 0 : 1);
