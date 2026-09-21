// ROM-free checks against an actual default-feature WASM build.
// node wasm/tests/timing-api.mjs target/wasm32-unknown-unknown/release/esp32sim_wasm.wasm
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {createJitHost} from '../../web/wasm/jit.mjs';

let w;
const host = createJitHost(() => w);
w = (await WebAssembly.instantiate(await fs.readFile(process.argv[2]), {
  env: {...host.imports, host_log() {}, host_profile_now: () => performance.now()},
})).instance.exports;
const board = new TextEncoder().encode('atech14');
const p = w.esp32sim_alloc(board.length);
new Uint8Array(w.memory.buffer).set(board, p);
const e = w.esp32sim_new(p, board.length, 4, 2);
w.esp32sim_free(p, board.length);
assert.notEqual(e, 0);
try {
  assert.equal(w.esp32sim_set_approximate_jit_cache(e, 96, 40, 0), 1);
  assert.equal(w.esp32sim_set_approximate_pie_timing(e, 1), 1);
  assert.equal(w.esp32sim_set_control_prices(e, 1), 1);
  assert.equal(w.esp32sim_set_icache_fill(e, 32), 1);
  assert.equal(w.esp32sim_set_approximate_jit_timing(e, 1, 512), 0);
  assert.equal(w.esp32sim_set_approximate_jit_frontiers(e, 3), 1);
  assert.equal(w.esp32sim_set_approximate_jit_cache(e, 96, 40, 99), 1);
  assert.equal(w.esp32sim_set_approximate_cache_contention(e, 1), 1);
  assert.equal(w.esp32sim_set_approximate_cache_fill_service(e, 160), 1);
  for (const mode of [0, 1, 2, 3]) {
    assert.equal(w.esp32sim_set_approximate_jit_cache(e, 96, 40, mode), 0);
    assert.equal(w.esp32sim_set_approximate_cache_contention(e, 1), 0);
    assert.equal(w.esp32sim_set_approximate_cache_fill_service(e, 95), 1);
    assert.equal(w.esp32sim_set_approximate_cache_fill_service(e, 160), 0);
  }
  assert.equal(w.esp32sim_set_approximate_pie_timing(e, 2), 0);
  assert.equal(w.esp32sim_set_control_prices(e, 1), 0);
  assert.equal(w.esp32sim_set_icache_fill(e, 32), 0);
  assert.equal(w.esp32sim_set_approximate_jit_timing(e, 0, 512), 1);
  assert.equal(w.esp32sim_set_approximate_jit_timing(e, 1, 0), 1);
  assert.equal(w.esp32sim_set_approximate_jit_frontiers(e, 0), 0);
  w.esp32sim_set_jit(e, 0);
  assert.equal(w.esp32sim_set_quantum(e, 128), 1);
  assert.equal(w.esp32sim_approximate_cache_wait(e, 1), 0);
  assert(Number.isNaN(w.esp32sim_approximate_cache_wait(e, 99)));
  assert.equal(w.esp32sim_boot(e, 0), 0, 'preboot setup must remain bootable');
  console.log('WASM timing ABI: prerequisite, range, inline mode and lifecycle checks passed');
} finally {
  w.esp32sim_delete(e);
}
