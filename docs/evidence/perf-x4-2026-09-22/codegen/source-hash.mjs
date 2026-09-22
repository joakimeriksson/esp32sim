import fs from 'node:fs';
import {createHash} from 'node:crypto';
const hash=b=>createHash('sha256').update(b).digest('hex');
const members=JSON.parse(fs.readFileSync('Cargo.toml','utf8').match(/members = (\[.*\])/)[1]);
const files=['Cargo.toml','Cargo.lock'];
function visit(p){for(const e of fs.readdirSync(p,{withFileTypes:true})){if(e.name==='target')continue;const f=p+'/'+e.name;if(e.isDirectory())visit(f);else if(e.isFile()&&(f.endsWith('.rs')||e.name==='Cargo.toml'))files.push(f);}}
for(const m of members)visit(m);
const rows=files.sort().map(f=>hash(fs.readFileSync(f))+' '+f+'\n').join('');
console.log(JSON.stringify({files:files.length,sourceSha256:hash(rows)}));
