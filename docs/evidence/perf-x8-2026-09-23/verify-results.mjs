#!/usr/bin/env node
// Recompute every median, reduction, pair order and work check in results.json, native.json and q256.json (validation and headline arms)
// from the per-arm samples. Usage: node docs/evidence/perf-x8-2026-09-23/verify-results.mjs
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const read = (f) => JSON.parse(readFileSync(join(here, f), 'utf8'));
const index = read('results.json');
const arms = new Map();
for (const f of index.armFiles) for (const j of read(f).jobs) arms.set(j.name, j.arms);

const median = (xs) => { const s = [...xs].sort((a, b) => a - b); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const pct = (b, c) => 100 * (1 - c / b);
const fail = [];
const pairs = (name, rows, job, tol) => {
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
};

// Browser jobs: rows are [pair, b|c, wall, frames, instructions, jitBytes, compileMs, consoleSha256].
for (const job of index.jobs) {
  const rows = arms.get(job.name);
  if (!rows) { fail.push(`${job.name}: no arms`); continue; }
  pairs(job.name, rows, job, 5e-5); // index percentages are rounded to 4 decimals
  if (!job.allArmsPass) fail.push(`${job.name}: an arm failed its checks`);
  if (rows.some((r) => r[3] !== job.frames || r[4] !== job.instructions || r[7] !== job.consoleSha256)) fail.push(`${job.name}: work differs between arms`);
  if (index.contracts[job.contract] !== job.instructions) fail.push(`${job.name}: instructions not a pinned contract`);
  if (!job.contract.endsWith(`-q${job.quantum}`)) fail.push(`${job.name}: contract does not match the job quantum`);
}

// Native screens: rows are [pair, b|c, wall, [core0, core1], jitLine, consoleSha256, load].
const native = read('native.json').screens;
for (const s of native) {
  pairs(`native/${s.name}`, s.rows, s, 5e-5);
  const same = (arm, k) => new Set(s.rows.filter((r) => r[1] === arm).map((r) => JSON.stringify(r[k]))).size === 1;
  for (const arm of ['b', 'c']) for (const k of [3, 5]) if (!same(arm, k)) fail.push(`native/${s.name}: ${arm} arms differ in work`);
  const eq = JSON.stringify(s.rows.find((r) => r[1] === 'b')[3]) === JSON.stringify(s.rows.find((r) => r[1] === 'c')[3]);
  if (eq !== s.equalWork) fail.push(`native/${s.name}: equalWork flag`);
}

// q64/q256 validation arms: medians and change per workload.
const q = read('q256.json');
for (const [w, v] of Object.entries(q.validation)) {
  const wall = (qq) => q.arms.filter((a) => a.workload === w && a.quantum === qq).map((a) => a.wallSeconds);
  const m64 = median(wall(64)), m256 = median(wall(256));
  if (wall(64).length !== 3 || wall(256).length !== 3) fail.push(`q256/${w}: arm count`);
  if (Math.abs(m64 - v.medianWallSeconds.q64) > 1e-9 || Math.abs(m256 - v.medianWallSeconds.q256) > 1e-9) fail.push(`q256/${w}: medians`);
  if (Math.abs(100 * (m256 / m64 - 1) - v.wallChangePercent) > 5e-5) fail.push(`q256/${w}: change`);
}
// Headline single arms: base8 at q64 against the shipped stack at q256.
for (const [w, v] of Object.entries(q.headline.summary)) {
  const wall = (k) => q.headline.arms.filter((a) => a.workload === w && a.stack === k).map((a) => a.wallSeconds);
  const mb = median(wall('base')), ms = median(wall('ship'));
  if (wall('base').length !== 3 || wall('ship').length !== 3) fail.push(`headline/${w}: arm count`);
  if (Math.abs(mb - v.medianWallSeconds['base8-q64']) > 1e-9 || Math.abs(ms - v.medianWallSeconds['ship-q256']) > 1e-9) fail.push(`headline/${w}: medians`);
  if (Math.abs(100 * (ms / mb - 1) - v.wallChangePercent) > 5e-5) fail.push(`headline/${w}: change`);
}
for (const a of [...q.arms, ...q.headline.arms]) {
  if (!a.passed || a.jitFailed !== 0) fail.push(`q256/${a.arm}: failed`);
  if (!a.contract.endsWith(`-q${a.quantum}`)) fail.push(`q256/${a.arm}: contract`);
}

const n = index.jobs.reduce((s, j) => s + 2 * j.pairs, 0);
if (fail.length) { console.error(fail.join('\n')); process.exit(1); }
console.log(`ok: ${index.jobs.length} Chrome jobs (${n} arms), ${native.length} native screens, ${q.arms.length} q64/q256 validation arms and ${q.headline.arms.length} headline arms recomputed`);
