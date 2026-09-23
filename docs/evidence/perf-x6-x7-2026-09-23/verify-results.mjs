#!/usr/bin/env node
// Recompute every median, pair reduction and ordering check in results.json from the per-arm samples.
// Usage: node docs/evidence/perf-x6-x7-2026-09-23/verify-results.mjs
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const read = (f) => JSON.parse(readFileSync(join(here, f), 'utf8'));
const index = read('results.json');
const arms = new Map();
for (const f of ['arms/x6.json', 'arms/x7.json', 'arms/final.json']) for (const j of read(f).jobs) arms.set(j.name, j.arms);
const safari = read('arms/safari-m3.json').jobs;

const median = (xs) => { const s = [...xs].sort((a, b) => a - b); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const pct = (b, c) => 100 * (1 - c / b);
const fail = [];
const check = (name, rows, job, tol) => {
  const wall = (arm) => rows.filter((r) => r[1] === arm).map((r) => r[2]);
  const med = { baseline: median(wall('b')), candidate: median(wall('c')) };
  for (const k of ['baseline', 'candidate']) if (Math.abs(med[k] - job.medianWallSeconds[k]) > 1e-9) fail.push(`${name}: median ${k}`);
  if (Math.abs(pct(med.baseline, med.candidate) - job.wallReductionPercent) > tol) fail.push(`${name}: reduction`);
  for (let p = 1; p <= job.pairs; p++) {
    const pair = rows.filter((r) => r[0] === p);
    const order = pair.map((r) => r[1]).join('');
    if (order !== (p % 2 ? 'bc' : 'cb')) fail.push(`${name}: pair ${p} order ${order}`);
    const [b, c] = ['b', 'c'].map((a) => pair.find((r) => r[1] === a)[2]);
    if (Math.abs(pct(b, c) - job.pairsWallReductionPercent[p - 1]) > tol) fail.push(`${name}: pair ${p}`);
  }
  if (rows.length !== 2 * job.pairs) fail.push(`${name}: arm count`);
  if (!job.allArmsPass) fail.push(`${name}: an arm failed its checks`);
  if (job.contract === undefined || job.contract === 'unknown') fail.push(`${name}: unknown work contract`);
};
for (const job of index.jobs) {
  const rows = arms.get(job.name);
  if (!rows) { fail.push(`${job.name}: no arms`); continue; }
  check(job.name, rows, job, 5e-5); // index percentages are rounded to 4 decimals
  if (rows.some((r) => r[3] !== job.frames)) fail.push(`${job.name}: frames differ`);
  if (index.contracts[job.contract] !== job.instructions) fail.push(`${job.name}: instructions`);
}
for (const job of safari) check(`safari/${job.name}`, job.arms, job, 1e-9);
const n = index.jobs.reduce((s, j) => s + 2 * j.pairs, 0) + safari.reduce((s, j) => s + 2 * j.pairs, 0);
if (fail.length) { console.error(fail.join('\n')); process.exit(1); }
console.log(`ok: ${index.jobs.length} Chrome jobs and ${safari.length} Safari jobs, ${n} arms recomputed`);
