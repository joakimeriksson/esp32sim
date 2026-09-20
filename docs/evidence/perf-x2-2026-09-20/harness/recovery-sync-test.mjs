import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
const root=fs.mkdtempSync(path.join(os.tmpdir(),'x2-sync-test-'));
fs.mkdirSync(root+'/m3');fs.mkdirSync(root+'/queue');fs.mkdirSync(root+'/bin');
fs.copyFileSync('/Users/alice/src/a/esp32sim-x2/m3/sync-jobs.sh',root+'/m3/sync-jobs.sh');
fs.copyFileSync('/Users/alice/src/a/esp32sim-x2/queue/jobs.jsonl',root+'/queue/jobs.jsonl');
fs.symlinkSync('/Users/alice/src/a/esp32sim-x2/.venv',root+'/.venv');
fs.copyFileSync('/tmp/x2-remote-jobs-before.txt',root+'/remote');
fs.writeFileSync(root+'/bin/ssh',`#!/bin/bash\nif [[ "$*" == *'cat ~/bench/esp32sim/x2/jobs.txt'* ]]; then /bin/cat "$MOCK_ROOT/remote"; elif [[ "$*" == *'shasum -a 256'* ]]; then exit "\${MOCK_CHECK_FAIL:-0}"; elif [[ "$*" == *'mv jobs.txt.new'* ]]; then cp "$MOCK_ROOT/m3/jobs.next.txt" "$MOCK_ROOT/remote"; echo published >> "$MOCK_ROOT/actions"; fi\n`,{mode:0o755});
fs.writeFileSync(root+'/bin/rsync','#!/bin/bash\necho rsync >> "$MOCK_ROOT/actions"\n',{mode:0o755});
const env={...process.env,PATH:root+'/bin:'+process.env.PATH,MOCK_ROOT:root};
function run(extra={}){return spawnSync('bash',[root+'/m3/sync-jobs.sh'],{env:{...env,...extra},encoding:'utf8'});}
let r=run({MOCK_CHECK_FAIL:'1'});if(r.status===0||fs.readFileSync(root+'/actions','utf8').includes('published'))throw Error('checksum failure published jobs');
r=run();if(r.status!==0)throw Error(r.stderr);const jobs=fs.readFileSync(root+'/remote','utf8').split('\n').filter(x=>x&&!x.startsWith('#'));if(jobs.length!==41||new Set(jobs.map(x=>x.split(' ')[0])).size!==41)throw Error('expected 41 unique jobs');
const actionCount=fs.readFileSync(root+'/actions','utf8').split('\n').length;r=run();if(r.status!==0||!r.stdout.includes('nothing new')||fs.readFileSync(root+'/actions','utf8').split('\n').length!==actionCount)throw Error('not idempotent');
console.log('PASS: checksum failure blocks publication; 16 new candidates become 41 unique jobs; second sync makes no transfers. Mock directory: '+root);
