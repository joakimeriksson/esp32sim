import fs from 'node:fs';
import { pathToFileURL } from 'node:url';

// Check the linked artifact, including debug sections inherited from prebuilt std.
export function checkProductionSections(bytes) {
  new WebAssembly.Module(bytes); // Reject invalid/truncated modules before parsing sections.
  let pos = 8;
  const uleb = () => {
    let n = 0, shift = 0, b;
    do { b = bytes[pos++]; n += (b & 127) * 2 ** shift; shift += 7; } while (b & 128);
    return n;
  };
  const names = [];
  while (pos < bytes.length) {
    const id = bytes[pos++], length = uleb(), end = pos + length;
    if (id === 0) {
      const size = uleb();
      names.push(Buffer.from(bytes.subarray(pos, pos + size)).toString('utf8'));
    }
    pos = end;
  }
  const dwarf = names.filter(name => /^(?:\.debug_|\.zdebug_)/.test(name));
  if (dwarf.length) throw new Error(`Production WASM contains DWARF: ${dwarf.join(', ')}`);
  if (!names.includes('name')) throw new Error('Production WASM has no name section for profiles');
  return { dwarfAbsent: true, nameRetained: true, bytes: bytes.length };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (!process.argv[2]) throw new Error('Usage: node tools/check-wasm-sections.mjs <module.wasm>');
  console.log(JSON.stringify(checkProductionSections(fs.readFileSync(process.argv[2]))));
}
