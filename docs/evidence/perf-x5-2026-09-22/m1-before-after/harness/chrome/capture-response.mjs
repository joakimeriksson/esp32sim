import fs from 'node:fs/promises';
const [url, output, port = '9228'] = process.argv.slice(2);
if (!url || !output) throw Error('Usage: capture-response.mjs URL OUTPUT_DIRECTORY [DEBUG_PORT]');
await fs.mkdir(output, {recursive:true});
const v=await(await fetch(`http://127.0.0.1:${port}/json/version`)).json();const ws=new WebSocket(v.webSocketDebuggerUrl);await new Promise(r=>ws.onopen=r);let id=0;const pending=new Map();ws.onmessage=({data})=>{const m=JSON.parse(data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}};
ws.onclose=()=>{for(const p of pending.values())p.reject(Error('Chrome disconnected'));pending.clear();};
const send=(method,params={},sessionId)=>new Promise((resolve,reject)=>{pending.set(++id,{resolve,reject});ws.send(JSON.stringify({id,method,params,sessionId}));});
const {targetId}=await send('Target.createTarget',{url,background:!v['User-Agent'].includes('HeadlessChrome/')});const {sessionId}=await send('Target.attachToTarget',{targetId,flatten:true});
const timeout=setTimeout(()=>{ws.close();process.exitCode=1;},600000);
try {
const evaluate=async expression=>(await send('Runtime.evaluate',{expression,returnByValue:true},sessionId)).result.value;
let ready=false;const start=Date.now();while(!ready){if(Date.now()-start>600000)throw Error('Startup timeout');await new Promise(r=>setTimeout(r,5000));const s=await evaluate('({ready:window.ready,status:document.querySelector("#status").textContent,tail:window.receipt?.serial.slice(-180)})');console.log(JSON.stringify(s));ready=s.ready;}
await new Promise(r=>setTimeout(r,3000));
const shot=await send('Page.captureScreenshot',{format:'png'},sessionId);await fs.writeFile(`${output}/drawing-before.png`,Buffer.from(shot.data,'base64'));
await evaluate('window.replay();true');
while(!await evaluate('window.finished'))await new Promise(r=>setTimeout(r,1000));
const receipt=await evaluate('window.receipt');receipt.captureBrowser=v;await fs.writeFile(`${output}/drawing-response.json`,JSON.stringify(receipt,null,2));console.log(JSON.stringify(receipt.summary));
const after=await send('Page.captureScreenshot',{format:'png'},sessionId);await fs.writeFile(`${output}/drawing-after.png`,Buffer.from(after.data,'base64'));
if (receipt.summary.committed !== 3) process.exitCode=1;
} finally { clearTimeout(timeout); if (ws.readyState === WebSocket.OPEN) await send('Target.closeTarget',{targetId}); ws.close(); }
