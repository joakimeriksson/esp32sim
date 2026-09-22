import fs from 'node:fs';
import crypto from 'node:crypto';
const inputs = process.argv.slice(2);
if (inputs.length !== 2) throw new Error('Pass before.wasm and after.wasm');
const hash = b => crypto.createHash('sha256').update(b).digest('hex');
function inspect(path) {
  const b = fs.readFileSync(path); let p = 8;
  const u = () => { let v = 0, s = 0, c; do { c = b[p++]; v += (c & 127) * 2 ** s; s += 7; } while (c & 128); return v; };
  const sections = [], bodies = [];
  while (p < b.length) {
    const id = b[p++], size = u(), start = p, end = start + size;
    sections.push({id, size, sha256: hash(b.subarray(start, end))});
    if (id === 10) { const n = u(); for (let i = 0; i < n; i++) { const size = u(); bodies.push({size, sha256: hash(b.subarray(p, p + size))}); p += size; } }
    p = end;
  }
  return {sha256: hash(b), sections, bodies};
}
const [a,b] = inputs.map(inspect);
const changes = a.bodies.flatMap((v,i) => v.sha256 === b.bodies[i]?.sha256 ? [] : [{definedFunctionIndex:i, before:v, after:b.bodies[i]}]);
console.log(JSON.stringify({beforeSha256:a.sha256, afterSha256:b.sha256, beforeFunctions:a.bodies.length, afterFunctions:b.bodies.length, beforeSections:a.sections.length, afterSections:b.sections.length, changedBodies:changes, sections:a.sections.map((v,i)=>({id:v.id,afterId:b.sections[i]?.id,beforeBytes:v.size,afterBytes:b.sections[i]?.size,identical:v.sha256===b.sections[i]?.sha256}))},null,2));
