// scratch: match wasm function bodies by name-section name (not index) between two modules
import fs from 'node:fs';
import crypto from 'node:crypto';
const sha = b => crypto.createHash('sha256').update(b).digest('hex').slice(0, 12);
function parse(path) {
  const b = fs.readFileSync(path); let p = 8;
  const u = () => { let v = 0, s = 0, c; do { c = b[p++]; v += (c & 127) * 2 ** s; s += 7; } while (c & 128); return v; };
  const out = { sections: [], bodies: [], names: new Map(), imports: 0 };
  while (p < b.length) {
    const id = b[p++], size = u(), start = p, end = start + size;
    let label = '';
    if (id === 0) { const n = u(); label = b.toString('utf8', p, p + n); p += n;
      if (label === 'name') { // subsections: 1 = function names
        while (p < end) { const sub = b[p++], slen = u(), sEnd = p + slen;
          if (sub === 1) { const n2 = u(); for (let i = 0; i < n2; i++) { const idx = u(), l = u(); out.names.set(idx, b.toString('utf8', p, p + l)); p += l; } }
          p = sEnd; } } }
    if (id === 2) { const n = u(); for (let i = 0; i < n; i++) { const ml = u(); p += ml; const fl = u(); p += fl; const kind = b[p++]; if (kind === 0) { out.imports++; u(); } else { u(); if (kind === 1 || kind === 2) u(); } } }
    if (id === 10) { const n = u(); for (let i = 0; i < n; i++) { const s2 = u(); out.bodies.push(b.subarray(p, p + s2)); p += s2; } }
    out.sections.push({ id, label, size });
    p = end;
  }
  return out;
}
const [A, B] = process.argv.slice(2).map(parse);
// v0 mangling embeds a per-crate disambiguator (Cs<hash>_) that moves when any -C flag moves
const norm = n => n.replace(/Cs[0-9A-Za-z]+_/g, 'Cs_');
const byName = m => { const r = new Map(); m.bodies.forEach((body, i) => r.set(norm(m.names.get(i + m.imports) ?? `#${i}`), body)); return r; };
const [na, nb] = [byName(A), byName(B)];
let same = 0, diff = 0, sizeDiff = 0, onlyA = 0, onlyB = 0, diffBytes = 0, permuted = 0;
const examples = [];
for (const [name, a] of na) {
  const b = nb.get(name);
  if (!b) { onlyA++; continue; }
  if (a.length !== b.length) { sizeDiff++; diff++; if (examples.length < 6) examples.push({ name, sizes: [a.length, b.length] }); continue; }
  let d = 0; for (let k = 0; k < a.length; k++) if (a[k] !== b[k]) d++;
  if (d === 0) same++; else { diff++; diffBytes += d; }
}
for (const name of nb.keys()) if (!na.has(name)) onlyB++;
const idxA = [...na.keys()], idxB = [...nb.keys()];
for (let i = 0; i < Math.min(idxA.length, idxB.length); i++) if (idxA[i] !== idxB[i]) permuted++;
console.log(JSON.stringify({ bodies: [A.bodies.length, B.bodies.length], imports: [A.imports, B.imports], matchedIdenticalByName: same, differing: diff, ofWhichSizeDiffer: sizeDiff, differingBytesInSameSizeBodies: diffBytes, onlyInA: onlyA, onlyInB: onlyB, positionsWhereNameOrderDiffers: permuted, examples }, null, 1));
