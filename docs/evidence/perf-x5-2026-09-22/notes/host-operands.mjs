// scratch: attribute every differing byte between two same-size function bodies to the opcode
// whose LEB128 immediate it sits in (walk back over continuation bytes to the opcode byte).
import fs from 'node:fs';
function parse(path) {
  const b = fs.readFileSync(path); let p = 8; const bodies = []; const names = new Map(); let imports = 0;
  const u = () => { let v = 0, s = 0, c; do { c = b[p++]; v += (c & 127) * 2 ** s; s += 7; } while (c & 128); return v; };
  while (p < b.length) {
    const id = b[p++], size = u(), end = p + size;
    if (id === 0) { const n = u(); const label = b.toString('utf8', p, p + n); p += n;
      if (label === 'name') { while (p < end) { const sub = b[p++], slen = u(), sEnd = p + slen;
        if (sub === 1) { const n2 = u(); for (let i = 0; i < n2; i++) { const idx = u(), l = u(); names.set(idx, b.toString('utf8', p, p + l)); p += l; } } p = sEnd; } } }
    if (id === 2) { const n = u(); for (let i = 0; i < n; i++) { const ml = u(); p += ml; const fl = u(); p += fl; const k = b[p++]; if (k === 0) { imports++; u(); } else { u(); if (k === 1 || k === 2) u(); } } }
    if (id === 10) { const n = u(); for (let i = 0; i < n; i++) { const s2 = u(); bodies.push(b.subarray(p, p + s2)); p += s2; } }
    p = end;
  }
  return { bodies, names, imports };
}
const [A, B] = process.argv.slice(2).map(parse);
const norm = n => n.replace(/Cs[0-9A-Za-z]+_/g, 'Cs_');
const map = m => { const r = new Map(); m.bodies.forEach((body, i) => r.set(norm(m.names.get(i + m.imports) ?? `#${i}`), body)); return r; };
const [na, nb] = [map(A), map(B)];
const hist = new Map(); let bytes = 0, bodiesDiff = 0, unexplained = 0;
for (const [name, a] of na) {
  const b = nb.get(name); if (!b || a.length !== b.length) continue;
  let d = false;
  for (let k = 0; k < a.length; k++) {
    if (a[k] === b[k]) continue;
    d = true; bytes++;
    let j = k; while (j > 0 && (a[j - 1] & 0x80)) j--;            // start of this LEB run
    const op = j > 0 ? a[j - 1] : -1;                              // byte before it
    // a memarg offset is preceded by the alignment byte, which is preceded by the load/store opcode
    let m = j - 1; while (m > 0 && (a[m - 1] & 0x80)) m--;
    const memop = op <= 0x0f && m > 0 && a[m - 1] >= 0x28 && a[m - 1] <= 0x3e;
    const key = memop ? 'load/store offset' : ({ 0x41: 'i32.const', 0x10: 'call', 0x11: 'call_indirect', 0x42: 'i64.const', 0x23: 'global.get', 0x24: 'global.set' }[op] ?? `0x${op.toString(16)}`);
    hist.set(key, (hist.get(key) ?? 0) + 1);
    if (!(op === 0x41 || op === 0x10 || op === 0x11 || memop)) unexplained++;
  }
  if (d) bodiesDiff++;
}
console.log(JSON.stringify({ bodiesCompared: na.size, bodiesDiffering: bodiesDiff, differingBytes: bytes, notInACallOrConstImmediate: unexplained, byPrecedingOpcode: Object.fromEntries([...hist].sort((x, y) => y[1] - x[1])) }, null, 1));
