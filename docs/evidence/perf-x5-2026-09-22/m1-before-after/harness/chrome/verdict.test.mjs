import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {validateVerdict, completedVerdict, optionalCounter} from './verdict.mjs';

const schema = JSON.parse(fs.readFileSync(new URL('./verdict-schema.json', import.meta.url)));
const verdict = [schema.marker, ...schema.gates.map(key => `${key}=1`), 'ssaa_receipt=yellow'].join(' ');

test('versioned verdict requires all 36 gates and the separate receipt', () => {
  assert.equal(schema.gates.length, 36);
  assert.deepEqual(validateVerdict(verdict, schema), {
    schema: 'tinydraw-gate1-v1', valid: true, passed: true, error: null,
  });
  const failed = validateVerdict(verdict.replace('stress=1', 'stress=0'), schema);
  assert.equal(failed.valid, true);
  assert.equal(failed.passed, false);
});

test('reject empty, missing, duplicate, unknown and malformed fields', () => {
  for (const line of [null, '', schema.marker,
    verdict.replace('stress=1 ', ''), verdict.replace(' ssaa_receipt=yellow', ''),
    `${verdict} stress=1`, `${verdict} surprise=1`,
    verdict.replace('stress=1', 'stress=true'), verdict.replace('stress=1', 'stress=01'),
    verdict.replace('stress=1', 'stress=1=1'), verdict.replace('yellow', 'unknown'),
    `${verdict}\n`, verdict.replace(schema.marker, 'OTHER')]) {
    assert.equal(validateVerdict(line, schema).passed, false, String(line));
  }
});

test('serial extraction requires exactly one complete verdict line', () => {
  assert.equal(completedVerdict(`boot\n${verdict}\r\n`, schema), verdict);
  for (const serial of [verdict, `${verdict}\n${verdict}\n`, `${verdict}\n${verdict}`]) {
    assert.equal(completedVerdict(serial, schema), null);
  }
});

test('unavailable counters differ from measured zero', () => {
  assert.equal(optionalCounter({}, 'count', 7), null);
  assert.equal(optionalCounter({count: () => 0}, 'count', 7), 0);
  assert.equal(optionalCounter({count: emu => emu}, 'count', 7), 7);
});
