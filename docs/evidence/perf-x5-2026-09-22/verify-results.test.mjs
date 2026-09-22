import {readFileSync} from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import {verifyResults} from './verify-results.mjs';
import {matchesReference} from './exact-contract.mjs';
const read = path => JSON.parse(readFileSync(new URL(path, import.meta.url)));
const index = read('results.json');
const jobs = Object.fromEntries(index.map(e => [e.name, read(`runs/${e.name}.json`)]));
const gates = read('exact-gates.json');
const check = (i = index, j = jobs, g = gates) => verifyResults(i, name => j[name], g);
test('all retained receipts verify without changing measured values', () => {
  assert.deepEqual([check().jobs, check().timedArms], [37, 254]);
});
const mutations = {
  'review reproduction: strings with fabricated summaries': (e, j) => {
    for (const run of j.runs) run.wallSeconds = String(run.wallSeconds);
    e.wallReductionPercent = j.wallReductionPercent = 40;
    e.medianWallSeconds = j.medianWallSeconds = {baseline: 999, candidate: 1};
  },
  'numeric strings': (e, j) => { j.runs[0].wallSeconds = String(j.runs[0].wallSeconds); },
  'NaN': (e, j) => { j.runs[0].wallSeconds = NaN; },
  'infinite wall time': (e, j) => { j.runs[0].wallSeconds = Infinity; },
  'zero wall time': (e, j) => { j.runs[0].wallSeconds = 0; },
  'negative wall time': (e, j) => { j.runs[0].wallSeconds = -1; },
  'NaN summary': (e, j) => { j.wallReductionPercent = NaN; },
  'missing index median': e => { delete e.medianWallSeconds.baseline; },
  'wrong index pair reduction': e => { e.pairsWallReductionPercent[0] += 1; },
  'unknown workload': (e, j) => { e.workload = j.workload = 'unknown'; },
  'mismatched index workload': e => { e.workload = 'tinydraw'; },
  'duplicate pair member': (e, j) => { j.runs[1] = structuredClone(j.runs[0]); },
  'wrong order': (e, j) => { j.runs.reverse(); },
  'missing JIT counter': (e, j) => { delete j.runs[0].jit.failed; },
  'truthy verdict': (e, j) => { j.runs[0].passed = 'true'; },
  'missing console hashes': (e, j) => { for (const r of j.runs) delete r.consoleSha256; },
  'ungated artifact': (e, j) => { e.candidateWasmSha256 = j.candidateWasmSha256 = 'a'.repeat(64); },
  'A/A different artifact': (e, j) => { e.candidateWasmSha256 = j.candidateWasmSha256 = gates[1].wasmSha256; },
};
for (const [name, mutate] of Object.entries(mutations)) test(`rejects ${name}`, () => {
  const i = structuredClone(index), j = structuredClone(jobs);
  mutate(i[0], j[i[0].name]);
  assert.throws(() => check(i, j));
});
test('rejects failed or missing exact gates', () => {
  assert.throws(() => check(index, jobs, []));
  assert.throws(() => check(index, jobs, gates.map(g => ({...g, jitFailures: null}))));
});
test('strict exactness requires zero JIT failures and panics on both runs', () => {
  const valid = {insns: '123', consoleSha256: 'abc', frames: 2, framesSha256: 'def', panics: 0, jitFailures: 0};
  assert.equal(matchesReference(valid, valid), true);
  for (const value of [undefined, null, false, '0', 1, NaN]) {
    assert.equal(matchesReference({...valid, jitFailures: value}, valid), false);
    assert.equal(matchesReference(valid, {...valid, jitFailures: value}), false);
  }
  assert.equal(matchesReference(valid, {...valid, panics: 1}), false);
  assert.equal(matchesReference({...valid, framesSha256: 'changed'}, valid), false);
});
