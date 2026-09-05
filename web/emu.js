// Page-side glue for the WebAssembly build. Active when the page is opened with `?wasm`; then
// window.EmuLink replaces the WebSocket transport in index.html, and a firmware panel is added.
// Firmware comes from the visitor's disk (file inputs) or from a manifest `wasm/fw/<name>.json`
// (`?wasm&fw=<name>`) listing files to fetch — for your own hosting; never publish blobs you
// may not redistribute (the Espressif mask ROM, third-party firmware).
(() => {
  const q = new URLSearchParams(location.search);
  // wasm mode: asked for explicitly, or the page is on a static host that cannot be the native emulator
  if (!q.has('wasm') && !/\.github\.io$/.test(location.hostname)) return;
  const worker = new Worker('wasm/worker.js', { type: 'module' });
  let onmessage = null, setStatus = () => {}, ready = false, started = false;
  const KINDS = { rom: 0, bootloader: 1, ptable: 2, app: 3, elf: 4, flash: 5, script: 6, picture: 7 };
  const pending = new Map();
  worker.onmessage = (ev) => {
    const m = ev.data;
    if (m.touchTrace) { window.recordTouchTrace?.(m.touchTrace); return; }
    if (m.frameTrace) window.recordTouchTrace?.(m.frameTrace);
    if (m.text !== undefined) { onmessage && onmessage(m.text); return; }
    if (m.bin !== undefined) {
      onmessage && onmessage(m.bin);
      if (m.frameTrace) window.recordTouchTrace?.({ stage: 'canvas-drawn', atMs: performance.timeOrigin + performance.now(), cycles: m.frameTrace.cycles });
      return;
    }
    if (m.log !== undefined) { console.log(m.log); onmessage && onmessage(JSON.stringify({ t: 'emu', msg: m.log })); return; }
    if (m.ready) { ready = true; setStatus('wasm loaded — choose firmware'); flush(); }
    if (m.created !== undefined) { const r = pending.get('created'); pending.delete('created'); r && r(m.created); }
    if (m.loaded !== undefined) { const r = pending.get('load' + m.loaded); pending.delete('load' + m.loaded); r && r(m.ok); }
    if (m.started !== undefined) { started = m.started; setStatus(started ? 'running in WebAssembly' : 'boot failed (see console)'); }
    if (m.stopped !== undefined) { started = false; setStatus('stopped: code ' + m.stopped); }
    if (m.netText) { onmessage && onmessage(JSON.stringify({ t: 'serial', src: 'node' + m.netText.node, data: m.netText.data })); }
    if (m.netStat) { onmessage && onmessage(JSON.stringify({ t: 'net', ...m.netStat })); }
    if (m.pace) { const el = document.getElementById('pace'); if (el) el.textContent = `${(m.pace.mips || 0).toFixed(1)} Minsn/s · ` + (m.pace.behind > 0.05 ? `⚠ ${m.pace.behind.toFixed(2)} s behind` : 'real time'); }
  };
  const queue = []; const flush = () => { while (ready && queue.length) worker.postMessage(...queue.shift()); };
  const post = (msg, transfer) => { queue.push([msg, transfer || []]); flush(); };
  const ask = (key, msg, transfer) => new Promise((res) => { pending.set(key, res); post(msg, transfer); });

  window.EmuLink = {
    connect(handler, status) {
      onmessage = handler; setStatus = status;
      fetch('wasm/esp32sim.wasm').then((r) => { if (!r.ok) throw new Error('wasm/esp32sim.wasm: ' + r.status); return r.arrayBuffer(); })
        .then((buf) => worker.postMessage({ op: 'init', wasm: buf, touchTrace: q.has('touchTrace') }, [buf]))
        .catch((e) => setStatus('cannot load wasm: ' + e.message));
      return { send: (d, timing) => { if (!started) return; if (typeof d === 'string') post({ op: 'text', data: d, touchTrace: timing }); else { const b = d.buffer ? d.buffer.slice(d.byteOffset, d.byteOffset + d.byteLength) : d; post({ op: 'bin', data: b }, [b]); } } };
    },
  };

  // Everything below touches the DOM; this script is loaded at the top of <body>, before the
  // header and main exist, so it waits for the document. EmuLink above is what index.html's
  // own script (at the end of <body>) needs, and that is defined synchronously.
  document.addEventListener('DOMContentLoaded', () => {
  // no firmware yet: no board to draw. The board announcement at boot switches the layout.
  document.body.classList.add('bare'); document.querySelector('h1').textContent = 'ESP32\u2011S3 emulator';
  // ---- firmware panel
  const panel = document.createElement('section');
  panel.id = 'fwpanel';
  panel.innerHTML = `<style>#fwpanel{margin:12px 18px 0;padding:12px 16px;background:#fff;border:1px solid #e5e7eb;border-radius:10px;font-size:13px}
    #fwpanel .row{display:flex;flex-wrap:wrap;gap:10px 18px;align-items:center;margin:4px 0}#fwpanel label{display:inline-flex;gap:6px;align-items:center}
    #fwpanel input[type=file]{max-width:180px}#fwpanel .go{padding:6px 14px;font-weight:600}#fwpanel .note{color:#6b7280}</style>
    <div class="row"><b>WebAssembly build</b><span class="note">everything runs in this tab — nothing is uploaded; firmware is read from your disk</span><span id="pace" class="note"></span></div>
    <div class="row" id="fw_demos" style="display:none"><span>Demos:</span></div>
    <div class="row">
      <label>board <select id="fw_board"><option>waveshare-lcd4b</option><option>waveshare-amoled18-v2</option><option>atech14</option><option>waveshare-cam</option><option>none</option><option>esp32c3</option><option>esp32c6</option><option>waveshare-c6-lcd147</option></select></label>
      <label>flash MB <input id="fw_flash" type="number" value="16" min="1" max="32" style="width:52px"></label>
      <label>PSRAM MB <input id="fw_psram" type="number" value="8" min="0" max="32" style="width:52px"></label>
      <label>WiFi <input id="fw_wifi" placeholder="ssid=…,psk=… (optional)" style="width:190px"></label>
      <label>stubs <input id="fw_stubs" placeholder="esp_wifi_start=0" style="width:150px"></label>
      <label><input id="fw_appdirect" type="checkbox"> boot app directly (no ROM)</label>
    </div>
    <div class="row">
      <label>ROM ELF <input type="file" id="fw_rom"></label>
      <label>bootloader.bin <input type="file" id="fw_bootloader"></label>
      <label>partition-table.bin <input type="file" id="fw_ptable"></label>
      <label>app.bin <input type="file" id="fw_app"></label>
      <label>app.elf (symbols) <input type="file" id="fw_elf"></label>
      <label>script <input type="file" id="fw_script"></label>
      <button class="go" id="fw_go">▶ Boot</button>
    </div>`;
  document.body.insertBefore(panel, document.querySelector('main'));
  const $ = (id) => document.getElementById(id);
  const readFile = (f) => new Promise((res, rej) => { const r = new FileReader(); r.onload = () => res(r.result); r.onerror = rej; r.readAsArrayBuffer(f); });

  async function boot(cfg, files) {
    if (started) { location.reload(); return; }
    setStatus('loading firmware…');
    const ok = await ask('created', { op: 'create', board: cfg.board, flash_mb: cfg.flash_mb, psram_mb: cfg.psram_mb, jit: q.get('jit') !== '0' });
    if (!ok) { setStatus('unknown board'); return; }
    for (const [kind, data, at] of files) {
      const key = at !== undefined ? 'loadat' + at : 'load' + KINDS[kind];
      const good = await ask(key, at !== undefined ? { op: 'load', at, data } : { op: 'load', kind: KINDS[kind], data }, [data]);
      if (!good) { setStatus('failed to load ' + (at !== undefined ? 'flash@0x' + at.toString(16) : kind) + ' (see console)'); return; }
    }
    for (const st of cfg.stubs || []) { const [name, v] = st.split('='); post({ op: 'stub', name: (cfg.symbols || {})[name] || name, value: v ? parseInt(v, 0) : 0 }); }
    if (cfg.wifi) post({ op: 'wifi', spec: cfg.wifi });
    post({ op: 'start', appDirect: !!cfg.appDirect });
  }
  // A network manifest boots several motes on one medium (esp32sim_net_*): the same files go to
  // every node, each with its own MAC, position and power-on offset.
  async function bootNet(cfg, files, nodeFiles) {
    setStatus('creating the network…');
    const r = await ask('created', { op: 'net-create', nodes: cfg.nodes, board: cfg.board, flash_mb: cfg.flash_mb, slice_ns: cfg.slice_ns || 0 });
    if (!r) { setStatus('could not create the network'); return; }
    for (let i = 0; i < cfg.nodes.length; i++) {
      // a node's own image of a kind replaces the shared one (a server next to a client)
      const own = (nodeFiles && nodeFiles[i]) || [];
      const mine = files.filter(([k]) => !own.some(([ok]) => ok === k)).concat(own);
      for (const [kind, data] of mine) {
        const copy = data.slice(0);   // each node keeps its own image: the buffer is transferred
        const good = await ask('load' + 'n' + i + ':' + KINDS[kind], { op: 'net-load', node: i, kind: KINDS[kind], data: copy }, [copy]);
        if (!good) { setStatus(`node ${i}: failed to load ${kind}`); return; }
      }
      for (const st of [].concat(cfg.nodes[i].stubs || cfg.stubs || [])) { const [name, v] = st.split('='); post({ op: 'net-stub', node: i, name: ((cfg.nodes[i].symbols || cfg.symbols) || {})[name] || name, value: v ? parseInt(v, 0) : 0 }); }
    }
    post({ op: 'net-start' });
  }

  $('fw_go').onclick = async () => {
    const files = [];
    for (const k of ['rom', 'bootloader', 'ptable', 'app', 'elf', 'script']) { const f = $('fw_' + k).files[0]; if (f) files.push([k, await readFile(f)]); }
    if (!files.some((x) => x[0] === 'app')) { setStatus('an app.bin is required'); return; }
    boot({ board: $('fw_board').value, flash_mb: +$('fw_flash').value, psram_mb: +$('fw_psram').value, wifi: $('fw_wifi').value.trim(), stubs: $('fw_stubs').value.split(/[ ,]+/).filter(Boolean), appDirect: $('fw_appdirect').checked }, files);
  };
  fetch('wasm/fw/demos.json', { cache: 'no-cache' }).then((r) => r.ok ? r.json() : []).then((demos) => {
    const row = $('fw_demos'); if (!demos.length) return; row.style.display = '';
    for (const d of demos) { const a = document.createElement('a'); const u = new URL(location.href); u.searchParams.set('wasm', ''); u.searchParams.set('fw', d.fw); a.href = u.toString(); a.textContent = d.title; a.title = d.note || ''; row.appendChild(a); }
  }).catch(() => {});
  // ---- manifest: ?wasm&fw=name → wasm/fw/name.json
  const fw = q.get('fw');
  if (fw) {
    (async () => {
      const man = await (await fetch(`wasm/fw/${fw}.json`, { cache: 'no-cache' })).json();   // manifests are tiny: always revalidate, so a removed demo disappears at once
      $('fw_board').value = man.board || 'none'; $('fw_flash').value = man.flash_mb || 8; $('fw_psram').value = man.psram_mb || 2; $('fw_wifi').value = man.wifi || ''; $('fw_stubs').value = (man.stubs || []).join(' ');
      const files = [];
      for (const [kind, url] of Object.entries(man.files || {})) for (const u of [].concat(url)) { const r = await fetch(`wasm/fw/${u}`, { cache: 'no-cache' }); if (!r.ok) { setStatus(`${u}: ${r.status}`); return; } files.push([kind, await r.arrayBuffer()]); }
      // flash_at: { "0x610000": "public/energydata.json" } — a data partition's contents
      for (const [off, u] of Object.entries(man.flash_at || {})) { const r = await fetch(`wasm/fw/${u}`, { cache: 'no-cache' }); if (!r.ok) { setStatus(`${u}: ${r.status}`); return; } files.push(['flash', await r.arrayBuffer(), parseInt(off, 16)]); }
      const cfg = { board: man.board, flash_mb: man.flash_mb || 8, psram_mb: man.psram_mb || 2, wifi: man.wifi || '', stubs: man.stubs || [], symbols: man.symbols || {}, appDirect: !!man.app_direct, nodes: man.nodes, slice_ns: man.slice_ns };
      const nodeFiles = [];
      for (const node of man.nodes || []) { const own = []; for (const [kind, url] of Object.entries(node.files || {})) for (const u of [].concat(url)) { const r = await fetch(`wasm/fw/${u}`, { cache: 'no-cache' }); if (!r.ok) { setStatus(`${u}: ${r.status}`); return; } own.push([kind, await r.arrayBuffer()]); } nodeFiles.push(own); }
      const wait = () => ready ? (man.nodes ? bootNet(cfg, files, nodeFiles) : boot(cfg, files)) : setTimeout(wait, 50);
      wait();
    })().catch((e) => setStatus('manifest: ' + e.message));
  }
  });
})();
