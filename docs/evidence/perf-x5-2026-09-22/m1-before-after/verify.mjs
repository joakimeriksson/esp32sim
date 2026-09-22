// Recompute the public M1 receipts; this checks retained data, not machine isolation.
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const requireThat = (ok, label) => { if (!ok) throw new Error(label); };
const finite = value => typeof value === 'number' && Number.isFinite(value);
const same = (actual, expected, label) => requireThat(finite(actual) && finite(expected) && Math.abs(actual - expected) <= 1e-9, label);
const median = values => {
  const sorted = values.toSorted((a, b) => a - b), middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
const hash = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
export function verifyResults(index, readJob, provenance) {
  requireThat(Array.isArray(index.browsers) && [2, 3].includes(index.browsers.length), 'campaign browsers');
  const expectedBrowsers = index.browsers.length === 2 ? ['chrome', 'safari'] : ['chrome', 'safari', 'firefox'];
  requireThat(index.jobs === expectedBrowsers.length * 6 && index.timedArms === expectedBrowsers.length * 28, 'campaign totals');
  const artifacts = provenance.artifacts;
  for (const arm of ['before', 'after'])
    requireThat(hash(artifacts[arm]?.wasmSha256) && /^[a-f0-9]{40}$/.test(artifacts[arm]?.sourceRevision), `${arm} provenance`);
  const browsers = new Set();
  let jobs = 0, arms = 0;
  for (const browser of index.browsers) {
    requireThat(expectedBrowsers.includes(browser.browser) && !browsers.has(browser.browser), 'browser identity');
    browsers.add(browser.browser);
    requireThat(browser.jobs === 6 && browser.arms === 28 && Array.isArray(browser.results) && browser.results.length === 6, 'browser totals');
    const names = new Set();
    for (const entry of browser.results) {
      const match = /^(warmup|control|before-after)-(pocket|tinydraw)$/.exec(entry.name);
      requireThat(match && !names.has(entry.name), 'job identity');
      names.add(entry.name);
      const [, kind, suffix] = match;
      const workload = suffix === 'pocket' ? 'pocket-tank' : 'tinydraw';
      const pairs = {warmup: 1, control: 2, 'before-after': 4}[kind];
      const role = {warmup: 'retained warm-up; excluded from headline', control: 'A/A control', 'before-after': 'primary comparison'}[kind];
      requireThat(entry.receipt === `${browser.browser}/${entry.name}.json`, 'receipt path');
      const job = readJob(entry.receipt), label = field => `${entry.receipt}: ${field}`;
      for (const field of ['name', 'workload', 'role', 'pairs']) requireThat(job[field] === entry[field], label(`index ${field}`));
      requireThat(job.browserFamily === browser.browser && job.workload === workload && job.pairs === pairs && job.role === role, label('job metadata'));
      for (const field of ['isolation', 'captureTransport']) requireThat(typeof job[field] === 'string' && job[field].length > 0 && job[field] === browser[field], label(`browser ${field}`));
      if (browser.browser === 'firefox') {
        requireThat(job.captureTransport === 'Firefox ordinary browser page', label('Firefox transport'));
        requireThat(job.isolation === 'Ordinary visible page; fresh page and worker per arm; browser process and caches may persist', label('Firefox isolation'));
      }
      for (const arm of ['baseline', 'candidate']) {
        const artifact = artifacts[arm === 'baseline' || kind === 'control' ? 'before' : 'after'];
        requireThat(job[`${arm}WasmSha256`] === artifact.wasmSha256 && job[`${arm}SourceRevision`] === artifact.sourceRevision, label(`${arm} provenance`));
      }
      if (kind === 'control') requireThat(job.baselineWasmSha256 === job.candidateWasmSha256, label('A/A identity'));
      requireThat(Array.isArray(job.runs) && job.runs.length === pairs * 2, label('arm count'));
      requireThat(Array.isArray(job.pairsWallReductionPercent) && job.pairsWallReductionPercent.length === pairs && Array.isArray(entry.pairsWallReductionPercent) && entry.pairsWallReductionPercent.length === pairs, label('pair reductions'));
      for (const [i, run] of job.runs.entries()) {
        const pair = Math.floor(i / 2) + 1;
        const arm = (pair % 2 === 1) === (i % 2 === 0) ? 'baseline' : 'candidate';
        requireThat(run.pair === pair && run.arm === arm, label('ordered pairs'));
        requireThat(finite(run.wallSeconds) && run.wallSeconds > 0, label('positive finite wallSeconds'));
        requireThat(run.instructions === (suffix === 'pocket' ? 10073833775 : 9819885134) && run.frames === (suffix === 'pocket' ? 737 : 428), label('pinned work'));
        requireThat(run.passed === true && run.instrumented === false && run.status === 'completed' && run.stopCode === 0 && run.captureMode === 'timing', label('completion'));
        requireThat(run.jit?.failed === 0 && Number.isInteger(run.jit?.compiled) && run.jit.compiled > 0, label('JIT status'));
        requireThat(run.verdictValidation?.valid === true && run.verdictValidation?.passed === true && run.verdictValidation?.error === null, label('verdict'));
        requireThat(run.wasmSha256 === job[`${arm}WasmSha256`] && hash(run.consoleSha256), label('arm hashes'));
        requireThat(run.captureTransport === job.captureTransport, label('capture transport'));
        if (browser.browser === 'firefox')
          requireThat(typeof run.browser === 'string' && /^Firefox\/[0-9]+(?:\.[0-9]+)*$/.test(run.browser) && run.engine === 'SpiderMonkey' && run.v8 === null && run.pageWasHidden === false, label('Firefox metadata and visibility'));
      }
      requireThat(new Set(job.runs.map(run => run.consoleSha256)).size === 1, label('console equality'));
      const medians = Object.fromEntries(['baseline', 'candidate'].map(arm => [arm, median(job.runs.filter(run => run.arm === arm).map(run => run.wallSeconds))]));
      for (const arm of ['baseline', 'candidate']) {
        same(medians[arm], job.medianWallSeconds?.[arm], label(`${arm} median`));
        same(medians[arm], entry.medianWallSeconds?.[arm], label(`index ${arm} median`));
      }
      const reduction = 100 * (1 - medians.candidate / medians.baseline);
      same(reduction, job.wallReductionPercent, label('headline'));
      same(reduction, entry.wallReductionPercent, label('index headline'));
      for (let pair = 1; pair <= pairs; pair++) {
        const runs = job.runs.slice((pair - 1) * 2, pair * 2);
        const reduction = 100 * (1 - runs.find(r => r.arm === 'candidate').wallSeconds / runs.find(r => r.arm === 'baseline').wallSeconds);
        same(reduction, job.pairsWallReductionPercent[pair - 1], label(`pair ${pair}`));
        same(reduction, entry.pairsWallReductionPercent[pair - 1], label(`index pair ${pair}`));
      }
      jobs++; arms += job.runs.length;
    }
  }
  requireThat(jobs === index.jobs && arms === index.timedArms, 'recomputed totals');
  return {status: 'passed', jobs, timedArms: arms};
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = process.argv[2] ? resolve(process.argv[2]) : fileURLToPath(new URL('.', import.meta.url));
  const read = name => JSON.parse(readFileSync(resolve(root, name)));
  console.log(JSON.stringify(verifyResults(read('results.json'), read, read('provenance.json')), null, 2));
}
