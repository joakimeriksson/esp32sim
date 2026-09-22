// exact.mjs runs base.wasm (to make the reference) and then the candidate, so a --cpu-prof
// profile holds two modules. Split at the first sample of the candidate module and rank self time.
import fs from 'node:fs';
const p = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const candUrlPart = process.argv[3]; // e.g. the candidate module's wasm:// url fragment
const nodes = new Map(p.nodes.map(n => [n.id, n.callFrame]));
const first = p.samples.findIndex(s => (nodes.get(s)?.url ?? '').includes(candUrlPart));
let t = 0; const self = new Map();
for (let i = first; i < p.samples.length; i++) {
  const f = nodes.get(p.samples[i]), d = p.timeDeltas[i];
  t += d;
  const url = f.url ?? '';
  const key = url.startsWith('wasm://wasm/esp32sim_wasm') ? f.functionName : (url.startsWith('wasm:') ? '<generated guest code>' : `<${f.functionName || 'js'}>`);
  self.set(key, (self.get(key) ?? 0) + d);
}
const demangle = n => n.replace(/Cs[0-9A-Za-z]+_/g, '').replace(/^_R[INMX]*[vt]?/, '').replace(/\d+/g, ' ').trim();
console.log(`candidate window: ${(t / 1e6).toFixed(3)} s, ${p.samples.length - first} samples`);
for (const [k, us] of [...self].sort((a, b) => b[1] - a[1]).slice(0, 34))
  console.log(`${(us / 1e6).toFixed(3).padStart(7)} s ${(100 * us / t).toFixed(2).padStart(6)}%  ${k.length > 110 ? demangle(k) : k}`);
