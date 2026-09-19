// usage: drawcheck.mjs URL OUT.png [port]  — one 1.5 s diagonal drag on run.html's panel; reports input and frame cadence
import fs from 'node:fs/promises';
const [url, out, port='9240'] = process.argv.slice(2);
const v=await(await fetch(`http://127.0.0.1:${port}/json/version`)).json();const ws=new WebSocket(v.webSocketDebuggerUrl);await new Promise(r=>ws.onopen=r);
let id=0;const pending=new Map();
ws.onmessage=({data})=>{const m=JSON.parse(data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}};
const send=(method,params={},sessionId)=>new Promise((resolve,reject)=>{pending.set(++id,{resolve,reject});ws.send(JSON.stringify({id,method,params,sessionId}));});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const {targetId}=await send('Target.createTarget',{url:'about:blank'});const {sessionId}=await send('Target.attachToTarget',{targetId,flatten:true});
const ev=async e=>(await send('Runtime.evaluate',{expression:e,returnByValue:true},sessionId)).result.value;
await send('Page.enable',{},sessionId);
await send('Emulation.setDeviceMetricsOverride',{width:1300,height:1000,deviceScaleFactor:1,mobile:false},sessionId);
await send('Page.addScriptToEvaluateOnNewDocument',{source:`window.cap={frames:[]};const o=CanvasRenderingContext2D.prototype.putImageData;let prev=null;
CanvasRenderingContext2D.prototype.putImageData=function(im,...r){const x=o.call(this,im,...r);if(im.width!==368||im.height!==448)return x;let ch=0;if(prev&&prev.length===im.data.length){for(let i=0;i<im.data.length;i+=16){if(im.data[i]!==prev[i]||im.data[i+1]!==prev[i+1]||im.data[i+2]!==prev[i+2]){ch++;}}}prev=new Uint8ClampedArray(im.data);cap.frames.push({t:performance.now(),ch});return x;};`},sessionId);
await send('Page.navigate',{url},sessionId);
for(let i=0;i<60;i++){await sleep(2000);if(await ev(`typeof lines!=='undefined'&&lines.some(l=>l.text.includes('TINYDRAW_VECTOR_V2_READY'))`))break;}
await sleep(3000);
const rect=await ev(`(()=>{const c=document.querySelector('#lcd');c.scrollIntoView();const r=c.getBoundingClientRect();return {x:r.left,y:r.top,sx:r.width/c.width,sy:r.height/c.height};})()`);
const mouse=(type,x,y)=>send('Input.dispatchMouseEvent',{type,x:rect.x+x*rect.sx,y:rect.y+y*rect.sy,button:'left',buttons:type==='mouseReleased'?0:1,clickCount:type==='mouseMoved'?0:1},sessionId);
await ev('cap.frames.length=0;cap.t0=performance.now();true');
await mouse('mousePressed',60,120);
const stats=[];for(let j=1;j<=600;j++){await sleep(8);const a=j/180*Math.PI*2;await mouse('mouseMoved',60+(j%180)*1.2,200+80*Math.sin(a*1.5));if(j%100===0)stats.push((await ev(`document.getElementById('stat').textContent`)).replace(/.*insns · /,''));}
console.log('speed during drag: '+stats.join(' || '));
await ev('cap.t1=performance.now();true');
await mouse('mouseReleased',60+216,200);
await sleep(2500);
const r=await ev(`({frames:cap.frames,t0:cap.t0,t1:cap.t1,speed:document.body.innerText.match(/[0-9.]+ Minsn\\/s[^\\n]*/)?.[0]})`);
const during=r.frames.filter(f=>f.t>=r.t0&&f.t<=r.t1+100&&f.ch>0);const gaps=during.slice(1).map((f,i)=>f.t-during[i].t).sort((a,b)=>a-b);
const pct=p=>gaps.length?gaps[Math.min(gaps.length-1,Math.ceil(gaps.length*p)-1)].toFixed(1):null;
console.log(JSON.stringify({contactMs:Math.round(r.t1-r.t0),changedFramesDuringContact:during.length,fps:(during.length/((r.t1-r.t0)/1000)).toFixed(1),gapP50:pct(.5),gapP95:pct(.95),gapMax:pct(1),speed:r.speed}));
const shot=await send('Page.captureScreenshot',{format:'png',clip:{x:rect.x-10,y:rect.y-10,width:368*rect.sx+20,height:448*rect.sy+20,scale:1}},sessionId);await fs.writeFile(out,Buffer.from(shot.data,'base64'));
await send('Target.closeTarget',{targetId});ws.close();
