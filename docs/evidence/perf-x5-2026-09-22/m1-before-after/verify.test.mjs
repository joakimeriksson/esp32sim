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
  assert.deepEqual(check(), {status: 'passed', jobs: index.jobs, timedArms: index.timedArms});
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
// Exercise the extension even when replaying the historical two-browser receipts.
// Synthetic Firefox metadata is used only in tests and is never a measurement.
function withFirefox() {
  const i = structuredClone(index), j = structuredClone(jobs);
  if (i.browsers.some(b => b.browser === 'firefox')) return [i, j];
  const browser = structuredClone(i.browsers.find(b => b.browser === 'safari'));
  browser.browser = 'firefox';
  browser.captureTransport = 'Firefox ordinary browser page';
  browser.isolation = 'Ordinary visible page; fresh page and worker per arm; browser process and caches may persist';
  for (const entry of browser.results) {
    const job = structuredClone(j[entry.receipt]);
    entry.receipt = `firefox/${entry.name}.json`;
    Object.assign(job, {browserFamily: 'firefox', captureTransport: browser.captureTransport, isolation: browser.isolation});
    for (const run of job.runs) Object.assign(run, {browser: 'Firefox/156.0', engine: 'SpiderMonkey', v8: null, pageWasHidden: false, captureTransport: browser.captureTransport});
    j[entry.receipt] = job;
  }
  i.browsers.push(browser); i.jobs = 18; i.timedArms = 84;
  return [i, j];
}
test('accepts the three-browser receipt schema', () => {
  const [i, j] = withFirefox();
  assert.deepEqual(check(i, j), {status: 'passed', jobs: 18, timedArms: 84});
});
for (const [field, value] of [['browser', 'Safari/26.0'], ['browser', 'Firefox/..'], ['engine', 'JavaScriptCore'], ['v8', '15.0'], ['pageWasHidden', true], ['pageWasHidden', undefined], ['captureTransport', 'Safari ordinary browser page']])
  test(`rejects Firefox ${field}=${String(value)}`, () => {
    const [i, j] = withFirefox();
    j['firefox/before-after-pocket.json'].runs[0][field] = value;
    assert.throws(() => check(i, j));
  });
test('rejects Firefox metadata mismatched consistently across index, job and arms', () => {
  const [i, j] = withFirefox();
  i.browsers.find(b => b.browser === 'firefox').captureTransport = 'unknown';
  for (const [path, job] of Object.entries(j)) if (path.startsWith('firefox/')) {
    job.captureTransport = 'unknown';
    for (const run of job.runs) run.captureTransport = 'unknown';
  }
  assert.throws(() => check(i, j));
});
