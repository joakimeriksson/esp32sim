// Generated-code counters for one artifact on the real workload, with no source change:
// bin/exact.mjs's run loop plus the jit host's own stats (web/wasm/jit.mjs tracks every
// host_jit_compile). usage: SECS=10 node host-jitstats.mjs <wasm> [manifest]
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
const BASE = process.env.TREE;
const fwDir = process.env.FW_DIR;
if (!BASE || !fwDir) throw new Error('Set TREE and FW_DIR to the source checkout and firmware directory');
const [wasmPath, name = 'pocket-tank'] = process.argv.slice(2);
const secs = Number(process.env.SECS || 10);
const { createJitHost } = await import(join(process.env.TREE || BASE, 'web/wasm/jit.mjs'));
const enc = new TextEncoder(), dec = new TextDecoder();
const m = JSON.parse(readFileSync(join(fwDir, `${name}.json`), 'utf8'));
const logs = []; let w;
const jit = createJitHost(() => w);
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), { env: { ...jit.imports, host_profile_now: () => performance.now(), host_log: (p, n) => logs.push(dec.decode(mem().subarray(p, p + n))) } });
w = instance.exports;
const mem = () => new Uint8Array(w.memory.buffer);
const withBytes = (b, f) => { const p = w.esp32sim_alloc(b.length); mem().set(b, p); try { return f(p, b.length); } finally { w.esp32sim_free(p, b.length); } };
const emu = withBytes(enc.encode(m.board), (p, n) => w.esp32sim_new(p, n, m.flash_mb | 0, m.psram_mb | 0));
const kinds = { rom: 0, bootloader: 1, ptable: 2, app: 3, flash: 5, script: 6, picture: 7 };
for (const [k, v] of Object.entries(m.files || {})) for (const rel of [].concat(v))
  if (withBytes(new Uint8Array(readFileSync(join(fwDir, rel))), (p, n) => w.esp32sim_load(emu, k === 'elf' ? 4 : kinds[k], p, n)) !== 0) throw new Error(`load ${rel}`);
for (const [off, rel] of Object.entries(m.flash_at || {})) withBytes(new Uint8Array(readFileSync(join(fwDir, rel))), (p, n) => w.esp32sim_load_at(emu, Number(off) >>> 0, p, n));
for (const s of m.stubs || []) { const [sym, val] = s.split('='); withBytes(enc.encode((m.symbols || {})[sym] || sym), (p, n) => w.esp32sim_stub(emu, p, n, Number(val ?? 0) >>> 0)); }
w.esp32sim_set_jit(emu, 1);
if (w.esp32sim_boot(emu, 0) !== 0) throw new Error('boot failed');
const hz = w.esp32sim_cpu_hz(emu); let text = ''; const fh = createHash('sha256'); let frames = 0;
const drain = () => { const n = w.esp32sim_out_take(emu); for (let i = 0; i < n; i++) { const kind = w.esp32sim_out_kind(emu, i), p = w.esp32sim_out_ptr(emu, i), len = w.esp32sim_out_len(emu, i); if (kind !== 1) { frames++; fh.update(mem().subarray(p, p + len)); continue; } const msg = JSON.parse(dec.decode(mem().subarray(p, p + len))); if (msg.t === 'serial') text += msg.data; } };
while (w.esp32sim_cycles(emu) < hz * secs) { if (w.esp32sim_run(emu, 2_000_000, 0) !== 0) throw new Error('run stopped'); drain(); }
drain();
console.log(JSON.stringify({ wasm: wasmPath, secs, insns: String(w.esp32sim_insns(emu)), frames, framesSha256: fh.digest('hex').slice(0, 8), console: createHash('sha256').update(text).digest('hex').slice(0, 8), ...jit.stats }));
