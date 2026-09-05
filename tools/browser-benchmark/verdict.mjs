// Receipt colors are evidence, not boolean correctness gates.
export function validateVerdict(line, schema) {
  const fail = error => ({schema: schema.version, valid: false, passed: false, error});
  if (typeof line !== 'string' || /[\r\n]/.test(line)) return fail('missing or incomplete verdict');
  const [marker, ...tokens] = line.trim().split(/\s+/);
  if (marker !== schema.marker) return fail('unexpected verdict marker');
  const fields = new Map();
  const allowed = new Set([...schema.gates, ...Object.keys(schema.receipts)]);
  for (const token of tokens) {
    const match = /^([a-z][a-z0-9_]*)=([a-z0-9]+)$/.exec(token);
    if (!match) return fail(`malformed field: ${token}`);
    const [, key, value] = match;
    if (!allowed.has(key)) return fail(`unknown field: ${key}`);
    if (fields.has(key)) return fail(`duplicate field: ${key}`);
    fields.set(key, value);
  }
  for (const key of allowed) {
    if (!fields.has(key)) return fail(`missing field: ${key}`);
    const values = schema.receipts[key] ?? ['0', '1'];
    if (!values.includes(fields.get(key))) return fail(`invalid value: ${key}`);
  }
  return {schema: schema.version, valid: true,
    passed: schema.gates.every(key => fields.get(key) === '1'), error: null};
}

export function completedVerdict(serial, schema) {
  const lines = serial.split(/\r?\n/);
  const pending = lines.pop();
  const verdicts = lines.filter(line => line.startsWith(schema.marker));
  if (verdicts.length !== 1 || pending.includes(schema.marker)) return null;
  return verdicts[0];
}

export const optionalCounter = (exports, name, emu) =>
  typeof exports[name] === 'function' ? exports[name](emu) : null;
