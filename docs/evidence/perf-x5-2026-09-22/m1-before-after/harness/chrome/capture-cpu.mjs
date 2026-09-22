import fs from 'node:fs/promises';
import path from 'node:path';

const [url, output, port = '9228', mode = 'battery'] = process.argv.slice(2);
if (!url || !output) throw Error('Usage: node capture-cpu.mjs URL OUTPUT_DIRECTORY [DEBUG_PORT] [battery|drawing]');
if (!['battery', 'drawing'].includes(mode)) throw Error('Unknown capture mode');
await fs.mkdir(output, {recursive: true});
const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const ws = new WebSocket(version.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
let sequence = 0, workerSession, targetId;
const pending = new Map();
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
  const id = ++sequence;
  pending.set(id, {resolve, reject});
  ws.send(JSON.stringify({id, method, params, ...(sessionId ? {sessionId} : {})}));
});
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
ws.onclose = () => { for (const request of pending.values()) request.reject(Error('Chrome connection closed')); pending.clear(); };
let profilerReady, profilerError;
ws.onmessage = async ({data}) => {
  const message = JSON.parse(data);
  if (message.id) {
    const request = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) request.reject(Error(JSON.stringify(message.error)));
    else request.resolve(message.result);
  } else if (message.method === 'Target.attachedToTarget' && message.params.targetInfo.type === 'worker') {
    try {
      workerSession = message.params.sessionId;
      await send('Profiler.enable', {}, workerSession);
      await send('Profiler.setSamplingInterval', {interval: 1000}, workerSession);
      if (mode === 'battery') await send('Profiler.start', {}, workerSession);
      await send('Runtime.runIfWaitingForDebugger', {}, workerSession);
      profilerReady = true;
    } catch (error) { profilerError = error; }
  }
};
// A missing worker, failed fetch or stalled firmware must not leave this job running forever.
const timeout = setTimeout(() => { console.error('CPU capture timed out'); ws.close(); process.exitCode = 1; }, 600_000);
try {
  ({targetId} = await send('Target.createTarget', {url: 'about:blank', background: !version['User-Agent'].includes('HeadlessChrome/')}));
  const {sessionId} = await send('Target.attachToTarget', {targetId, flatten: true});
  await send('Target.setAutoAttach', {autoAttach: true, waitForDebuggerOnStart: true, flatten: true}, sessionId);
  await send('Page.navigate', {url,background:true}, sessionId);
  const startupDeadline = Date.now() + 30_000;
  while (!profilerReady) {
    if (profilerError) throw profilerError;
    if (Date.now() > startupDeadline) throw Error('Worker profiler did not initialize');
    await sleep(100);
  }
  if (mode === 'drawing') {
    let ready = false;
    while (!ready) {
      await sleep(3000);
      ready = (await send('Runtime.evaluate', {expression: '!!window.ready', returnByValue: true}, sessionId)).result.value;
    }
    await sleep(3000);
    await send('Profiler.start', {}, workerSession);
    await send('Runtime.evaluate', {expression: 'window.replay();true'}, sessionId);
  } else {
    await send('Runtime.evaluate', {expression: 'worker.postMessage({start:true})'}, sessionId);
  }
  let result;
  while (!result) {
    await sleep(3000);
    const response = await send('Runtime.evaluate', {
      expression: mode === 'drawing' ? '({result:window.finished ? window.receipt.summary : null})' : '({result:window.result,progress:window.events?.filter(e=>e.type==="progress").at(-1)})',
      returnByValue: true,
    }, sessionId);
    result = response.result.value?.result;
    console.log(JSON.stringify(response.result.value));
  }
  const {profile} = await send('Profiler.stop', {}, workerSession);
  await fs.writeFile(path.join(output, `${mode}.cpuprofile`), JSON.stringify(profile));
  await fs.writeFile(path.join(output, 'result.json'), JSON.stringify({version, result}, null, 2));
  const events = await send('Runtime.evaluate', {expression: mode === 'drawing' ? 'window.receipt' : 'window.events', returnByValue: true}, sessionId);
  await fs.writeFile(path.join(output, 'events.json'), JSON.stringify(events.result.value));
  if (mode === 'drawing') {
    const shot = await send('Page.captureScreenshot', {format: 'png'}, sessionId);
    await fs.writeFile(path.join(output, 'drawing.png'), Buffer.from(shot.data, 'base64'));
    if (result.committed !== 3) process.exitCode = 1;
  } else if (!result.passed) process.exitCode = 1;
} finally {
  if (targetId && ws.readyState === WebSocket.OPEN) await send('Target.closeTarget', {targetId});
  clearTimeout(timeout);
  ws.close();
}
