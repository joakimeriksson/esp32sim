import fs from 'node:fs';
function memory(path) {
  const b=fs.readFileSync(path); let p=8;
  const u=()=>{let n=0,s=0,c;do{c=b[p++];n+=(c&127)*2**s;s+=7;}while(c&128);return n;};
  while(p<b.length){const id=b[p++],size=u(),end=p+size;if(id!==11){p=end;continue;}
    const n=u(),segments=[];
    for(let i=0;i<n;i++) {const flag=u();if(flag===2)u();else if(flag!==0)throw new Error('Unexpected data mode');if(b[p++]!==65)throw new Error('Expected i32.const');const addr=u();if(b[p++]!==11)throw new Error('Expected end');const len=u();segments.push({addr,bytes:b.subarray(p,p+len)});p+=len;}
    const mem=Buffer.alloc(Math.max(...segments.map(s=>s.addr+s.bytes.length)));for(const s of segments)s.bytes.copy(mem,s.addr);return mem;
  }
}
const [a,b]=process.argv.slice(2).map(memory), out=[];
for(let i=0;i<a.length;i++)if(a[i]!==b[i]){
 const lineAddr=i-i%4, pointer=a.readUInt32LE(lineAddr-8),len=a.readUInt32LE(lineAddr-4);
 const filename=a.subarray(pointer,pointer+len).toString();
 const before=a.readUInt32LE(lineAddr),after=b.readUInt32LE(lineAddr);
 if(!filename.endsWith('.rs')||after!==before+2)throw new Error('Not a source-line change at '+i);
 out.push({address:i,filename,beforeLine:before,afterLine:after});
}
console.log(JSON.stringify({changedBytes:out.length,allDifferencesAreSourceLinesPlusTwo:true,changes:out},null,2));
