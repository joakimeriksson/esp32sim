// Production index.html + EmuLink + worker, driven by real CDP mouse events.
// Queue stages pair by sample ID. Changed-pixel observations are per stroke,
// not proof that any particular input was consumed or optically presented.
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
const [url, output, port = '9239', existingTarget] = process.argv.slice(2);
if (!url || !output) throw Error('Usage: capture-page-response.mjs URL OUTPUT_DIRECTORY [HEADLESS_DEBUG_PORT] [RESUME_TARGET_ID]');
await fs.mkdir(output, {recursive: true});
const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
assert.ok(version['User-Agent'].includes('HeadlessChrome/'), 'Use isolated headless Chrome');
const socket = new WebSocket(version.webSocketDebuggerUrl);
await new Promise(resolve => socket.onopen = resolve);
let nextId = 0, targetId;
const pending = new Map();
socket.onmessage = ({data}) => {
  const message = JSON.parse(data);
  if (!message.id) return;
  const task = pending.get(message.id); pending.delete(message.id);
  message.error ? task.reject(Error(JSON.stringify(message.error))) : task.resolve(message.result);
};
socket.onclose = () => { for (const task of pending.values()) task.reject(Error('Browser disconnected during '+task.method)); pending.clear(); };
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
  const id = ++nextId; pending.set(id, {resolve, reject, method});
  socket.send(JSON.stringify({id, method, params, sessionId}));
});
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const percentile = (values, fraction) => values.length ? [...values].sort((a,b) => a-b)[Math.min(values.length-1, Math.ceil(values.length*fraction)-1)] : null;
const stats = values => ({n: values.length, p50: percentile(values,.5), p95: percentile(values,.95), p99: percentile(values,.99), max: values.length ? Math.max(...values) : null});
try {
  if (existingTarget) targetId = existingTarget;
  else ({targetId} = await send('Target.createTarget', {url: 'about:blank', background: false}));
  const {sessionId} = await send('Target.attachToTarget', {targetId, flatten: true});
  const evaluate = async expression => {
    const result = await send('Runtime.evaluate', {expression, returnByValue: true, awaitPromise: true}, sessionId);
    if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  await send('Page.enable', {}, sessionId);
  await send('Page.addScriptToEvaluateOnNewDocument', {source: `
    window.pageCapture = {frames: [], strokes: [], errors: []};
    window.addEventListener('error', e => pageCapture.errors.push(e.message));
    const originalPutImageData = CanvasRenderingContext2D.prototype.putImageData;
    let previousPixels = null;
    CanvasRenderingContext2D.prototype.putImageData = function(im, ...rest) {
      const result = originalPutImageData.call(this, im, ...rest);
      if (this.canvas.id !== 'lcd') return result;
      const atMs = performance.timeOrigin + performance.now();
      let changed = 0, regionChanged = 0;
      const active = pageCapture.strokes.at(-1);
      if (previousPixels && previousPixels.length === im.data.length) {
        for (let i=0;i<im.data.length;i+=4) {
          if (im.data[i] === previousPixels[i] && im.data[i+1] === previousPixels[i+1] && im.data[i+2] === previousPixels[i+2]) continue;
          changed++;
          const x=(i/4)%im.width,y=Math.floor(i/4/im.width);
          if(active && x>=active.x-8 && x<=active.x+128 && Math.abs(y-active.y)<=8) regionChanged++;
        }
      }
      previousPixels = new Uint8ClampedArray(im.data);
      pageCapture.frames.push({atMs,changed,regionChanged,stroke:active?.index ?? null});
      return result;
    };
  `}, sessionId);
  if (!existingTarget) await send('Page.navigate', {url}, sessionId);
  else assert.equal(await evaluate('pageCapture.strokes.length'), 0, 'resume only before the first stroke');
  const started = Date.now();
  while (true) {
    const state = await evaluate(`({ready:typeof lines !== 'undefined' && lines.some(l=>l.text.includes('TINYDRAW_VECTOR_V2_READY')),status:document.querySelector('#status')?.textContent})`);
    if (state.ready) break;
    if (Date.now()-started > 600000) throw Error('Firmware ready timeout: '+JSON.stringify(state));
    console.log(JSON.stringify({bootElapsedMs:Date.now()-started,...state}));
    await sleep(5000);
  }
  await sleep(2000);
  const rect = await evaluate(`(() => { const c=document.querySelector('#lcd'); c.scrollIntoView(); const r=c.getBoundingClientRect(); return {x:r.left,y:r.top,sx:r.width/c.width,sy:r.height/c.height}; })()`);
  const screenshot = async name => {
    const shot = await send('Page.captureScreenshot', {format:'png'}, sessionId);
    await fs.writeFile(output+'/'+name, Buffer.from(shot.data,'base64'));
  };
  await screenshot('page-before.png');
  await evaluate(`window.__esp32simTouchTrace.length=0; pageCapture.frames.length=0;
    window.pageCaptureBaseline=document.querySelector('#lcd').getContext('2d').getImageData(0,0,368,448).data; true;`);
  const mouse = (type, x, y) => send('Input.dispatchMouseEvent', {type,x:rect.x+x*rect.sx,y:rect.y+y*rect.sy,
    button:'left',buttons:type==='mouseReleased'?0:1,clickCount:type==='mouseMoved'?0:1}, sessionId);
  for (let index=0; index<3; index++) {
    const x=80,y=140+index*45;
    await evaluate(`pageCapture.strokes.push({index:${index},x:${x},y:${y},startMs:performance.timeOrigin+performance.now()})`);
    await send('Input.dispatchMouseEvent', {type:'mouseMoved',x:rect.x+x*rect.sx,y:rect.y+y*rect.sy,button:'none',buttons:0}, sessionId);
    await mouse('mousePressed',x,y);
    // CDP transport overhead is part of event scheduling; trace arrival times
    // describe the cadence actually delivered instead of assuming exact 8 ms.
    for (let j=1;j<=120;j++) { await sleep(8); await mouse('mouseMoved',x+j,y); }
    await mouse('mouseReleased',x+120,y);
    await evaluate('pageCapture.strokes.at(-1).upMs=performance.timeOrigin+performance.now()');
    await sleep(2500);
    console.log(JSON.stringify({stroke:index,complete:true}));
  }
  // A short contact confirms delivery/order at the page and worker. Firmware
  // need not report a separate stroke when its own sampling misses the tap.
  await send('Input.dispatchMouseEvent', {type:'mouseMoved',x:rect.x+240*rect.sx,y:rect.y+300*rect.sy,button:'none',buttons:0}, sessionId);
  await mouse('mousePressed',240,300);
  await sleep(8);
  await mouse('mouseReleased',240,300);
  await sleep(2000);
  const receipt = await evaluate(`({url:location.href,trace:window.__esp32simTouchTrace,...pageCapture,serial:lines.filter(l=>l.src==='usb').map(l=>l.text).join('\\n')})`);
  receipt.visibleStrokeColumns = await evaluate(`(() => {
    const c=document.querySelector('#lcd'),pixels=c.getContext('2d').getImageData(0,0,c.width,c.height).data;
    return pageCapture.strokes.map(stroke => {
      let columns=0;
      for(let x=stroke.x;x<=stroke.x+120;x++) {
        let changed=false;
        for(let y=stroke.y-8;y<=stroke.y+8;y++) {
          const i=(y*c.width+x)*4;
          if(pixels[i]!==pageCaptureBaseline[i]||pixels[i+1]!==pageCaptureBaseline[i+1]||pixels[i+2]!==pageCaptureBaseline[i+2]) changed=true;
        }
        if(changed) columns++;
      }
      return columns;
    });
  })()`);
  receipt.provenance = await (await fetch(new URL('/provenance.json',url))).json();
  receipt.browser = version;
  receipt.resumedBeforeStrokes = !!existingTarget;
  receipt.captureHarnessSha256 = createHash('sha256').update(await fs.readFile(new URL(import.meta.url))).digest('hex');
  receipt.endpoint = 'Production page listeners, sampled sends, worker receipt, next run entry, and canvas submission. No controller-consumption or optical timestamps. Per-stroke pixel regions are observations, not sample-to-pixel causal matches.';
  const sent = receipt.trace.filter(e=>e.stage==='page-send');
  const received = new Map(receipt.trace.filter(e=>e.stage==='worker-receive').map(e=>[e.id,e]));
  const run = new Map(receipt.trace.filter(e=>e.stage==='run-entry').flatMap(e=>e.ids.map(id=>[id,e])));
  const exit = new Map(receipt.trace.filter(e=>e.stage==='run-exit').flatMap(e=>e.ids.map(id=>[id,e])));
  receipt.samples = sent.map(e=>({...e,workerReceiveMs:received.get(e.id)?.atMs??null,runEntryMs:run.get(e.id)?.atMs??null,runExitMs:exit.get(e.id)?.atMs??null}));
  const deltas = (a,b) => receipt.samples.filter(e=>Number.isFinite(e[a])&&Number.isFinite(e[b])).map(e=>e[b]-e[a]);
  const changedFrames = receipt.frames.filter(e=>e.changed);
  receipt.strokeInputs = receipt.strokes.map(stroke => {
    const points = receipt.samples.filter(e=>e.arrivalMs>=stroke.startMs&&e.arrivalMs<=stroke.upMs);
    return {index:stroke.index, samples:points.length, events:points.map(e=>e.event), minX:Math.min(...points.map(e=>e.x)), maxX:Math.max(...points.map(e=>e.x))};
  });
  receipt.summary = {
    visibleStrokeColumns:receipt.visibleStrokeColumns,
    inputSamples:sent.length, workerReceived:receipt.samples.filter(e=>e.workerReceiveMs!==null).length,
    runEntered:receipt.samples.filter(e=>e.runEntryMs!==null).length,
    pageSamplingMs:stats(deltas('arrivalMs','sentMs')),
    pageToWorkerMs:stats(deltas('sentMs','workerReceiveMs')),
    workerToRunEntryMs:stats(deltas('workerReceiveMs','runEntryMs')),
    inputRunDurationMs:stats(deltas('runEntryMs','runExitMs')),
    arrivalToRunEntryMs:stats(deltas('arrivalMs','runEntryMs')),
    changedCanvasFrames:changedFrames.length,
    changedFrameGapMs:stats(changedFrames.slice(1).map((f,i)=>f.atMs-changedFrames[i].atMs)),
    framesDuringContacts:receipt.strokes.map(s=>receipt.frames.filter(f=>f.atMs>=s.startMs&&f.atMs<=s.upMs&&f.regionChanged>0).length),
    commitReports:(receipt.serial.match(/TINYDRAW_LIVE_STROKE_DONE committed=1 refresh=1 commit_failed=0/g)||[]).length,
  };
  await fs.writeFile(output+'/page-response.json',JSON.stringify(receipt,null,2));
  await screenshot('page-after.png');
  await fs.writeFile(output+'/canvas-after.png',Buffer.from((await evaluate("document.querySelector('#lcd').toDataURL('image/png')")).split(',')[1],'base64'));
  console.log(JSON.stringify(receipt.summary));
  assert.equal(receipt.errors.length,0,'page errors');
  assert.equal(receipt.summary.workerReceived,sent.length,'all sampled input reaches worker');
  assert.equal(receipt.summary.runEntered,sent.length,'all sampled input has a subsequent run entry');
  assert.ok(receipt.strokeInputs.every(s=>s.samples>3&&s.maxX-s.minX===120&&s.events[0]==='pointerdown'&&s.events.at(-1)==='pointerup'&&!s.events.includes('lostpointercapture')&&!s.events.includes('pointercancel')), 'each full drag must deliver its path and finish with its actual release');
  assert.ok(receipt.visibleStrokeColumns.every(n=>n>=100), 'every stroke must leave a visible path across at least 100 of121 columns');
  assert.ok(receipt.summary.framesDuringContacts.every(n=>n>0),'drawing changes must publish during every continuous stroke');
} finally {
  if (targetId && socket.readyState===WebSocket.OPEN) await send('Target.closeTarget',{targetId});
  socket.close();
}
