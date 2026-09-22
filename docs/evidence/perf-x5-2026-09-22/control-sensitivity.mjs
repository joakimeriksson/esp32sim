// Exploratory sensitivity only: retain every original observation in runs/*.json.
// Run: node docs/evidence/perf-x5-2026-09-22/control-sensitivity.mjs
import fs from 'node:fs';
const read = name => JSON.parse(fs.readFileSync(new URL(`runs/${name}.json`, import.meta.url)));
const median = values => {
  if (!values.length || values.some(v => typeof v !== 'number' || !Number.isFinite(v) || v <= 0)) throw Error('invalid wall samples');
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};
const reduction = runs => 100 * (1 - median(runs.filter(r => r.arm === 'candidate').map(r => r.wallSeconds)) / median(runs.filter(r => r.arm === 'baseline').map(r => r.wallSeconds)));
const names = ['control-aa-1', 'control-aa-td', 'control-aa-final', 'control-aa-final-td', 'combined-size', 'combined-size-td'];
const jobs = names.map(name => {
  const job = read(name);
  const allPairsPercent = reduction(job.runs);
  const omitFirstPairPercent = reduction(job.runs.filter(r => r.pair !== 1));
  return { name, source: `runs/${name}.json`, pairs: job.pairs, startedAt: job.startedAt, allPairsPercent, omitFirstPairPercent, differencePercentagePoints: omitFirstPairPercent - allPairsPercent };
});
console.log(JSON.stringify({ method: '100 * (1 - median(candidate walls) / median(baseline walls)), recalculated after excluding both arms of pair 1.', limitation: 'Post-hoc sensitivity, not a replacement estimate, formal confidence interval, established noise floor or proof of a cold-start cause. All raw observations remain retained.', jobs }, null, 2));
