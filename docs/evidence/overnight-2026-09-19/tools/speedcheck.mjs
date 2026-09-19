// usage: speedcheck.mjs URL [port] [seconds] — samples the page's own speed readout once per 5 s
const [url, port='9240', secs='45'] = process.argv.slice(2);
const v=await(await fetch(`http://127.0.0.1:${port}/json/version`)).json();const ws=new WebSocket(v.webSocketDebuggerUrl);await new Promise(r=>ws.onopen=r);
let id=0;const pending=new Map();
ws.onmessage=({data})=>{const m=JSON.parse(data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}};
const send=(method,params={},sessionId)=>new Promise((resolve,reject)=>{pending.set(++id,{resolve,reject});ws.send(JSON.stringify({id,method,params,sessionId}));});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const {targetId}=await send('Target.createTarget',{url:'about:blank'});const {sessionId}=await send('Target.attachToTarget',{targetId,flatten:true});
await send('Emulation.setDeviceMetricsOverride',{width:1300,height:1000,deviceScaleFactor:1,mobile:false},sessionId);
await send('Page.enable',{},sessionId);await send('Page.navigate',{url},sessionId);
const out=[];
for(let t=5;t<=Number(secs);t+=5){await sleep(5000);const r=(await send('Runtime.evaluate',{expression:`(document.getElementById('pace')?.textContent||'')+' | '+(document.getElementById('stat')?.textContent||'')`,returnByValue:true},sessionId)).result.value;out.push(`t+${t}s: ${r}`);}
console.log(out.join('\n'));
await send('Target.closeTarget',{targetId});ws.close();
