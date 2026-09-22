// Identify changed u32 line fields in Rust source-location records without assuming a fixed delta.
// Usage: node data-locations.mjs before.wasm after.wasm
import fs from 'node:fs';
import crypto from 'node:crypto';
const inputs = process.argv.slice(2);
if (inputs.length !== 2) throw new Error('Pass before.wasm and after.wasm');
function inspect(path) {
  const bytes = fs.readFileSync(path);
  if (!WebAssembly.validate(bytes)) throw new Error('Invalid WASM');
  let p = 8;
  const integer = signed => {
    let value = 0, shift = 0, byte;
    do {
      if (p >= bytes.length || shift > 35) throw new Error('Invalid LEB');
      byte = bytes[p++]; value += (byte & 127) * 2 ** shift; shift += 7;
    } while (byte & 128);
    return signed && (byte & 64) ? value - 2 ** shift : value;
  };
  const segments = [];
  while (p < bytes.length) {
    const id = bytes[p++], size = integer(false), end = p + size;
    if (id === 11) {
      const count = integer(false);
      for (let i = 0; i < count; i++) {
        const mode = integer(false);
        if (mode === 2) { if (integer(false) !== 0) throw new Error('Nondefault memory'); }
        else if (mode !== 0) throw new Error('Expected active data segment');
        if (bytes[p++] !== 65) throw new Error('Expected i32.const');
        const address = integer(true);
        if (address < 0 || bytes[p++] !== 11) throw new Error('Unsupported offset');
        const length = integer(false);
        segments.push({address, bytes: bytes.subarray(p, p + length)}); p += length;
      }
      if (p !== end) throw new Error('Invalid data-section boundary');
    }
    p = end;
  }
  const memory = Buffer.alloc(Math.max(0, ...segments.map(s => s.address + s.bytes.length)));
  for (const segment of segments) segment.bytes.copy(memory, segment.address);
  return {sha256: crypto.createHash('sha256').update(bytes).digest('hex'), memory,
    layout: segments.map(s => ({address: s.address, bytes: s.bytes.length}))};
}
const [before, after] = inputs.map(inspect);
if (JSON.stringify(before.layout) !== JSON.stringify(after.layout)) throw new Error('Data layout changed');
const a = before.memory, b = after.memory, words = new Set();
let changedBytes = 0;
for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) { changedBytes++; words.add(i - i % 4); }
const changes = [...words].map(address => {
  if (address < 8 || address + 8 > a.length) throw new Error('Truncated location record');
  const pointer = a.readUInt32LE(address - 8), length = a.readUInt32LE(address - 4);
  const beforeLine = a.readUInt32LE(address), afterLine = b.readUInt32LE(address);
  const column = a.readUInt32LE(address + 4);
  const filename = a.subarray(pointer, pointer + length).toString('utf8');
  if (pointer + length > a.length || !/^[A-Za-z0-9_./-]+\.rs$/.test(filename) ||
      !a.subarray(address - 8, address).equals(b.subarray(address - 8, address)) ||
      !a.subarray(pointer, pointer + length).equals(b.subarray(pointer, pointer + length)) ||
      column !== b.readUInt32LE(address + 4) || beforeLine === 0 || afterLine === 0)
    throw new Error(`Not an isolated source-line field at ${address}`);
  return {address, filename, beforeLine, afterLine, column};
});
console.log(JSON.stringify({beforeSha256: before.sha256, afterSha256: after.sha256, changedBytes,
  changedWords: changes.length, allDifferencesAreSourceLineRecords: true,
  dataSegmentLayoutUnchanged: true, filenamesPointersLengthsAndColumnsUnchanged: true, changes}, null, 2));
