import {readFileSync,writeFileSync} from 'node:fs';
const [dir,symbolFile,out]=process.argv.slice(2);
const profile=JSON.parse(readFileSync(`${dir}/battery.cpuprofile`));
const events=JSON.parse(readFileSync(`${dir}/events.json`));
const result=JSON.parse(readFileSync(`${dir}/result.json`)).result;
const nodes=new Map(profile.nodes.map(n=>[n.id,n]));
const parents=new Map();
for(const n of profile.nodes) for(const c of n.children||[]) parents.set(c,n.id);
const cache=new Map();
function ancestry(id){if(cache.has(id))return cache.get(id);const ns=[];for(let n=id;n;n=parents.get(n))ns.push(nodes.get(n).callFrame.functionName);const a={run:ns.includes('esp32sim_run'),guest:ns.find(n=>/^xtensa_[0-9a-f]{8}$/.test(n))};cache.set(id,a);return a;}
let t=0,lastRun=0;
const samples=profile.samples.map((id,i)=>{const dt=profile.timeDeltas[i]/1000;t+=dt;const a=ancestry(id);if(a.run)lastRun=t;return {t,dt,id,...a};});
// The last esp32sim_run sample is within roughly one sample interval of execution
// completion. This removes the capture tool's up-to-three-second stop-poll delay.
const offsetMs=lastRun-result.wallSeconds*1000;
const marker=events.findIndex(e=>e.type==='serial'&&e.data.includes('TINYDRAW_GATE1_WORKLOAD kind=realistic'));
const end=events[marker].wallMs;
let before=marker-1;while(events[before].type!=='serial')before--;
const start=events[before].wallMs;
const syms=readFileSync(symbolFile,'utf8').split('\n').map(l=>l.match(/^([0-9a-f]+) ([0-9a-f]+) [tTwW] (.+)$/)).filter(Boolean).map(m=>({lo:parseInt(m[1],16),size:parseInt(m[2],16),name:m[3]})).sort((a,b)=>a.lo-b.lo);
function symbol(pc){let lo=0,hi=syms.length;while(lo<hi){const mid=(lo+hi)>>1;if(syms[mid].lo<=pc)lo=mid+1;else hi=mid;}const s=syms[lo-1];return s&&pc<s.lo+s.size?s.name:`0x${pc.toString(16)}`;}
const all=new Map(),guest=new Map(),timeline=[];
let sampledMs=0,guestMs=0;
for(const s of samples){const wall=s.t-offsetMs;if(wall<start||wall>end)continue;sampledMs+=s.dt;const name=nodes.get(s.id).callFrame.functionName;all.set(name,(all.get(name)||0)+s.dt);if(s.guest){const pc=parseInt(s.guest.slice(7),16),name=symbol(pc);guest.set(name,(guest.get(name)||0)+s.dt);guestMs+=s.dt;timeline.push({wallMs:wall,ms:s.dt,pc:pc.toString(16),name});}}
const ranked=m=>[...m].sort((a,b)=>b[1]-a[1]).map(([name,ms])=>({name,ms,percentOfWindow:100*ms/sampledMs}));
const summary={source:dir,offsetMs,startWallMs:start,endWallMs:end,sampledMs,guestAttributedMs:guestMs,scope:'Between previous serial marker and realistic workload print; includes workload population before the measured load_us. End aligned to last esp32sim_run sample, approximate to sampling granularity.',guest:ranked(guest),exclusive:ranked(all),timeline};
writeFileSync(out,JSON.stringify(summary,null,2)+'\n');
console.log(JSON.stringify({...summary,timeline:undefined,exclusive:summary.exclusive.slice(0,8),guest:summary.guest.slice(0,16)},null,2));
