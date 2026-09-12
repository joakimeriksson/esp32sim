import assert from 'node:assert/strict';
import { createPacing } from '../web/wasm/pacing.mjs';

const p = createPacing();
assert.equal(p.turnMs(0), 25);
assert.equal(p.sliceCycles(3_000_000, 25, 0), 2_000_000, 'original slices before interaction');
p.input(100);
assert.equal(p.turnMs(100), 8);
assert.equal(p.turnMs(349), 8);
assert.equal(p.turnMs(350), 25);
p.input(300);
assert.equal(p.turnMs(549), 8);
assert.equal(p.turnMs(550), 25);
assert.equal(p.sliceCycles(3_000_000, 25, 550), 2_000_000, 'original slices after interaction');
assert.equal(p.sliceCycles(2_000_000, 25, 300), 64_000);
assert.equal(p.sliceCycles(100, 8, 300), 100, 'never extend the guest target');
p.observe(64_000, 16);
assert.equal(p.sliceCycles(2_000_000, 8, 300), 16_000, 'adapt immediately to expensive work');
assert.equal(p.sliceCycles(2_000_000, 1, 300), 4_000, 'respect remaining host turn');
p.observe(64_000, 1);
assert.equal(p.sliceCycles(2_000_000, 8, 300), 20_000, 'grow gradually');
p.observe(0, 1);
p.observe(1, 0);
assert.equal(p.sliceCycles(2_000_000, 8, 300), 20_000);
for (let i = 0; i < 100; i++) p.observe(2_000_000, 1);
assert.equal(p.sliceCycles(3_000_000, 25, 300), 2_000_000, 'retain maximum guest slice');
console.log('worker pacing tests passed');

// Exercise the actual worker loop with a deterministic WASM stand-in. Each run advances
// exactly its requested cycles and consumes host time; scheduling must not invent guest time.
const { readFile } = await import('node:fs/promises');
const { runInNewContext } = await import('node:vm');
let wall = 0, cycles = 0;
const pending = [], runs = [];
const wasm = {
  memory: new WebAssembly.Memory({ initial: 1 }),
  esp32sim_alloc: () => 0, esp32sim_free() {}, esp32sim_new: () => 1,
  esp32sim_set_jit() {}, esp32sim_boot: () => 0,
  esp32sim_cpu_hz: () => 240e6, esp32sim_cycles: () => cycles,
  esp32sim_insns: () => cycles, esp32sim_out_take: () => 0,
  esp32sim_in_text() {},
  esp32sim_run(_emu, amount) { runs.push(amount); cycles += amount; wall += amount / 16_000; return 0; },
};
const source = (await readFile(new URL('../web/wasm/worker.js', import.meta.url), 'utf8'))
  .replace(/^import .*;\n/gm, '');
const context = {
  createPacing, createJitHost: () => ({ imports: {} }), TextEncoder, TextDecoder,
  performance: { now: () => wall }, Date, postMessage() {},
  WebAssembly: { instantiate: async () => ({ instance: { exports: wasm } }) },
  setTimeout: (callback) => pending.push(callback),
};
runInNewContext(source, context);
await context.onmessage({ data: { op: 'init' } });
await context.onmessage({ data: { op: 'create', board: 'test' } });
await context.onmessage({ data: { op: 'start' } });
wall = 100;
await context.onmessage({ data: { op: 'text', data: '{"t":"touch"}' } });
const start = wall;
pending.shift()();
assert.equal(wall - start, 8, 'interaction turn yields after eight ms');
assert.equal(cycles, runs.reduce((a, b) => a + b, 0), 'only WASM advances guest cycles');
assert.ok(runs.every(n => n > 0 && n <= 2_000_000));
console.log('worker integration test passed');
wall = 400;
const idleStart = wall;
pending.shift()();
assert.equal(wall - idleStart, 125, 'idle turn allows the original two-million-cycle slice');
assert.equal(runs.at(-1), 2_000_000);
await context.onmessage({ data: { op: 'stop' } });
const stoppedCycles = cycles;
pending.shift()();
assert.equal(cycles, stoppedCycles, 'a pending callback cannot run a stopped emulator');

// The pace report: emulated seconds per wall second over each report window. The stand-in
// costs 1 ms per 16,000 cycles, about a fifteenth of real time, and the loop resynchronises
// while that far behind. A resync resets `behind` but must not make the speed look real-time.
{
  let wall2 = 0, cycles2 = 0;
  const paces = [], queue = [];
  const wasm2 = { ...wasm, esp32sim_cycles: () => cycles2, esp32sim_insns: () => cycles2,
    esp32sim_run(_emu, amount) { cycles2 += amount; wall2 += amount / 16_000; return 0; } };
  const context2 = {
    createPacing, createJitHost: () => ({ imports: {} }), TextEncoder, TextDecoder,
    performance: { now: () => wall2 }, Date, postMessage(message) { if (message.pace) paces.push(message.pace); },
    WebAssembly: { instantiate: async () => ({ instance: { exports: wasm2 } }) },
    setTimeout: (callback) => queue.push(callback),
  };
  runInNewContext(source, context2);
  await context2.onmessage({ data: { op: 'init' } });
  await context2.onmessage({ data: { op: 'create', board: 'test' } });
  await context2.onmessage({ data: { op: 'start' } });
  for (let turn = 0; turn < 400 && paces.length < 3; turn++) { wall2 += 10; queue.shift()(); }
  assert.ok(paces.length >= 3, 'pace reports arrive');
  for (const pace of paces) assert.ok(pace.speed > 0.05 && pace.speed < 0.07, `speed ${pace.speed} reports the slow run`);
  assert.ok(paces.some(pace => pace.resyncs > 0), 'the run resynchronised while behind');
  await context2.onmessage({ data: { op: 'stop' } });
  console.log('worker pace report test passed');
}
