// Recompute reported reductions and enforce the retained browser correctness contract.
import {readFileSync} from 'node:fs';
const dir = new URL('./', import.meta.url);
const read = path => JSON.parse(readFileSync(new URL(path, dir)));
const median = values => {
  const sorted = values.toSorted((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.ceil((sorted.length - 1) / 2)]) / 2;
};
const same = (actual, expected, label) => {
  if (Math.abs(actual - expected) > 1e-9) throw new Error(label);
};
let arms = 0;
for (const entry of read('results.json')) {
  const job = read(`runs/${entry.name}.json`);
  if (job.runs.length !== job.pairs * 2) throw new Error(`${job.name}: pair count`);
  const expected = job.workload === 'tinydraw' ? 9819885134 : 10073833775;
  for (const run of job.runs) {
    if (!run.passed || run.status !== 'completed' || run.stopCode !== 0 || run.instrumented ||
        run.instructions !== expected || run.jit.failed !== 0 || run.jit.compiled <= 0 ||
        !run.verdictValidation.valid || !run.verdictValidation.passed)
      throw new Error(`${job.name}: correctness`);
  }
  for (const field of ['consoleSha256', 'frames'])
    if (new Set(job.runs.map(run => run[field])).size !== 1) throw new Error(`${job.name}: ${field}`);
  const b = median(job.runs.filter(run => run.arm === 'baseline').map(run => run.wallSeconds));
  const c = median(job.runs.filter(run => run.arm === 'candidate').map(run => run.wallSeconds));
  same(b, job.medianWallSeconds.baseline, `${job.name}: baseline median`);
  same(c, job.medianWallSeconds.candidate, `${job.name}: candidate median`);
  same(100 * (1 - c / b), job.wallReductionPercent, `${job.name}: reduction`);
  same(job.wallReductionPercent, entry.wallReductionPercent, `${job.name}: index`);
  for (let pair = 1; pair <= job.pairs; pair++) {
    const baseline = job.runs.find(run => run.pair === pair && run.arm === 'baseline');
    const candidate = job.runs.find(run => run.pair === pair && run.arm === 'candidate');
    same(100 * (1 - candidate.wallSeconds / baseline.wallSeconds), job.pairsWallReductionPercent[pair - 1], `${job.name}: pair ${pair}`);
  }
  arms += job.runs.length;
}
console.log(JSON.stringify({status: 'passed', jobs: read('results.json').length, timedArms: arms,
  checks: ['pair counts', 'median and paired reductions', 'pinned instructions', 'successful verdicts',
    'equal console hashes and frame counts', 'zero JIT failures', 'uninstrumented captures']}, null, 2));
