// Recompute reported reductions and enforce the retained browser correctness contract.
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const dir = new URL('./', import.meta.url);
const read = path => JSON.parse(readFileSync(new URL(path, dir)));
const requireThat = (ok, label) => { if (!ok) throw new Error(label); };
const finite = value => typeof value === 'number' && Number.isFinite(value);
const median = values => {
  const sorted = values.toSorted((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.ceil((sorted.length - 1) / 2)]) / 2;
};
const same = (actual, expected, label) => {
  requireThat(finite(actual) && finite(expected) && Math.abs(actual - expected) <= 1e-9, label);
};
export function verifyResults(index, readJob, gates) {
  requireThat(Array.isArray(index) && index.length > 0, 'empty index');
  const gated = new Set(gates.filter(g => g.EXACT === true && g.jitFailures === 0 && g.panics === 0 && g.secs === 30).map(g => g.wasmSha256));
  const names = new Set();
  let arms = 0;
  for (const entry of index) {
    requireThat(typeof entry.name === 'string' && /^[a-z0-9-]+$/.test(entry.name) && !names.has(entry.name), 'invalid or duplicate job name');
    names.add(entry.name);
    const job = readJob(entry.name);
    const label = field => `${entry.name}: ${field}`;
    requireThat(['pocket-tank', 'tinydraw'].includes(job.workload), label('unknown workload'));
    for (const field of ['name', 'workload', 'sourceRevision', 'baseRevision', 'baselineWasmSha256', 'candidateWasmSha256', 'pairs'])
      requireThat(job[field] === entry[field], label(`index ${field}`));
    for (const field of ['sourceRevision', 'baseRevision'])
      requireThat(typeof job[field] === 'string' && /^[a-f0-9]{40}$/.test(job[field]), label(field));
    for (const field of ['baselineWasmSha256', 'candidateWasmSha256'])
      requireThat(typeof job[field] === 'string' && /^[a-f0-9]{64}$/.test(job[field]) && gated.has(job[field]), label(`${field} exact gate`));
    if (job.name.startsWith('control-aa-'))
      requireThat(job.baselineWasmSha256 === job.candidateWasmSha256, label('A/A artifact identity'));
    requireThat(Number.isInteger(job.pairs) && job.pairs > 0 && Array.isArray(job.runs) && job.runs.length === job.pairs * 2, label('pair count'));
    requireThat(Array.isArray(job.pairsWallReductionPercent) && job.pairsWallReductionPercent.length === job.pairs && Array.isArray(entry.pairsWallReductionPercent) && entry.pairsWallReductionPercent.length === job.pairs, label('pair reductions'));
    const expected = job.workload === 'tinydraw' ? 9819885134 : 10073833775;
    const frames = job.workload === 'tinydraw' ? 428 : 737;
    for (const [i, run] of job.runs.entries()) {
      const pair = Math.floor(i / 2) + 1;
      const arm = (pair % 2 === 1) === (i % 2 === 0) ? 'baseline' : 'candidate';
      requireThat(run.pair === pair && run.arm === arm, label('alternating pair order'));
      requireThat(finite(run.wallSeconds) && run.wallSeconds > 0, label('positive finite wallSeconds'));
      requireThat(run.passed === true && run.status === 'completed' && run.stopCode === 0 && run.instrumented === false &&
        run.instructions === expected && run.frames === frames && run.jit?.failed === 0 && Number.isInteger(run.jit?.compiled) && run.jit.compiled > 0 &&
        run.verdictValidation?.valid === true && run.verdictValidation?.passed === true, label('correctness'));
      requireThat(typeof run.consoleSha256 === 'string' && /^[a-f0-9]{64}$/.test(run.consoleSha256), label('console hash'));
    }
    requireThat(new Set(job.runs.map(run => run.consoleSha256)).size === 1, label('console equality'));
    const b = median(job.runs.filter(run => run.arm === 'baseline').map(run => run.wallSeconds));
    const c = median(job.runs.filter(run => run.arm === 'candidate').map(run => run.wallSeconds));
    for (const [arm, actual] of [['baseline', b], ['candidate', c]]) {
      same(actual, job.medianWallSeconds?.[arm], label(`${arm} median`));
      same(actual, entry.medianWallSeconds?.[arm], label(`index ${arm} median`));
    }
    same(100 * (1 - c / b), job.wallReductionPercent, label('reduction'));
    same(job.wallReductionPercent, entry.wallReductionPercent, label('index reduction'));
    for (let pair = 1; pair <= job.pairs; pair++) {
      const runs = job.runs.slice((pair - 1) * 2, pair * 2);
      const baseline = runs.find(run => run.arm === 'baseline');
      const candidate = runs.find(run => run.arm === 'candidate');
      const reduction = 100 * (1 - candidate.wallSeconds / baseline.wallSeconds);
      same(reduction, job.pairsWallReductionPercent[pair - 1], label(`pair ${pair}`));
      same(reduction, entry.pairsWallReductionPercent[pair - 1], label(`index pair ${pair}`));
    }
    arms += job.runs.length;
  }
  return {status: 'passed', jobs: index.length, timedArms: arms,
    checks: ['finite numeric timings and summaries', 'index metadata agreement', 'alternating pair order',
      'exact-gated artifact hashes', 'A/A artifact identity', 'median and paired reductions',
      'pinned instructions and frames', 'successful verdicts', 'equal console hashes', 'zero JIT failures', 'uninstrumented captures']};
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  console.log(JSON.stringify(verifyResults(read('results.json'), name => read(`runs/${name}.json`), read('exact-gates.json')), null, 2));
