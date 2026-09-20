export async function createPanel(q) {
  if (document.readyState === 'loading') await new Promise(r => document.addEventListener('DOMContentLoaded', r, { once: true }));
  const panel = document.createElement('section');
  panel.style.cssText = 'margin:6px;padding:8px;background:#101820;color:#eef;border-radius:8px;font:12px/1.5 monospace;overflow-wrap:anywhere';
  const title = document.createElement('strong');
  title.textContent = `Phone diagnostics · guest JIT ${q.get('jit') === '0' ? 'OFF' : 'ON'}`;
  const output = document.createElement('pre'); output.style.whiteSpace = 'pre-wrap'; output.textContent = 'Waiting for emulator…';
  const note = document.createElement('div'); note.textContent = 'Keep foregrounded for 60 seconds. Times are worker-side; output is not screen painting. Gaps include intentional pacing and other worker tasks. Compilation time is cumulative and included in execution, not added to it.';
  const button = document.createElement('button'); button.textContent = 'Show report to copy';
  const report = document.createElement('textarea'); report.hidden = true; report.readOnly = true; report.style.cssText = 'width:100%;height:180px';
  const details = document.createElement('details');
  const summary = document.createElement('summary'); summary.textContent = 'How to read this'; details.append(summary, note);
  panel.append(title, output, button, details, report); document.body.prepend(panel);
  const samples = [], visibility = [{ elapsedMs: 0, state: document.visibilityState }], opened = performance.now();
  document.addEventListener('visibilitychange', () => visibility.push({ elapsedMs: performance.now() - opened, state: document.visibilityState }));
  button.onclick = () => { report.hidden = false; report.value = JSON.stringify({ version: 1, firmware: q.get('fw'), jit: q.get('jit') !== '0', userAgent: navigator.userAgent, visibility, samples }, null, 2); report.focus(); report.select(); };
  return { update(s) {
    samples.push(s); if (samples.length > 180) samples.shift();
    const pct = ms => (100 * ms / s.wallMs).toFixed(1) + '%';
    output.textContent = `${s.elapsedSec.toFixed(0)}s elapsed · ${document.visibilityState}\nRealtime: ${(100*s.speed).toFixed(1)}% now / ${(100*s.averageSpeed).toFixed(1)}% average\nExecute ${pct(s.runMs)} · output ${pct(s.drainMs)}\nBetween turns ${pct(s.gapMs)} · other ${pct(s.otherMs)}\nCatch-up gaps ${pct(s.catchupGapMs)} · longest gap ${s.maxGapMs.toFixed(1)}ms\nJIT compiled ${s.jit.compiled} · FAILED ${s.jit.failed}\nCompilation ${(s.jit.compileMs/1000).toFixed(2)}s total\nLive modules ${s.jit.compiled-s.jit.released} · peak ${s.jit.peakModules}\nGenerated code ${(s.jit.liveBytes/1048576).toFixed(2)} MiB (Wasm bytes)`;
  } };
}
