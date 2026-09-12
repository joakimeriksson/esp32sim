// Offline control-flow test for fixed-duration workloads: mocked WASM exports, no emulator.
// Run: node --test tools/browser-benchmark/battery-workload.test.mjs
import test from 'node:test';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';

const root = path.resolve(import.meta.dirname, '../..');
const harness = path.join(root, 'tools/browser-benchmark');
const workloads = JSON.parse(await fs.readFile(path.join(harness, 'workloads.json'), 'utf8'));
const workload = {name: 'pocket-tank', ...workloads['pocket-tank']};

async function importBattery() {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), 'esp32-battery-workload-'));
  let source = await fs.readFile(path.join(harness, 'battery.mjs'), 'utf8');
  source = source.replace("'./verdict.mjs'", JSON.stringify(pathToFileURL(path.join(harness, 'verdict.mjs')).href));
  source = source.replace("'/web/wasm/jit.mjs'", JSON.stringify(pathToFileURL(path.join(root, 'web/wasm/jit.mjs')).href));
  await fs.writeFile(path.join(temp, 'battery.mjs'), source);
  const module = await import(pathToFileURL(path.join(temp, 'battery.mjs')).href);
  return {module, temp};
}

// A stand-in that prints `lines` on UART0 once per run slice and advances a guest second per slice.
function mockExports(lines, calls) {
  const memory = {buffer: new ArrayBuffer(131072)};
  const msg = new TextEncoder().encode(JSON.stringify({t: 'serial', src: 'uart0', data: lines}));
  new Uint8Array(memory.buffer).set(msg, 32768);
  let cycles = 0;
  return {
    memory,
    esp32sim_alloc: () => 128, esp32sim_free: () => {},
    esp32sim_new: () => 1, esp32sim_boot: () => 0, esp32sim_set_jit: () => {},
    esp32sim_load: (_emu, kind) => { calls.push(['load', kind]); return 0; },
    esp32sim_load_at: (_emu, offset) => { calls.push(['load_at', offset]); return 0; },
    esp32sim_cpu_hz: () => 240_000_000, esp32sim_cycles: () => cycles,
    esp32sim_run: () => { cycles += 240_000_000; return 0; },
    esp32sim_out_take: () => 1, esp32sim_out_kind: () => 1,
    esp32sim_out_ptr: () => 32768, esp32sim_out_len: () => msg.length,
    esp32sim_insns: () => cycles, esp32sim_block_jit_insns: () => 0,
    esp32sim_delete: () => {},
  };
}

async function run(lines) {
  const {module, temp} = await importBattery();
  const realInstantiate = WebAssembly.instantiate;
  const calls = [];
  try {
    WebAssembly.instantiate = async () => ({instance: {exports: mockExports(lines, calls)}});
    const result = await module.runBattery(async () => new Uint8Array(), () => {}, true, false, workload);
    return {result, calls};
  } finally {
    WebAssembly.instantiate = realInstantiate;
    await fs.rm(temp, {recursive: true, force: true});
  }
}

test('pocket-tank loads its model at the flash offset, runs its guest duration and passes its checks', async () => {
  const {result, calls} = await run('I (1) advisor: zone 2 -> explore [1800 ms, 24.3 tok/s]\nI (2) display: 62.5 fps | render 3.2 ms\n');
  assert.deepEqual(calls.filter(([kind]) => kind === 'load').map(([, k]) => k), [0, 1, 2, 3], 'rom, bootloader, ptable and app, no ELF');
  assert.deepEqual(calls.filter(([kind]) => kind === 'load_at'), [['load_at', 0x290000]]);
  assert.equal(result.workload, 'pocket-tank');
  assert.equal(result.status, 'completed');
  assert.equal(result.guestSeconds, 30);
  assert.equal(result.verdict, null);
  assert.equal(result.verdictValidation.schema, 'pocket-tank-v1');
  assert.ok(result.checks.every(check => check.count >= check.min));
  assert.equal(result.passed, true);
});

test('pocket-tank fails when the model never decides', async () => {
  const {result} = await run('I (2) display: 62.5 fps | render 3.2 ms\n');
  assert.equal(result.status, 'completed');
  assert.equal(result.checks.find(check => check.name === 'model_decisions').count, 0);
  assert.equal(result.passed, false);
});

test('pocket-tank rejects a firmware panic even when the checks pass', async () => {
  const {result} = await run('I (1) advisor: x [1 ms, 24.3 tok/s]\nI (2) display: 62.5 fps\nGuru Meditation Error: simulated\n');
  assert.equal(result.status, 'firmware-failure');
  assert.equal(result.passed, false);
});
