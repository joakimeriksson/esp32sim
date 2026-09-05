import { createJitHost } from './jit.mjs';
import { createPacing } from './pacing.mjs';
let pacing = createPacing();
// Optional host-stage diagnostics. Run entry is not controller consumption;
// framebuffer output is not physical panel scanout. Epoch times match the page.
let traceEnabled = false, pendingInputTrace = [];
const traceNow = () => performance.timeOrigin + performance.now();
// esp32sim in a Web Worker: owns the wasm instance, paces it to wall time, and relays the UI
// protocol (docs/web-ui.md) to the page as postMessage — text as strings, binary as ArrayBuffers.
let CPU_HZ = 240e6;   // replaced from the module once an emulator exists: the C3 runs at 160 MHz
let wasm = null, emu = 0, running = false, t0 = 0, resyncs = 0, lastStat = { wall: 0, insns: 0 };
let net = 0, netNodes = 0, netT0 = 0;   // a network of motes: several emulators on one medium
const enc = new TextEncoder(), dec = new TextDecoder();
const mem = () => new Uint8Array(wasm.memory.buffer);
function put(bytes) { const p = wasm.esp32sim_alloc(bytes.length); mem().set(bytes, p); return p; }
function withBytes(bytes, f) { const p = put(bytes); try { return f(p, bytes.length); } finally { wasm.esp32sim_free(p, bytes.length); } }
const blockJit = createJitHost(() => wasm);
const imports = { env: { ...blockJit.imports, host_log: (p, n) => postMessage({ log: dec.decode(mem().subarray(p, p + n)) }) } };

function drain() {
  const n = wasm.esp32sim_out_take(emu);
  for (let i = 0; i < n; i++) {
    const kind = wasm.esp32sim_out_kind(emu, i), p = wasm.esp32sim_out_ptr(emu, i), len = wasm.esp32sim_out_len(emu, i);
    if (kind === 1) postMessage({ text: dec.decode(mem().subarray(p, p + len)) });
    else {
      const buf = new ArrayBuffer(len);
      new Uint8Array(buf).set(mem().subarray(p, p + len));
      const frameTrace = traceEnabled && new Uint8Array(buf)[0] === 1
        ? { stage: 'worker-frame', atMs: traceNow(), cycles: wasm.esp32sim_cycles(emu) } : undefined;
      postMessage({ bin: buf, frameTrace }, [buf]);
    }
  }
}

function loop() {
  if (!running) return;
  const now = performance.now();
  let cur = wasm.esp32sim_cycles(emu);
  let target = (now - t0) / 1000 * CPU_HZ;
  if (target - cur > CPU_HZ * 0.5) { t0 = now - cur / CPU_HZ * 1000; target = cur + CPU_HZ * 0.02; resyncs++; }   // hopelessly behind: skip, don't burst
  const turnMs = pacing.turnMs(now);
  while (cur < target) {
    const before = performance.now();
    const remaining = pacing.sliceCycles(target - cur, turnMs - (before - now), now);
    const previous = cur;
    const inputTraceIds = pendingInputTrace;
    if (inputTraceIds.length) { postMessage({ touchTrace: { stage: 'run-entry', ids: inputTraceIds, atMs: traceNow(), cycles: cur } }); pendingInputTrace = []; }
    const rc = wasm.esp32sim_run(emu, remaining, Date.now());
    cur = wasm.esp32sim_cycles(emu);
    if (inputTraceIds.length) postMessage({ touchTrace: { stage: 'run-exit', ids: inputTraceIds, atMs: traceNow(), cycles: cur } });
    pacing.observe(cur - previous, performance.now() - before);
    drain();
    if (rc !== 0) { running = false; postMessage({ stopped: rc }); return; }
    if (performance.now() - now >= turnMs) break;                       // let messages flow, come back
  }
  const aheadMs = cur / CPU_HZ * 1000 - (performance.now() - t0);
  const wall = performance.now();
  if (wall - lastStat.wall > 1000) {
    const insns = wasm.esp32sim_insns(emu);
    postMessage({ pace: { behind: Math.max(0, -aheadMs / 1000), resyncs, mips: Math.max(0, (insns - lastStat.insns)) / (wall - lastStat.wall) / 1000 } });
    lastStat = { wall, insns };
  }
  setTimeout(loop, Math.max(0, Math.min(20, aheadMs)));
}

// A network runs on its own clock: network time in nanoseconds paced to the wall clock, with
// each node's console and LED drained per turn. The medium and the stepping are in the module
// (esp32c6::net) — this only says how far to run and relays what came out.
function netLoop() {
  if (!running) return;
  const now = performance.now();
  const targetNs = (now - netT0) * 1e6;
  const curNs = wasm.esp32sim_net_now_ns(net);
  if (targetNs - curNs > 5e8) { netT0 = now - curNs / 1e6; resyncs++; }   // hopelessly behind: skip
  const turnStart = performance.now();
  while (wasm.esp32sim_net_now_ns(net) < targetNs) {
    wasm.esp32sim_net_run(net, Math.min(targetNs, wasm.esp32sim_net_now_ns(net) + 5e6));   // 5 ms of network time
    if (performance.now() - turnStart >= 12) break;
  }
  const stats = [];
  for (let i = 0; i < netNodes; i++) {
    const len = wasm.esp32sim_net_console_take(net, i);
    if (len) { const p = wasm.esp32sim_net_console_ptr(net); postMessage({ netText: { node: i, data: dec.decode(mem().subarray(p, p + len)) } }); }
    stats.push({ tx: wasm.esp32sim_net_stat(net, i, 0), rx: wasm.esp32sim_net_stat(net, i, 1),
                 dropped: wasm.esp32sim_net_stat(net, i, 2), ns: wasm.esp32sim_net_stat(net, i, 3),
                 led: wasm.esp32sim_net_stat(net, i, 4), halted: wasm.esp32sim_net_stat(net, i, 6) !== 0 });
  }
  const wall = performance.now();
  if (wall - lastStat.wall > 500) {
    // `t` is the protocol's message type, so the network clock travels as `sec`
    postMessage({ netStat: { sec: wasm.esp32sim_net_now_ns(net) / 1e9, nodes: stats, resyncs,
                             behind: Math.max(0, (targetNs - wasm.esp32sim_net_now_ns(net)) / 1e9) } });
    lastStat = { wall, insns: 0 };
  }
  setTimeout(netLoop, 5);
}

onmessage = async (ev) => {
  const m = ev.data;
  try {
    if (m.op === 'init') { traceEnabled = !!m.touchTrace; const r = await WebAssembly.instantiate(m.wasm, imports); wasm = r.instance.exports;  postMessage({ ready: true }); }
    else if (m.op === 'create') {
      running = false;
      pacing = createPacing();
      pendingInputTrace = [];
      if (emu) { wasm.esp32sim_delete(emu); emu = 0; }
      emu = withBytes(enc.encode(m.board), (p, n) => wasm.esp32sim_new(p, n, m.flash_mb | 0, m.psram_mb | 0));
      if (emu !== 0) wasm.esp32sim_set_jit(emu, m.jit === false ? 0 : 1);
      if (emu !== 0 && wasm.esp32sim_cpu_hz) CPU_HZ = wasm.esp32sim_cpu_hz(emu);
      postMessage({ created: emu !== 0 });
    }
    else if (m.op === 'load') { const rc = withBytes(new Uint8Array(m.data), (p, n) => m.at !== undefined ? wasm.esp32sim_load_at(emu, m.at >>> 0, p, n) : wasm.esp32sim_load(emu, m.kind, p, n)); postMessage({ loaded: m.at !== undefined ? 'at' + m.at : m.kind, ok: rc === 0 }); }
    else if (m.op === 'stub') { withBytes(enc.encode(m.name), (p, n) => wasm.esp32sim_stub(emu, p, n, m.value >>> 0)); }
    else if (m.op === 'wifi') { withBytes(enc.encode(m.spec), (p, n) => wasm.esp32sim_wifi(emu, p, n)); }
    else if (m.op === 'start') { const rc = wasm.esp32sim_boot(emu, m.appDirect ? 1 : 0); if (rc === 0) { running = true; t0 = performance.now(); loop(); } postMessage({ started: rc === 0 }); }
    else if (m.op === 'net-create') {
      running = false;
      if (net) { wasm.esp32sim_net_delete(net); net = 0; }
      if (emu) { wasm.esp32sim_delete(emu); emu = 0; }
      net = wasm.esp32sim_net_new(m.slice_ns || 0);
      netNodes = 0;
      for (const node of m.nodes) {
        const mac = (node.mac || '02:00:00:00:00:0' + (netNodes + 1)).split(':').map(h => parseInt(h, 16));
        withBytes(new Uint8Array(mac), (mp) => withBytes(enc.encode(node.board || m.board || 'none'),
          (bp, bn) => wasm.esp32sim_net_add(net, mp, node.flash_mb || m.flash_mb || 2, (node.start_ms || 0) * 1e6, node.x || 0, node.y || 0, bp, bn)));
        netNodes++;
      }
      postMessage({ created: net !== 0, nodes: netNodes });
    }
    else if (m.op === 'net-load') { const rc = withBytes(new Uint8Array(m.data), (p, n) => wasm.esp32sim_net_load(net, m.node, m.kind, p, n)); postMessage({ loaded: 'n' + m.node + ':' + m.kind, ok: rc === 0 }); }
    else if (m.op === 'net-stub') { withBytes(enc.encode(m.name), (p, n) => wasm.esp32sim_net_stub(net, m.node, p, n, m.value >>> 0)); }
    else if (m.op === 'net-start') { const rc = wasm.esp32sim_net_boot(net); if (rc === 0) { running = true; netT0 = performance.now(); netLoop(); } postMessage({ started: rc === 0 }); }
    else if (m.op === 'stop') { running = false; }
    else if (m.op === 'text') {
      if (traceEnabled && m.touchTrace) {
        postMessage({ touchTrace: { stage: 'worker-receive', ...m.touchTrace, atMs: traceNow(), cycles: wasm.esp32sim_cycles(emu) } });
        if (pendingInputTrace.length < 2048) pendingInputTrace.push(m.touchTrace.id);
      }
      pacing.input(performance.now());
      withBytes(enc.encode(m.data), (p, n) => wasm.esp32sim_in_text(emu, p, n));
    }
    else if (m.op === 'bin') { pacing.input(performance.now()); withBytes(new Uint8Array(m.data), (p, n) => wasm.esp32sim_in_bin(emu, p, n)); }
  } catch (err) { postMessage({ log: '[worker] ' + (err && err.stack || err) }); running = false; }
};
