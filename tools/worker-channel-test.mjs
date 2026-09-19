import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { MessageChannel } from 'node:worker_threads';
import { runInNewContext } from 'node:vm';
import { applyExperiments, HW } from '../web/wasm/experiments.mjs';
import { createPacing } from '../web/wasm/pacing.mjs';

const source = (await readFile(new URL('../web/wasm/worker.js', import.meta.url), 'utf8')).replace(/^import .*;\n/gm, '');
async function harness(cost = 0, additions = [], experiments = [], overrides = {}) {
  let wall = 0, cycles = 0, input = 0, frame = null, immediate = 0;
  const timers = [], messages = [], channels = [], runs = [], deletedNetworks = [];
  let delivered;
  // Native ports dispatch the actual worker continuation asynchronously. Only the clock
  // and timers are controlled, so assertions do not depend on host machine speed.
  class Channel extends MessageChannel {
    constructor() {
      super(); channels.push(this);
      const post = this.port2.postMessage.bind(this.port2);
      this.port2.postMessage = value => { immediate++; post(value); };
      this.port1.addEventListener('message', () => { delivered?.(); delivered = null; });
    }
  }
  const wasm = {
    memory: new WebAssembly.Memory({ initial: 1 }),
    esp32sim_alloc: () => 128, esp32sim_free() {}, esp32sim_new: () => 1,
    esp32sim_delete() {}, esp32sim_set_jit() {}, esp32sim_boot: () => 0,
    esp32sim_net_new: () => 2, esp32sim_net_delete(net) { deletedNetworks.push(net); },
    esp32sim_net_add: () => additions.shift() ?? 0,
    esp32sim_cpu_hz: () => 240e6, esp32sim_cycles: () => cycles,
    esp32sim_insns: () => cycles, esp32sim_in_text() { input++; },
    esp32sim_run(_emu, amount) { runs.push({ amount, input }); cycles += amount; wall += amount * cost; return 0; },
    esp32sim_out_take() {
      if (frame === null) return 0;
      new Uint8Array(this.memory.buffer).set([1, frame]); frame = null; return 1;
    },
    esp32sim_out_kind: () => 2, esp32sim_out_ptr: () => 0, esp32sim_out_len: () => 2,
  };
  const context = {
    applyExperiments, createPacing, createJitHost: () => ({ imports: {} }), TextEncoder, TextDecoder, MessageChannel: Channel,
    performance: { now: () => wall }, Date, postMessage: m => messages.push(m),
    WebAssembly: { instantiate: async () => ({ instance: { exports: wasm } }) },
    setTimeout: (callback, delay) => timers.push({ callback, delay }),
  };
  Object.assign(wasm, overrides);
  runInNewContext(source, context);
  const send = data => context.onmessage({ data });
  await send({ op: 'init' }); await send({ op: 'create', board: 'test', experiments });
  return {
    send, timers, messages, runs, deletedNetworks, get immediate() { return immediate; },
    setWall(value) { wall = value; }, get wall() { return wall; },
    nextMessage() { return new Promise(resolve => { delivered = resolve; }); },
    frame(id) { frame = id; runInNewContext('drain()', context); },
    close() { channels.forEach(c => { c.port1.close(); c.port2.close(); }); },
  };
}

for (const cost of [0, 1 / 24_000_000]) {
  const h = await harness(cost);
  try {
    await h.send({ op: 'start' });
    for (let turn = 0; turn < 20; turn++) {
      assert.equal(h.timers.length, 1, 'caught-up guest schedules one sleep');
      const timer = h.timers.shift();
      assert.ok(timer.delay >= 1 && timer.delay <= 20);
      h.setWall(h.wall + timer.delay); timer.callback();
      assert.equal(h.immediate, 0, 'cheap guest never spins on the channel');
    }
    assert.ok(h.runs.length > 0, 'sleep still advances the guest');
  } finally { h.close(); }
}
{
  const h = await harness(1 / 16_000);
  try {
    await h.send({ op: 'start' });
    h.setWall(100);
    await h.send({ op: 'text', data: 'touch down' });
    const next = h.nextMessage();
    h.timers.shift().callback();
    assert.equal(h.wall, 108, 'active drawing yields after eight ms');
    assert.equal(h.immediate, 1, 'unfinished turn uses the native channel');
    assert.equal(h.timers.length, 0, 'no clamped timer while behind');
    await h.send({ op: 'text', data: 'touch move' });
    await next;
    assert.equal(h.wall, 116, 'native channel executes another interactive turn');
    assert.equal(h.runs.at(-1).input, 2, 'input queued between turns reaches the next run');
    assert.ok(h.runs.every(r => r.input && r.amount <= 64_000), 'input reaches bounded guest slices');
    await h.send({ op: 'stop' });
    const stopped = h.runs.length;
    await h.nextMessage();
    assert.equal(h.runs.length, stopped, 'queued immediate yield respects stop');
  } finally { h.close(); }
}

// Execute each supported consumer's real handler against the real worker drain/ACK
// protocol. Rendering is replaced by a recorder; messages retain FIFO delivery.
for (const [path, end] of [['../web/emu.js', '  const queue ='], ['../tools/browser-benchmark/response.mjs', 'worker.onerror']]) {
  const text = await readFile(new URL(path, import.meta.url), 'utf8');
  const handler = text.slice(text.indexOf('worker.onmessage ='), text.indexOf(end));
  const h = await harness(), rendered = [], acks = [];
  const record = buf => rendered.push(new Uint8Array(buf)[1]);
  const consumer = { worker: { postMessage(m) { acks.push(m); } }, onmessage: record, frame: record, window: {}, waiters: [] };
  runInNewContext(handler, consumer);
  try {
    for (let id = 1; id <= 6; id++) {
      h.frame(id);
      const message = h.messages.findLast(m => m.bin);
      assert.equal(new Uint8Array(message.bin)[1], id, `${path} receives every sequential frame`);
      consumer.worker.onmessage({ data: message });
      assert.equal(acks.length, 1, `${path} ACKs after rendering`);
      assert.equal(rendered.at(-1), id);
      await h.send(acks.shift());
    }
    assert.equal(rendered.length, 6);
  } finally { h.close(); }
}

for (const op of ['create', 'net-create']) {
  const h = await harness();
  try {
    h.frame(11); h.frame(22); h.frame(33);
    assert.equal(h.messages.filter(m => m.bin).length, 2);
    await h.send({ op, board: 'replacement', nodes: [] });
    const boundary = h.messages.length;
    await h.send({ op: 'frame-ack' });
    assert.equal(h.messages.length, boundary, `${op} discards the old pending frame`);
    if (op === 'net-create') await h.send({ op: 'create', board: 'replacement' });
    h.frame(44); h.frame(55);
    assert.equal(new Uint8Array(h.messages.findLast(m => m.bin).bin)[1], 44, 'late ACK does not reset all outstanding credits');
    await h.send({ op: 'frame-ack' });
    assert.equal(new Uint8Array(h.messages.findLast(m => m.bin).bin)[1], 55, 'remaining old ACK releases exactly one slot');
  } finally { h.close(); }
}
for (const failure of [-1, 0xffffffff]) {
  const h = await harness(0, [0, failure]);
  try {
    await h.send({ op: 'net-create', nodes: [{}, { flash_mb: 4096 }] });
    assert.equal(h.messages.at(-1).created, false, 'partial network creation reports failure');
    assert.equal(h.messages.at(-1).nodes, 0, 'no missing node is reported as created');
    assert.deepEqual(h.deletedNetworks, [2], 'failed creation releases the entire partial network');
  } finally { h.close(); }
}
console.log('worker native-channel, consumer ACK, replacement and network failure tests passed');

// Unsafe dispatch must never call arbitrary exports or boot a partial configuration.
for (const experiments of [
  [['esp32sim_delete']], [['unknown']], [['toString']], null, {}, [null],
  [['esp32sim_set_quantum', 0]], [['esp32sim_set_quantum', NaN]],
  [['esp32sim_set_quantum', '64']], [['esp32sim_set_quantum', 64, 1]],
  [['esp32sim_set_approximate_jit_cache', 96, 160, 4]],
  [...HW, ['esp32sim_set_quantum', 64]],
]) {
  let deleted = 0, booted = 0;
  const h = await harness(0, [], experiments, {
    esp32sim_delete() { deleted++; }, esp32sim_boot() { booted++; return 0; },
  });
  try {
    await h.send({ op: 'start' });
    assert.equal(deleted, 0, 'dangerous delete experiment never dispatched');
    assert.equal(booted, 0, 'invalid experiments never boot');
    assert.equal(h.messages.at(-1).started, false, 'start failure acknowledged');
    assert.equal(h.runs.length, 0);
  } finally { h.close(); }
}
for (const failure of ['missing', 'reject', 'throw']) {
  let booted = 0, later = 0;
  const exports = Object.fromEntries(HW.map(([name]) => [name, () => 0]));
  if (failure === 'missing') delete exports[HW[1][0]];
  else exports[HW[1][0]] = () => { if (failure === 'throw') throw Error('setter trap'); return 1; };
  exports[HW[2][0]] = () => { later++; return 0; };
  const h = await harness(0, [], HW, {...exports, esp32sim_boot() { booted++; return 0; }});
  try {
    await h.send({ op: 'start' });
    assert.equal(h.messages.at(-1).started, false, failure);
    assert.equal(booted, 0); assert.equal(later, 0); assert.equal(h.runs.length, 0);
  } finally { h.close(); }
}
{
  const applied = [];
  const h = await harness(0, [], HW, Object.fromEntries(HW.map(([name]) => [name, (_emu, ...args) => { applied.push([name, ...args]); return 0; }])));
  try {
    await h.send({op: 'start'});
    assert.equal(h.messages.at(-1).started, true);
    assert.deepEqual(applied, HW);
  } finally { h.close(); }
}
console.log('worker native-channel, consumer ACK, replacement and experiment tests passed');
