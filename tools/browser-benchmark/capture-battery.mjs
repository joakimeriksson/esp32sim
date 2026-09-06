// Uninstrumented battery timing in a dedicated headless Chrome process.
import fs from 'node:fs/promises';
import path from 'node:path';

const [url, output, port = '9228'] = process.argv.slice(2);
if (!url || !output) throw Error('Usage: node capture-battery.mjs URL OUTPUT_DIRECTORY [DEBUG_PORT]');
await fs.mkdir(output, {recursive: true});
const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
if (!version['User-Agent'].includes('HeadlessChrome/')) throw Error('Timing requires headless Chrome');
const ws = new WebSocket(version.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
let sequence = 0, targetId;
const pending = new Map();
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
  const id = ++sequence;
  pending.set(id, {resolve, reject});
  ws.send(JSON.stringify({id, method, params, ...(sessionId ? {sessionId} : {})}));
});
ws.onmessage = ({data}) => {
  const message = JSON.parse(data);
  if (!message.id) return;
  const request = pending.get(message.id);
  pending.delete(message.id);
  if (message.error) request.reject(Error(JSON.stringify(message.error)));
  else request.resolve(message.result);
};
ws.onclose = () => {
  for (const request of pending.values()) request.reject(Error('Chrome connection closed'));
  pending.clear();
};
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const timeout = setTimeout(() => { console.error('Battery capture timed out'); ws.close(); process.exitCode = 1; }, 600_000);
try {
  ({targetId} = await send('Target.createTarget', {url: 'about:blank'}));
  const {sessionId} = await send('Target.attachToTarget', {targetId, flatten: true});
  await send('Page.navigate', {url}, sessionId);
  const evaluate = async expression => (await send('Runtime.evaluate', {expression, returnByValue: true}, sessionId)).result.value;
  const deadline = Date.now() + 30_000;
  while (!await evaluate('!!window.worker')) {
    if (Date.now() > deadline) throw Error('Battery page did not initialize');
    await sleep(100);
  }
  await evaluate('worker.postMessage({start:true,jit:new URL(location.href).searchParams.get("jit")!=="0"});true');
  let result;
  while (!result) {
    await sleep(3000);
    const state = await evaluate('({result:window.result,error:window.events?.find(e=>e.type==="error"),progress:window.events?.filter(e=>e.type==="progress").at(-1)})');
    if (state?.error) throw Error(state.error.line);
    result = state?.result;
    console.log(JSON.stringify(state));
  }
  await fs.writeFile(path.join(output, 'result.json'), JSON.stringify({captureMode:'timing', version, result}, null, 2));
  await fs.writeFile(path.join(output, 'events.json'), JSON.stringify(await evaluate('window.events')));
  if (!result.passed) process.exitCode = 1;
} finally {
  if (targetId && ws.readyState === WebSocket.OPEN) await send('Target.closeTarget', {targetId});
  clearTimeout(timeout);
  ws.close();
}
