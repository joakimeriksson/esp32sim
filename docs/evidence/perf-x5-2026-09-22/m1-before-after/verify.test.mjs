import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import test from 'node:test';
import assert from 'node:assert/strict';
import {verifyResults} from './verify.mjs';
// With an explicit directory this also tests a curator output before publication.
const root = process.env.M1_EVIDENCE_DIR || fileURLToPath(new URL('.', import.meta.url));
const read = name => JSON.parse(readFileSync(resolve(root, name)));
const index = read('results.json'), provenance = read('provenance.json');
const jobs = Object.fromEntries(index.browsers.flatMap(b => b.results.map(e => [e.receipt, read(e.receipt)])));
const check = (i = index, j = jobs, p = provenance) => verifyResults(i, name => j[name], p);
test('all public M1 receipts verify without changing measured values', () => {
  assert.deepEqual(check(), {status: 'passed', jobs: 12, timedArms: 56});
});
const mutations = {
  'original review exploit: numeric strings and fabricated headline': (e, j) => {
    for (const run of j.runs) run.wallSeconds = String(run.wallSeconds);
    e.wallReductionPercent = j.wallReductionPercent = 40;
    e.medianWallSeconds = j.medianWallSeconds = {baseline: 999, candidate: 1};
  },
  'NaN wall time': (e, j) => { j.runs[0].wallSeconds = NaN; },
  'infinite wall time': (e, j) => { j.runs[0].wallSeconds = Infinity; },
  'zero wall time': (e, j) => { j.runs[0].wallSeconds = 0; },
  'negative wall time': (e, j) => { j.runs[0].wallSeconds = -1; },
  'NaN summary': (e, j) => { j.wallReductionPercent = NaN; },
  'missing index median': e => { delete e.medianWallSeconds.baseline; },
  'wrong index pair reduction': e => { e.pairsWallReductionPercent[0] += 1; },
  'wrong index workload': e => { e.workload = 'unknown'; },
  'duplicate arm': (e, j) => { j.runs[1] = structuredClone(j.runs[0]); },
  'wrong pair order': (e, j) => { j.runs.reverse(); },
  'missing JIT failures': (e, j) => { delete j.runs[0].jit.failed; },
  'no compiled JIT code': (e, j) => { j.runs[0].jit.compiled = 0; },
  'truthy passed': (e, j) => { j.runs[0].passed = 'true'; },
  'instrumented arm': (e, j) => { j.runs[0].instrumented = true; },
  'different console': (e, j) => { j.runs[0].consoleSha256 = 'a'.repeat(64); },
  'wrong artifact per arm': (e, j) => { j.runs[0].wasmSha256 = 'a'.repeat(64); },
  'wrong source revision': (e, j) => { j.baselineSourceRevision = 'a'.repeat(40); },
  'wrong instruction count': (e, j) => { j.runs[0].instructions--; },
  'wrong frame count': (e, j) => { j.runs[0].frames--; },
};
for (const [name, mutate] of Object.entries(mutations)) test(`rejects ${name}`, () => {
  const i = structuredClone(index), j = structuredClone(jobs);
  const e = i.browsers[0].results.find(e => e.name === 'before-after-pocket');
  mutate(e, j[e.receipt]);
  assert.throws(() => check(i, j));
});
test('rejects an A/A job using the other artifact', () => {
  const j = structuredClone(jobs);
  const job = j['chrome/control-pocket.json'];
  job.candidateWasmSha256 = provenance.artifacts.after.wasmSha256;
  job.candidateSourceRevision = provenance.artifacts.after.sourceRevision;
  for (const run of job.runs) if (run.arm === 'candidate') run.wasmSha256 = job.candidateWasmSha256;
  assert.throws(() => check(index, j));
});
test('rejects missing or duplicated campaign entries', () => {
  const i = structuredClone(index);
  i.browsers[1] = structuredClone(i.browsers[0]);
  assert.throws(() => check(i));
  const truncated = structuredClone(index);
  truncated.browsers[0].results.pop();
  assert.throws(() => check(truncated));
});
