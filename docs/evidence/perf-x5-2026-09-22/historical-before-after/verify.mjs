// Verify retained historical receipts; no timing or equal-work assumption.
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
const requireThat = (ok, label) => { if (!ok) throw Error(label); };
const finite = v => typeof v === 'number' && Number.isFinite(v);
const positive = v => finite(v) && v > 0;
const hash = v => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
const same = (a, b, label) => requireThat(finite(a) && finite(b) && Math.abs(a - b) <= 1e-9, label);
const equal = (a, b, label) => requireThat(isDeepStrictEqual(a, b), label);
const median = values => { const a = values.toSorted((a, b) => a - b), m = a.length >> 1; return a.length % 2 ? a[m] : (a[m - 1] + a[m]) / 2; };
const artifacts = {
  earliest: {wasm: '22c3df896b739ee12dce71408d3f3f77a63125ad943007d44e1e9105035020a4', instructions: 9820325756, frames: 101, mode: 'historical-interpreter', console: '2496efdf2a12f86ea13f1b29de0991de5c71a2955f58c8ff4add72434221209f'},
  firstJit: {wasm: '11a781893f89f598fbbf36933a703a82e3771f65a26e743ef0da56838996e042', instructions: 9820325756, frames: 101, mode: 'current-jit', console: '2496efdf2a12f86ea13f1b29de0991de5c71a2955f58c8ff4add72434221209f'},
  current: {wasm: 'eb754176f3100563da93b5269a54af1b93ff611abcf09fbae5026fd110b8409a', instructions: 9819885134, frames: 428, mode: 'current-jit', console: 'f4e7e4f3be2f988b8b52d2797d373208b49e86e70b9c2b6618910a90c7e36291'},
};
const jobs = {'control-current.json': ['current', 2, 0], 'earliest-vs-current.json': ['earliest', 4, 1], 'first-jit-vs-current.json': ['firstJit', 4, 1]};
const gates = 'native_kernels minimap_navigation color_dialog cooperative_compose stress stress_100 stress_400 overlap_ready overlap_cold general_cold_ready general_cold owner_document workload paced_cold hard_100 hard_400 pan_100 pan_400 ring_local pan_seq pan_boundary live_overlay draw_fill cache full_world_cache cache_tour mixed_draw idle_repair ink_trace hairline_capacity long_gesture history_latency settle_timing export_encode export_reserve return'.split(' ');
const frozenInputs = {rom: 'c0ce0f338d1de1bdc6efbef1591779a2a42c1ab7d759d3c6ae8ae63a7dd34cfd', bootloader: '5f5111980c1c8aeae76e08706eee6df49424bac759e703c78662946de4a12288', ptable: 'f53268312c8caffe6c7f4e6c66d4092aeca3435c142db3116466f84a6a608d2d', app: 'f4702138f5f4012938274a96aa18e9d1ffbb35f4d3139bc87a4e03038ac904c1', elf: 'd8e9be336ff6b12cd84580c69fc182ccfee8da7a3cc3813571ba6c25013a850e'};
function validVerdict(verdict) {
  requireThat(typeof verdict === 'string', 'verdict type');
  const [marker, ...tokens] = verdict.split(' ');
  requireThat(marker === 'TINYDRAW_GATE1_AUTOMATED_DONE' && tokens.length === 37 && tokens.every(t => /^[a-z][a-z_0-9]*=(?:1|yellow)$/.test(t)), 'verdict fields');
  const fields = Object.fromEntries(tokens.map(token => token.split('=')));
  requireThat(Object.keys(fields).length === 37 && gates.every(g => fields[g] === '1') && fields.ssaa_receipt === 'yellow', 'verdict gates');
}
export function verifyResults(index, read) {
  requireThat(index.jobs === 3 && index.timedArms === 24 && Array.isArray(index.jobFiles), 'campaign totals');
  equal([...index.jobFiles].sort(), Object.keys(jobs).sort(), 'complete unique jobs');
  let arms = 0, inputReference, nonWasmReference, browserReference, verdictReference;
  for (const file of index.jobFiles) {
    const job = read(file), [baseline, pairs, warmups] = jobs[file];
    const label = field => `${file}: ${field}`;
    requireThat(job.pairs === pairs && job.warmupPairs === warmups && Array.isArray(job.runs) && job.runs.length === 2 * (pairs + warmups), label('pair counts'));
    for (const [arm, artifactName] of [['baseline', baseline], ['candidate', 'current']]) {
      const c = job.contracts[arm], a = artifacts[artifactName];
      requireThat(c.qualified === true && c.workload === 'tinydraw' && c.mode === a.mode && c.wasmSha256 === a.wasm && c.instructions === a.instructions && c.frames === a.frames && c.consoleSha256 === a.console, label(`${arm} frozen contract`));
      validVerdict(c.verdict);
      verdictReference ??= c.verdict; equal(c.verdict, verdictReference, label('shared verdict'));
      equal(Object.keys(c.inputSha256).sort(), ['app', 'bootloader', 'elf', 'ptable', 'rom'], label('input names'));
      requireThat(Object.values(c.inputSha256).every(hash), label('input hashes'));
      equal(c.inputSha256, frozenInputs, label('frozen firmware inputs'));
      inputReference ??= c.inputSha256; equal(c.inputSha256, inputReference, label('shared firmware inputs'));
      const q = c.qualification, f = q?.finalFrame;
      requireThat(hash(q?.pilotResultSha256) && f?.present === true && f.nonuniform === true && f.width === 368 && f.height === 448 && f.uniqueColors === 16 && f.lcdFrames === c.frames && f.rgbaSha256 === 'dbf0d387a05232f38dc694f81ea871e448e2dc112c90d0fd94d9ecaef1fa41d0' && f.rgb565Sha256 === 'cbd7fda3c28fbe7127fac2e60eada6ffa5c08fef4be3830677d1417e9aecfad4', label('qualified final image'));
    }
    if (baseline === 'current') equal(job.contracts.baseline, job.contracts.candidate, label('A/A identity'));
    for (const [i, row] of job.runs.entries()) {
      const stage = i < warmups * 2 ? 'warmup' : 'primary';
      const j = stage === 'warmup' ? i : i - warmups * 2, pair = Math.floor(j / 2) + 1;
      const arm = (pair % 2 === 1) === (j % 2 === 0) ? 'baseline' : 'candidate';
      requireThat(row.stage === stage && row.pair === pair && row.arm === arm, label('ordered stage/pairs'));
      const r = row.result, c = job.contracts[arm];
      requireThat(row.captureMode === 'timing' && r && r.workload === 'tinydraw' && r.status === 'completed' && r.stopCode === 0 && r.passed === true && r.instrumented === false, label('successful uninstrumented arm'));
      requireThat(positive(r.wallSeconds) && positive(r.guestSeconds), label('finite positive timing'));
      for (const field of ['wallSeconds', 'guestSeconds', 'instructions', 'frames', 'passed', 'capabilities', 'provenance']) equal(row[field], r[field], label(`row/result ${field}`));
      requireThat(r.instructions === c.instructions && r.frames === c.frames && row.consoleSha256 === c.consoleSha256 && r.verdict === c.verdict, label('per-artifact work/output'));
      const v = r.verdictValidation;
      requireThat(v?.valid === true && v.passed === true && v.error === null && v.schema === 'tinydraw-gate1-v1', label('strict verdict status'));
      const jit = r.jit, cap = r.capabilities;
      requireThat(jit?.failed === 0 && Number.isInteger(jit.compiled) && cap?.mode === c.mode && cap.missingCountersAreZero === false, label('JIT status'));
      const enabled = c.mode === 'current-jit';
      requireThat(cap.hasJitSetter === enabled && cap.hasJitCounter === enabled && cap.executionMode === (enabled ? 'jit' : 'interpreter'), label('capability mode'));
      requireThat(enabled ? jit.compiled > 0 && positive(jit.instructions) : jit.compiled === 0 && jit.instructions === null, label('JIT execution'));
      for (const [field, key] of [['jitFailed', 'failed'], ['jitCompiled', 'compiled'], ['jitBytes', 'bytes']]) equal(row[field], jit[key], label(`row/result ${field}`));
      same(row.realtimeRatio, r.guestSeconds / r.wallSeconds, label('realtime ratio'));
      const hashes = r.provenance?.sha256;
      requireThat(hashes && hashes['asset/wasm'] === c.wasmSha256 && Object.values(hashes).every(hash), label('captured hashes'));
      for (const [name, expected] of Object.entries(c.inputSha256)) requireThat(hashes[`asset/${name}`] === expected, label(`captured ${name}`));
      const nonWasm = Object.fromEntries(Object.entries(hashes).filter(([k]) => k !== 'asset/wasm'));
      nonWasmReference ??= nonWasm; equal(nonWasm, nonWasmReference, label('common non-WASM inputs'));
      requireThat(/^Chrome\/[0-9]+(?:\.[0-9]+)*$/.test(row.browser) && typeof row.v8 === 'string' && row.v8.length > 0, label('browser identity'));
      browserReference ??= [row.browser, row.v8]; equal([row.browser, row.v8], browserReference, label('stable browser'));
    }
    const primary = job.runs.filter(r => r.stage === 'primary');
    const med = Object.fromEntries(['baseline', 'candidate'].map(arm => [arm, median(primary.filter(r => r.arm === arm).map(r => r.wallSeconds))]));
    for (const arm of ['baseline', 'candidate']) same(job.medianWallSeconds?.[arm], med[arm], label(`${arm} primary median`));
    same(job.wallReductionPercent, 100 * (1 - med.candidate / med.baseline), label('primary reduction'));
    same(job.throughputRatioForFirmwareTask, med.baseline / med.candidate, label('task ratio'));
    requireThat(Array.isArray(job.pairsWallReductionPercent) && job.pairsWallReductionPercent.length === pairs, label('paired reductions'));
    for (let pair = 1; pair <= pairs; pair++) same(job.pairsWallReductionPercent[pair - 1], 100 * (1 - primary.find(r => r.pair === pair && r.arm === 'candidate').wallSeconds / primary.find(r => r.pair === pair && r.arm === 'baseline').wallSeconds), label(`pair ${pair} reduction`));
    arms += job.runs.length;
  }
  requireThat(arms === index.timedArms, 'verified arm total');
  const pilots = read('qualification.json').pilots;
  requireThat(Array.isArray(pilots), 'qualification pilots');
  equal(pilots.map(p => p.name).sort(), ['31037b26-tinydraw', 'e7a16784-tinydraw', 'a2db66cf-tinydraw', '80c43cae-tinydraw', 'current-tinydraw'].sort(), 'all positive and negative pilots retained');
  for (const p of pilots) {
    requireThat(p.timingComparable === false, 'pilots are not browser timing');
    for (const [key, expected] of Object.entries(frozenInputs)) requireThat(p.inputs?.[key]?.sha256 === expected, 'pilot frozen inputs');
    const key = {'a2db66cf-tinydraw': 'earliest', '80c43cae-tinydraw': 'firstJit', 'current-tinydraw': 'current'}[p.name];
    if (key) {
      const a = artifacts[key], r = p.result;
      requireThat(p.accepted === true && p.inputs.wasm.sha256 === a.wasm && r.passed === true && r.status === 'completed' && r.stopCode === 0 && r.jit?.failed === 0 && r.instructions === a.instructions && r.frames === a.frames && p.consoleSha256 === a.console, 'qualified pilot contract');
      validVerdict(r.verdict); equal(r.verdict, verdictReference, 'pilot verdict');
      requireThat(p.finalFrame?.present === true && p.finalFrame.nonuniform === true && p.finalFrame.width === 368 && p.finalFrame.height === 448 && p.finalFrame.rgbaSha256 === 'dbf0d387a05232f38dc694f81ea871e448e2dc112c90d0fd94d9ecaef1fa41d0', 'pilot final image');
    } else {
      requireThat(p.accepted === false, 'negative pilot accepted');
      if (p.name === '31037b26-tinydraw') requireThat(p.result.status === 'timeout' && p.result.passed === false, 'first-board failure retained');
      else requireThat(p.finalFrame?.present === true && hash(p.finalFrame.rgbaSha256) && p.finalFrame.rgbaSha256 !== 'dbf0d387a05232f38dc694f81ea871e448e2dc112c90d0fd94d9ecaef1fa41d0', 'parent image mismatch retained');
    }
  }
  const provenance = read('provenance.json').artifacts;
  for (const [name, key] of [['a2db66cf', 'earliest'], ['80c43cae', 'firstJit'], ['current', 'current']]) {
    const p = provenance[name];
    requireThat(p && /^[a-f0-9]{40}$/.test(p.sourceRevision) && p.wasmSha256 === artifacts[key].wasm, 'artifact source provenance');
  }
  return {status: 'passed', jobs: 3, timedArms: arms};
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = process.argv[2] ? resolve(process.argv[2]) : fileURLToPath(new URL('.', import.meta.url));
  const read = name => JSON.parse(readFileSync(resolve(root, name)));
  console.log(JSON.stringify(verifyResults(read('results.json'), read), null, 2));
}
