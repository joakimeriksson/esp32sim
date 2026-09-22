import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { test } from 'node:test';
import { checkProductionSections } from './check-wasm-sections.mjs';

function policy(overrides = {}) {
  const env = { ...process.env };
  for (const key of ['RUSTFLAGS', 'CARGO_PROFILE_RELEASE_DEBUG', 'CARGO_PROFILE_RELEASE_STRIP']) delete env[key];
  Object.assign(env, overrides);
  return execFileSync('sh', ['-c', '. ./tools/wasm-rustflags.sh; printf "%s\\n%s\\n%s" "$CARGO_PROFILE_RELEASE_DEBUG" "$CARGO_PROFILE_RELEASE_STRIP" "$RUSTFLAGS"'], { env, encoding: 'utf8' }).split('\n');
}
test('unset and empty Cargo debug/strip use production defaults', () => {
  for (const overrides of [{}, { CARGO_PROFILE_RELEASE_DEBUG: '', CARGO_PROFILE_RELEASE_STRIP: '' }]) {
    assert.deepEqual(policy(overrides).slice(0, 2), ['0', 'debuginfo']);
  }
});
test('debug 1/2 and strip overrides remain explicit choices', () => {
  for (const debug of ['1', '2']) {
    assert.deepEqual(policy({ CARGO_PROFILE_RELEASE_DEBUG: debug }).slice(0, 2), [debug, 'none']);
  }
  assert.equal(policy({ CARGO_PROFILE_RELEASE_STRIP: 'none' })[1], 'none');
  assert.equal(policy({ RUSTFLAGS: '' })[2], '');
});
const header = [0, 97, 115, 109, 1, 0, 0, 0];
function moduleWith(...names) {
  return Uint8Array.from([...header, ...names.flatMap(name => [0, name.length + 1, name.length, ...Buffer.from(name)])]);
}
test('production guard accepts name but rejects DWARF, stripped names and malformed WASM', () => {
  assert.equal(checkProductionSections(moduleWith('name')).dwarfAbsent, true);
  for (const debug of ['.debug_info', '.debug_line', '.zdebug_info']) {
    assert.throws(() => checkProductionSections(moduleWith('name', debug)), /DWARF/);
  }
  assert.throws(() => checkProductionSections(moduleWith()), /name section/);
  assert.throws(() => checkProductionSections(Uint8Array.of(0)), WebAssembly.CompileError);
});
