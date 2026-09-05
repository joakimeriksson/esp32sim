// Exercise the shipped page listeners with real CDP mouse input and a captured
// transport. Does not boot firmware or measure guest/optical response latency.
// Run against an independently launched headless Chrome and `python3 -m
// http.server --directory web PORT`.
import assert from 'node:assert/strict';
const [url, port = '9228'] = process.argv.slice(2);
if (!url) throw Error('Usage: node check-page-input.mjs PAGE_URL [DEBUG_PORT]');
const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
assert.ok(version['User-Agent'].includes('HeadlessChrome/'), 'Use isolated headless Chrome: visible-page timers are required without taking user focus');
const socket = new WebSocket(version.webSocketDebuggerUrl);
await new Promise(resolve => socket.onopen = resolve);
let nextId = 0;
const pending = new Map();
socket.onmessage = ({data}) => {
  const message = JSON.parse(data);
  if (!message.id) return;
  const task = pending.get(message.id); pending.delete(message.id);
  message.error ? task.reject(Error(JSON.stringify(message.error))) : task.resolve(message.result);
};
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
  const id = ++nextId; pending.set(id, {resolve, reject});
  socket.send(JSON.stringify({id, method, params, sessionId}));
});
let targetId;
try {
  ({targetId} = await send('Target.createTarget', {url: 'about:blank', background: false}));
  const {sessionId} = await send('Target.attachToTarget', {targetId, flatten: true});
  const evaluate = async expression => {
    const result = await send('Runtime.evaluate', {expression, returnByValue: true, awaitPromise: true}, sessionId);
    if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  await send('Page.enable', {}, sessionId);
  await send('Page.addScriptToEvaluateOnNewDocument', {source: `
    window.sentTouches = [];
    document.addEventListener('pointerdown', e => { if (e.isTrusted) window.activePointer = e.pointerId; }, true);
    window.EmuLink = { connect: () => ({ send: data => {
      if (typeof data === 'string') { const m = JSON.parse(data); if (m.t === 'touch') window.sentTouches.push({...m, wall: performance.now()}); }
    } }) };
  `}, sessionId);
  await send('Page.navigate', {url}, sessionId);
  for (let i = 0; i < 100 && !await evaluate('typeof installTouchInput === "function" && document.readyState === "complete"'); i++) await new Promise(r => setTimeout(r, 20));
  assert.ok(await evaluate('typeof installTouchInput === "function" && document.readyState === "complete"'), 'page initialization timeout');
  const rect = await evaluate(`(() => {
    document.querySelector('#lcdpanel').style.display = '';
    const c = document.querySelector('#lcd'); c.scrollIntoView();
    const r = c.getBoundingClientRect(); return {x:r.left+30,y:r.top+30};
  })()`);
  const mouse = (type, offset = 0) => send('Input.dispatchMouseEvent', {
    type, x: rect.x + offset, y: rect.y, button: 'left',
    buttons: type === 'mouseReleased' ? 0 : 1, clickCount: type === 'mouseMoved' ? 0 : 1,
  }, sessionId);
  await mouse('mousePressed');
  // Dense burst on the real page. Synthetic moves reuse the active mouse ID;
  // the actual press above establishes capture without replacing browser APIs.
  await evaluate(`(() => { const c = document.querySelector('#lcd'); const r=c.getBoundingClientRect();
    for(let x=31;x<=60;x++) c.dispatchEvent(new PointerEvent('pointermove', {pointerId:window.activePointer,clientX:r.left+x,clientY:r.top+30}));
  })()`);
  await new Promise(r => setTimeout(r, 100));
  let events = await evaluate('window.sentTouches');
  assert.equal(events[0].down, 1);
  assert.equal(events.at(-1).x, 60, 'latest move must arrive even without another event');
  assert.equal(events.at(-1).down, 1);
  assert.ok(events.length <= 3, 'dense moves must be sampled');
  await mouse('mouseReleased', 30);
  events = await evaluate('window.sentTouches');
  assert.equal(events.at(-1).down, 0);
  await evaluate('window.sentTouches.length = 0');
  await mouse('mousePressed');
  await evaluate(`(() => { const c = document.querySelector('#lcd'); const r=c.getBoundingClientRect();
    c.dispatchEvent(new PointerEvent('pointerdown', {pointerId:99,clientX:r.left+200,clientY:r.top+30}));
    c.dispatchEvent(new PointerEvent('pointerup', {pointerId:99,clientX:r.left+200,clientY:r.top+30}));
    c.dispatchEvent(new PointerEvent('pointermove', {pointerId:window.activePointer,clientX:r.left+55,clientY:r.top+30}));
    c.dispatchEvent(new PointerEvent('pointercancel', {pointerId:window.activePointer,clientX:r.left+55,clientY:r.top+30}));
  })()`);
  events = await evaluate('window.sentTouches');
  assert.deepEqual(events.map(({x,down}) => [x,down]), [[30,1],[55,1],[55,0]], 'cancel flushes pending point; other contacts cannot release it');
  await new Promise(r => setTimeout(r, 80));
  assert.equal((await evaluate('window.sentTouches')).length, 3, 'no stale timer after cancel');
  await mouse('mouseReleased', 25);
  await evaluate(`window.sentTouches.length=0; window.pointerLifecycle=[];
    for (const type of ['pointerdown','pointermove','pointerup','gotpointercapture','lostpointercapture'])
      document.querySelector('#lcd').addEventListener(type, e => pointerLifecycle.push({type:e.type,id:e.pointerId,buttons:e.buttons,captured:e.currentTarget.hasPointerCapture(e.pointerId)}));`);
  for (let index=0;index<3;index++) {
    await send('Input.dispatchMouseEvent', {type:'mouseMoved',x:rect.x+index*10,y:rect.y,button:'none',buttons:0}, sessionId);
    await mouse('mousePressed',index*10);
    await mouse('mouseMoved',index*10+5);
    await mouse('mouseMoved',index*10+10);
    await mouse('mouseReleased',index*10+10);
    await new Promise(r=>setTimeout(r,50));
  }
  events = await evaluate('window.sentTouches');
  assert.deepEqual(events.map(({x,down})=>[x,down]), [[30,1],[40,1],[40,0],[40,1],[50,1],[50,0],[50,1],[60,1],[60,0]], 'successive real drags keep their contact');
  const lifecycle = await evaluate('window.pointerLifecycle');
  assert.ok(lifecycle.filter(e=>e.type==='pointermove'&&e.buttons===1).every(e=>e.captured), 'CDP drag events must preserve actual capture');
  if (new URL(url).searchParams.has('touchTrace')) {
    await evaluate(`(() => { for (let i=0;i<2100;i++) window.recordTouchTrace({stage:'test',i}); })()`);
    assert.equal(await evaluate('window.__esp32simTouchTrace.length'), 2048, 'trace retention is bounded');
  }
  console.log(JSON.stringify({passed: true, browser: version.Browser, checks: ['trailing dense move', 'bounded sampling', 'release', 'cancel flush', 'single contact', 'no stale timer', 'successive captured drags'], scope: 'actual index.html listeners, captured transport; no firmware latency assertion'}));
} finally {
  if (targetId) await send('Target.closeTarget', {targetId});
  socket.close();
}
