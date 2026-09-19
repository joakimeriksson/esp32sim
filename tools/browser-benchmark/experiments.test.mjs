import test from 'node:test';
import assert from 'node:assert/strict';
import { HW, experimentsFromParams, validateExperiments, applyExperiments } from '../../web/wasm/experiments.mjs';

const parse = query => experimentsFromParams(new URLSearchParams(query));
test('page and response timing parameters preserve valid models and bisects', () => {
  assert.deepEqual(parse(''), []);
  assert.deepEqual(parse('timing=hw'), HW);
  assert.deepEqual(parse('timing=hw-2-8'), HW.filter((_, n) => n !== 2 && n !== 8));
  assert.deepEqual(parse('timing=hw-0'), HW.slice(1)); // wasm enforces prerequisites, not the bisect parser
  for (const n of [64, 128, 4096]) assert.deepEqual(parse(`quantum=${n}`), [['esp32sim_set_quantum', n]]);
});
test('malformed bisects and ineffective quantum combinations reject', () => {
  for (const timing of ['', 'hw-', 'hw--1', 'hw-1-', 'hw-9', 'hw-01', 'hw-nope', 'hwat', 'hw-1.0']) {
    assert.throws(() => parse(`timing=${timing}`), /invalid timing/);
  }
  for (const quantum of ['', 'NaN', 'Infinity', '-64', '0', '65', '4097', '64.5', '0x40', '4294967360']) {
    assert.throws(() => parse(`quantum=${quantum}`), /quantum/);
  }
  assert.throws(() => parse('timing=hw&quantum=64'), /combined/);
  assert.throws(() => parse('timing=hw-0&quantum=64'), /combined/);
});
test('diagnostic setters remain supported but invalid argument shapes never dispatch', () => {
  const probes = [['esp32sim_set_approximate_pie_timing', 2], ['esp32sim_set_approximate_flash_timing', 128, 475], ['esp32sim_set_approximate_jit_frontiers', 2], ['esp32sim_set_approximate_jit_cache', 96, 160, 3]];
  validateExperiments(probes);
  for (const entry of [...HW, ...probes]) {
    assert.throws(() => validateExperiments([[...entry, 0]]));
    assert.throws(() => validateExperiments([entry.slice(0, -1)]));
    for (const value of [-1, NaN, Infinity, 0x100000000, 1.5, '1', null, true]) {
      assert.throws(() => validateExperiments([[entry[0], value, ...entry.slice(2)]]));
    }
  }
  let called = false;
  assert.throws(() => applyExperiments({esp32sim_set_quantum() { called = true; return 0; }}, 1, [['esp32sim_set_quantum', 64], ['esp32sim_delete']]));
  assert.equal(called, false, 'whole list validated before first setter');
});
