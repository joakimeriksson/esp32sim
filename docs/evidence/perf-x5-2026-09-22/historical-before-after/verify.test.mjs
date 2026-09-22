import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import test from 'node:test';
import assert from 'node:assert/strict';
import {verifyResults} from './verify.mjs';
const root = process.env.HISTORICAL_EVIDENCE_DIR || fileURLToPath(new URL('.', import.meta.url));
const read = name => JSON.parse(readFileSync(resolve(root, name)));
const index = read('results.json');
const data = Object.fromEntries([...index.jobFiles, 'qualification.json', 'provenance.json'].map(name => [name, read(name)]));
const check = (d = data, i = index) => verifyResults(i, name => d[name]);
test('all retained historical receipts pass, preserving different retired work', () => {
  assert.deepEqual(check(), {status: 'passed', jobs: 3, timedArms: 24});
  assert.notEqual(data['earliest-vs-current.json'].contracts.baseline.instructions, data['earliest-vs-current.json'].contracts.candidate.instructions);
});
test('warm-up timing does not contribute to primary summaries', () => {
  const d = structuredClone(data);
  for (const r of d['earliest-vs-current.json'].runs.filter(r => r.stage === 'warmup')) {
    r.wallSeconds = r.result.wallSeconds = 100000;
    r.realtimeRatio = r.guestSeconds / r.wallSeconds;
  }
  assert.deepEqual(check(d), {status: 'passed', jobs: 3, timedArms: 24});
});
const mutations = {
  'numeric strings and false headline': job => {
    for (const r of job.runs) r.wallSeconds = r.result.wallSeconds = String(r.wallSeconds);
    job.wallReductionPercent = 90; job.medianWallSeconds = {baseline: 999, candidate: 1};
  },
  'NaN time': j => { j.runs[0].wallSeconds = j.runs[0].result.wallSeconds = NaN; },
  'infinite time': j => { j.runs[0].wallSeconds = j.runs[0].result.wallSeconds = Infinity; },
  'zero time': j => { j.runs[0].wallSeconds = j.runs[0].result.wallSeconds = 0; },
  'NaN headline': j => { j.wallReductionPercent = NaN; },
  'wrong primary median': j => { j.medianWallSeconds.baseline++; },
  'wrong pair reduction': j => { j.pairsWallReductionPercent[0]++; },
  'missing pair reduction': j => { j.pairsWallReductionPercent.pop(); },
  'warm-up relabeled primary': j => { j.runs[0].stage = 'primary'; },
  'duplicate pair member': j => { j.runs[1] = structuredClone(j.runs[0]); },
  'incorrect order': j => { j.runs.reverse(); },
  'row/result timing mismatch': j => { j.runs[0].wallSeconds++; },
  'wrong retired work': j => { j.runs[0].instructions = ++j.runs[0].result.instructions; },
  'wrong frame count': j => { j.runs[0].frames = ++j.runs[0].result.frames; },
  'wrong console hash': j => { j.runs[0].consoleSha256 = '0'.repeat(64); },
  'truthy success': j => { j.runs[0].passed = j.runs[0].result.passed = 1; },
  'instrumented capture': j => { j.runs[0].result.instrumented = true; },
  'missing JIT failure counter': j => { delete j.runs[0].result.jit.failed; },
  'boolean zero compiled counter': j => { j.runs[0].jitCompiled = j.runs[0].result.jit.compiled = false; },
  'invalid verdict status': j => { j.runs[0].result.verdictValidation.valid = false; },
  'forged missing-counter semantics': j => { j.runs[0].result.capabilities.missingCountersAreZero = true; j.runs[0].capabilities.missingCountersAreZero = true; },
  'wrong input hash': j => { j.runs[0].result.provenance.sha256['asset/app'] = '0'.repeat(64); j.runs[0].provenance.sha256['asset/app'] = '0'.repeat(64); },
  'wrong artifact': j => { j.contracts.baseline.wasmSha256 = '0'.repeat(64); },
  'wrong final image qualification': j => { j.contracts.baseline.qualification.finalFrame.rgbaSha256 = '0'.repeat(64); },
  'malformed otherwise passing verdict': j => { j.contracts.baseline.verdict = j.contracts.baseline.verdict.replace('native_kernels=1', 'native_kernels=1=0'); },
};
for (const [name, mutate] of Object.entries(mutations)) test(`rejects ${name}`, () => {
  const d = structuredClone(data); mutate(d['earliest-vs-current.json']); assert.throws(() => check(d));
});
test('first shipped JIT must actually compile and execute guest instructions', () => {
  for (const field of ['compiled', 'instructions']) {
    const d = structuredClone(data), r = d['first-jit-vs-current.json'].runs[0];
    r.result.jit[field] = 0; if (field === 'compiled') r.jitCompiled = 0;
    assert.throws(() => check(d));
  }
});
test('retains both negative qualifications without upgrading them to accepted', () => {
  const d = structuredClone(data); d['qualification.json'].pilots[0].accepted = true;
  assert.throws(() => check(d));
  const missing = structuredClone(data); missing['qualification.json'].pilots.shift();
  assert.throws(() => check(missing));
});
test('rejects missing job and wrong total', () => {
  assert.throws(() => check(data, {...index, timedArms: 22}));
  assert.throws(() => check(data, {...index, jobFiles: index.jobFiles.slice(1)}));
});
