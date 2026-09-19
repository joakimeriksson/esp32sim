// Uses the production worker, including pacing and the input queue.
const canvas = document.querySelector('canvas'), ctx = canvas.getContext('2d');
const status = document.querySelector('#status');
const receipt = {startedAt: performance.now(), environment: {userAgent: navigator.userAgent, crossOriginIsolated}, inputs: [], frames: [], strokes: [], serial: ''};
window.receipt = receipt;
let ready = false, replayed = false, previous = null, lastChange = 0, active = null, serial = '', frameId = 0;
const worker = new Worker('/web/wasm/worker.js', {type: 'module'});
const waiters = [];
function command(message, key) {
  return new Promise(resolve => { waiters.push({key, resolve}); worker.postMessage(message); });
}
function touch(x, y, down, kind) {
  const event = {at: performance.now(), x, y, down, kind};
  receipt.inputs.push(event);
  if (active && kind !== 'up') active.pending.push(event);
  worker.postMessage({op: 'text', data: JSON.stringify({t: 'touch', x, y, down: down ? 1 : 0})});
  return event;
}
function frame(buf) {
  const at = performance.now(), dv = new DataView(buf);
  if (dv.getUint8(0) !== 1) return;
  const w = dv.getUint16(1, true), h = dv.getUint16(3, true);
  if (buf.byteLength !== 5 + w * h * 2) throw Error('Invalid frame size');
  const pixels = new Uint16Array(w * h), im = ctx.createImageData(w, h), matched = new Set();
  let changed = 0, roiChanged = 0, strokeChanged = 0;
  for (let i = 0; i < pixels.length; i++) {
    const p = dv.getUint16(5 + i * 2, true); pixels[i] = p;
    if (previous && previous[i] !== p) {
      changed++;
      if (active) {
        const x = i % w, y = Math.floor(i / w);
        if (Math.abs(x - active.x) < 14 && Math.abs(y - active.y) < 14) roiChanged++;
        if (x >= active.x - 14 && x <= active.x + 134 && Math.abs(y - active.y) < 14) strokeChanged++;
        for (const event of active.pending) {
          if (at >= event.at && Math.abs(x - event.x) <= 4 && Math.abs(y - event.y) <= 4) matched.add(event);
        }
      }
    }
    im.data[i * 4] = (p >> 11) * 255 / 31;
    im.data[i * 4 + 1] = ((p >> 5) & 63) * 255 / 63;
    im.data[i * 4 + 2] = (p & 31) * 255 / 31;
    im.data[i * 4 + 3] = 255;
  }
  if (changed) lastChange = at;
  previous = pixels;
  if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
  ctx.putImageData(im, 0, 0);
  const event = {id: ++frameId, receivedAt: at, canvasSubmittedAt: performance.now(), changed, roiChanged, strokeChanged};
  receipt.frames.push(event);
  if (active) {
    if (roiChanged && !active.firstChangedFrame) {
      active.firstChangedFrame = event.id;
      active.inputToFrameMs = at - active.downAt;
      active.inputToCanvasMs = event.canvasSubmittedAt - active.downAt;
    }
    if (strokeChanged) active.lastStrokeCanvasAt = event.canvasSubmittedAt;
    for (const input of matched) {
      input.changedFrame = event.id;
      input.canvasSubmittedAt = event.canvasSubmittedAt;
      input.inputToCanvasMs = event.canvasSubmittedAt - input.at;
    }
    active.pending = active.pending.filter(input => !matched.has(input));
  }
  requestAnimationFrame(t => { event.nextAnimationFrameTimestamp = t; event.animationFrameCallbackAt = performance.now(); });
}
worker.onmessage = ({data: message}) => {
  for (let i = waiters.length - 1; i >= 0; i--) {
    if (message[waiters[i].key] !== undefined) { const [pending] = waiters.splice(i, 1); pending.resolve(message); }
  }
  if (message.bin) frame(message.bin);
  if (message.text) {
    const event = JSON.parse(message.text);
    if (event.t === 'serial' && event.src === 'usb') {
      serial += event.data; receipt.serial = serial;
      if (active?.upAt && !active.commitReportedAt && /TINYDRAW_LIVE_STROKE_DONE committed=1 refresh=1 commit_failed=0/.test(serial.slice(active.serialStart))) active.commitReportedAt = performance.now();
      if (!ready && serial.includes('TINYDRAW_VECTOR_V2_READY')) {
        ready = true; window.ready = true; receipt.readyAt = performance.now(); receipt.bootWallMs = receipt.readyAt - receipt.startedAt;
        status.textContent = 'Ready. Drag on the drawing or replay the fixed strokes.';
        document.querySelector('#replay').disabled = false;
      }
    }
  }
  if (message.log) receipt.logs = (receipt.logs || []).concat(message.log);
  if (message.stopped) { status.textContent = 'Emulator stopped: ' + message.stopped; receipt.stopped = message.stopped; }
};
worker.onerror = event => { status.textContent = event.message; receipt.error = event.message; };
receipt.assets = await (await fetch('/assets.json')).json();
const wasm = await (await fetch('/asset/wasm')).arrayBuffer();
await command({op: 'init', wasm}, 'ready');
const HW = [["esp32sim_set_approximate_jit_timing",1,512],["esp32sim_set_approximate_jit_frontiers",1],["esp32sim_set_approximate_jit_cache",96,160,2],["esp32sim_set_approximate_cache_contention",1],["esp32sim_set_approximate_cache_fill_service",160],["esp32sim_set_spi2_timing",1],["esp32sim_set_measured_te",1],["esp32sim_set_control_prices",1],["esp32sim_set_icache_fill",404]];
const timingParam = new URL(location.href).searchParams.get('timing');
// `timing=hw` is the whole model; `timing=hw-<n>[-<m>...]` drops the listed exports (for bisecting).
const dropped = (timingParam || '').split('-').slice(1).map(Number);
const experiments = timingParam && timingParam.startsWith('hw') ? HW.filter((_, n) => !dropped.includes(n)) : [];
await command({op: 'create', board: 'waveshare-amoled18-v2', flash_mb: 16, psram_mb: 8, experiments}, 'created');
for (const [name, kind] of [['rom', 0], ['bootloader', 1], ['ptable', 2], ['app', 3], ['elf', 4]]) {
  const data = await (await fetch('/asset/' + name)).arrayBuffer();
  const result = await command({op: 'load', kind, data}, 'loaded');
  if (!result.ok) throw Error('Load failed: ' + name);
}
await command({op: 'start'}, 'started');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function replay() {
  if (!ready || replayed) return;
  replayed = true; document.querySelector('#replay').disabled = true;
  for (let n = 0; n < 3; n++) {
    const started = performance.now();
    while (performance.now() - lastChange < 1000 && performance.now() - started < 10000) await sleep(50);
    const x = 80, y = 140 + n * 45;
    active = {index: n, x, y, quietBeforeMs: performance.now() - lastChange, serialStart: serial.length, pending: [], inputStart: receipt.inputs.length};
    receipt.strokes.push(active);
    active.downAt = touch(x, y, true, 'down').at;
    await sleep(250);
    for (let j = 1; j <= 8; j++) { const event = touch(x + j * 15, y, true, 'move'); if (j === 1) active.firstMoveAt = event.at; await sleep(100); }
    active.upAt = touch(x + 120, y, false, 'up').at;
    await sleep(2000);
    active.serial = serial.slice(active.serialStart);
    active.committed = /TINYDRAW_LIVE_STROKE_DONE committed=1 refresh=1 commit_failed=0/.test(active.serial);
    active.upToCommitReportMs = active.commitReportedAt ? active.commitReportedAt - active.upAt : null;
    active.upToLastCanvasChangeMs = active.lastStrokeCanvasAt > active.upAt ? active.lastStrokeCanvasAt - active.upAt : null;
    active.doneAt = performance.now(); active = null;
  }
  const moves = receipt.inputs.filter(input => input.kind === 'move');
  receipt.summary = {
    strokes: receipt.strokes.length,
    responded: receipt.strokes.filter(stroke => Number.isFinite(stroke.inputToCanvasMs)).length,
    committed: receipt.strokes.filter(stroke => stroke.committed).length,
    downToCanvasMs: receipt.strokes.map(stroke => stroke.inputToCanvasMs ?? null),
    movementPoints: moves.length, movementResponded: moves.filter(input => Number.isFinite(input.inputToCanvasMs)).length,
    movementToCanvasMs: moves.map(input => input.inputToCanvasMs ?? null),
    upToCommitReportMs: receipt.strokes.map(stroke => stroke.upToCommitReportMs),
    upToLastCanvasChangeMs: receipt.strokes.map(stroke => stroke.upToLastCanvasChangeMs),
    endpoint: 'Queued input to changed pixels near its position submitted to canvas; lift report is firmware console arrival. Not optical presentation.',
  };
  document.querySelector('#summary').textContent = JSON.stringify(receipt.summary, null, 2);
  window.finished = true;
}
window.replay = replay; document.querySelector('#replay').onclick = replay;
let down = false;
const pos = event => { const rect = canvas.getBoundingClientRect(); return [Math.round((event.clientX - rect.left) * canvas.width / rect.width), Math.round((event.clientY - rect.top) * canvas.height / rect.height)]; };
canvas.onpointerdown = event => { if (!ready || active) return; down = true; canvas.setPointerCapture(event.pointerId); touch(...pos(event), true, 'manual'); };
canvas.onpointermove = event => { if (down) touch(...pos(event), true, 'manual'); };
canvas.onpointerup = canvas.onpointercancel = event => { if (down) { touch(...pos(event), false, 'manual'); down = false; } };
document.querySelector('#save').onclick = () => {
  const link = document.createElement('a'); link.href = URL.createObjectURL(new Blob([JSON.stringify(receipt, null, 2)], {type: 'application/json'}));
  link.download = 'drawing-response.json'; link.click(); URL.revokeObjectURL(link.href);
};
