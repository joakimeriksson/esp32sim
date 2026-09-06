// Offline harness control-flow regression: mocked WASM exports, no emulator execution.
// Run: node --test tools/browser-benchmark/battery-failure.test.mjs
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
const root = path.resolve(import.meta.dirname, '../..');
const harness = path.join(root, 'tools/browser-benchmark');
const temp = await fs.mkdtemp(path.join(os.tmpdir(), 'esp32-battery-audit-'));
const realFetch = globalThis.fetch;
const realInstantiate = WebAssembly.instantiate;
try {
  const batteryPath = path.join(harness, 'battery.mjs');
  let source = await fs.readFile(batteryPath, 'utf8');
  source = source.replace("'./verdict.mjs'", JSON.stringify(pathToFileURL(path.join(harness, 'verdict.mjs')).href));
  source = source.replace("'/web/wasm/jit.mjs'", JSON.stringify(pathToFileURL(path.join(root, 'web/wasm/jit.mjs')).href));
  await fs.writeFile(path.join(temp, 'battery.mjs'), source);
  const schema = JSON.parse(await fs.readFile(path.join(harness, 'verdict-schema.json'), 'utf8'));
  const verdict = [schema.marker, ...schema.gates.map(key => `${key}=1`), 'ssaa_receipt=yellow'].join(' ');
  globalThis.fetch = async () => ({json: async () => schema});
  const {runBattery} = await import(pathToFileURL(path.join(temp, 'battery.mjs')).href);
  for (const channel of ['serial', 'host_log']) {
    const memory = {buffer: new ArrayBuffer(131072)};
    const bytes = new Uint8Array(memory.buffer), enc = new TextEncoder();
    const serial = `${verdict}\n${channel === 'serial' ? 'Guru Meditation Error: simulated audit failure\n' : ''}`;
    const msg = enc.encode(JSON.stringify({t:'serial', src:'usb', data:serial}));
    bytes.set(msg, 32768);
    const log = enc.encode('panic: simulated audit failure'); bytes.set(log, 65536);
    let cycles = 0;
    WebAssembly.instantiate = async (_wasm, imports) => ({instance:{exports:{
      memory,
      esp32sim_alloc: () => 128, esp32sim_free: () => {},
      esp32sim_new: () => 1, esp32sim_load: () => 0, esp32sim_boot: () => 0,
      esp32sim_set_jit: () => {}, esp32sim_cpu_hz: () => 240000000,
      esp32sim_cycles: () => cycles,
      esp32sim_run: () => {cycles += 100; if(channel === 'host_log') imports.env.host_log(65536, log.length); return 0;},
      esp32sim_out_take: () => 1, esp32sim_out_kind: () => 1,
      esp32sim_out_ptr: () => 32768, esp32sim_out_len: () => msg.length,
      esp32sim_insns: () => 100, esp32sim_block_jit_insns: () => 0,
      esp32sim_delete: () => {},
    }}});
    const result = await runBattery(async () => new Uint8Array(), () => {});
    assert.equal(result.status, 'firmware-failure');
    assert.equal(result.passed, false);
    console.log(`Rejected complete DONE plus ${channel} failure`);
  }
} finally {
  globalThis.fetch = realFetch; WebAssembly.instantiate = realInstantiate;
  await fs.rm(temp, {recursive:true, force:true});
}
