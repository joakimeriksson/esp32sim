// host-s1b control: drop only the .debug_* custom sections from a built module.
// Every other section (types, functions, code, data, elements, name) is copied byte for byte,
// so the code section is identical to the input by construction.
// usage: node host-strip-dwarf.mjs in.wasm out.wasm
import fs from 'node:fs';
const [inPath, outPath] = process.argv.slice(2);
const b = fs.readFileSync(inPath);
let p = 8;
const u = () => { let v = 0, s = 0, c; do { c = b[p++]; v += (c & 127) * 2 ** s; s += 7; } while (c & 128); return v; };
const keep = [b.subarray(0, 8)];
const dropped = [];
while (p < b.length) {
  const secStart = p, id = b[p++], size = u(), body = p, end = body + size;
  let name = '';
  if (id === 0) { const q = p; const n = u(); name = b.toString('utf8', p, p + n); p = q; }
  if (id === 0 && name.startsWith('.debug_')) dropped.push({ name, size }); else keep.push(b.subarray(secStart, end));
  p = end;
}
fs.writeFileSync(outPath, Buffer.concat(keep));
console.log(JSON.stringify({ in: inPath, out: outPath, inBytes: b.length, outBytes: fs.statSync(outPath).size, dropped }));
