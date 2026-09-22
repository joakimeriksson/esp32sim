import {createFinalFrameCapture} from './final-frame.mjs';
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {runBatteryNode} from './battery-node.mjs';
const opts={};for(let i=2;i<process.argv.length;i+=2){if(!process.argv[i].startsWith('--')||!process.argv[i+1])throw Error('Expected --name value');opts[process.argv[i].slice(2)]=process.argv[i+1];}
for(const key of Object.keys(opts))if(!['wasm','workload','assets','output','mode','max-wall-seconds'].includes(key))throw Error('Unknown argument: '+key);
for(const key of ['wasm','workload','assets','output','mode'])if(!opts[key])throw Error('Missing --'+key);
if(!['pocket-tank','tinydraw'].includes(opts.workload))throw Error('Only canonical same-workload pilots are supported');
if(!['current-jit','historical-interpreter'].includes(opts.mode))throw Error('Invalid --mode');
const maxWallSeconds=Number(opts['max-wall-seconds']??1800);if(!Number.isFinite(maxWallSeconds)||maxWallSeconds<=0)throw Error('Invalid wall limit');
const output=path.resolve(opts.output);if(fs.existsSync(output))throw Error('Output must be a new directory');fs.mkdirSync(output,{recursive:true});
const workload=JSON.parse(fs.readFileSync(new URL('./workloads.json',import.meta.url)))[opts.workload];
const assets=JSON.parse(fs.readFileSync(opts.assets));if(assets.workload!==opts.workload)throw Error('Asset map workload mismatch');
const digest=b=>createHash('sha256').update(b).digest('hex');const inputs={};const serial=createHash('sha256'),frames=createHash('sha256');let frameCount=0;
const finalFrameCapture=createFinalFrameCapture();
const events=fs.openSync(output+'/events.jsonl','wx');
function emit(event){
 if(event.type==='frame'){finalFrameCapture.accept(event);const header=Buffer.alloc(12);header.writeUInt32LE(event.index,0);header.writeUInt32LE(event.kind,4);header.writeUInt32LE(event.bytes.length,8);frames.update(header);frames.update(event.bytes);frameCount++;event={type:'frame',index:event.index,kind:event.kind,bytes:event.bytes.length,sha256:digest(event.bytes)};}
 else if(event.type==='serial')serial.update(event.data);
 fs.writeSync(events,JSON.stringify(event)+'\n');
 if(['progress','capabilities'].includes(event.type)){const safe={...event};delete safe.tail;console.log(JSON.stringify(safe));}
}
try{
 const result=await runBatteryNode(async key=>{const filename=key==='wasm'?opts.wasm:assets[key];if(typeof filename!=='string')throw Error('Missing asset '+key);const bytes=fs.readFileSync(filename);inputs[key]={bytes:bytes.length,sha256:digest(bytes)};return bytes;},emit,true,false,{...workload,name:opts.workload,key:opts.workload},{mode:opts.mode,emitFrameData:true,maxWallSeconds});
 const frameExpected=opts.workload==='pocket-tank'?737:428;
 const checks={verdictPassed:result.passed===true,zeroStopCode:result.stopCode===0,pinnedInstructions:result.instructions===workload.expectedInstructions,pinnedFrames:result.frames===frameExpected,jitFailedZero:result.jit.failed===0,productionJitObserved:opts.mode==='current-jit'?result.jit.compiled>0:null};
 const finalFrame=finalFrameCapture.write(output);
 const receipt={finalFrame,pilotOnly:true,timingComparable:false,reason:'Node compatibility pilot with binary output hashing; not browser timing',runtime:{node:process.version,v8:process.versions.v8},inputs,mode:opts.mode,result,consoleSha256:serial.digest('hex'),binaryFrames:{count:frameCount,framing:'uint32le index, kind, length followed by raw bytes per output',sha256:frames.digest('hex')},checks,compatibleCurrentContract:Object.values(checks).every(v=>v===true||v===null)};
 fs.writeFileSync(output+'/result.json',JSON.stringify(receipt,null,2)+'\n');console.log(JSON.stringify({type:'pilot-complete',passed:receipt.compatibleCurrentContract,status:result.status,guestSeconds:result.guestSeconds,wallSeconds:result.wallSeconds,instructions:result.instructions,frames:result.frames,checks}));if(!receipt.compatibleCurrentContract)process.exitCode=1;
}catch(error){fs.writeFileSync(output+'/error.json',JSON.stringify({message:error.message,stack:error.stack},null,2)+'\n');console.error(JSON.stringify({type:'pilot-error',message:error.message}));process.exitCode=1;}finally{fs.closeSync(events);}
