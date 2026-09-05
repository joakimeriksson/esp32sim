import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../web/touch-input.js', import.meta.url), 'utf8');
function panel() {
  let clock = 0, nextTimer = 0;
  const timers = new Map(), messages = [];
  const canvas = Object.assign(new EventTarget(), {
    width: 368, height: 448,
    getBoundingClientRect: () => ({left: 10, top: 20, width: 368, height: 448}),
    setPointerCapture() {},
  });
  const context = {
    window: {}, performance: {timeOrigin: 1000, now: () => clock},
    setTimeout: (callback, delay) => { const id = ++nextTimer; timers.set(id, {callback, at: clock + delay}); return id; },
    clearTimeout: id => timers.delete(id),
  };
  vm.runInNewContext(source, context);
  context.window.installTouchInput(canvas, data => messages.push(JSON.parse(data)));
  return {
    messages,
    event(type, x, y = 100, pointerId = 1) {
      canvas.dispatchEvent(Object.assign(new Event(type), {clientX: x + 10, clientY: y + 20, pointerId}));
    },
    advance(ms) {
      const end = clock + ms;
      while (true) {
        const next = [...timers].filter(([,timer]) => timer.at <= end).sort((a,b) => a[1].at-b[1].at)[0];
        if (!next) break;
        const [id, timer] = next; clock = timer.at; timers.delete(id); timer.callback();
      }
      clock = end;
    },
  };
}
const touch = (x, down = 1, y = 100) => ({t: 'touch', x, y, down});

test('a dense burst delivers its newest point without needing another event', () => {
  const p = panel();
  p.event('pointerdown', 10);
  for (let x=11;x<=39;x++) { p.advance(1); p.event('pointermove', x); }
  assert.deepEqual(p.messages, [touch(10)]);
  p.advance(11);
  assert.deepEqual(p.messages, [touch(10),touch(39)]);
  p.advance(200);
  assert.equal(p.messages.length, 2, 'a stationary held contact does not repeat old moves');
});

for (const release of ['pointerup', 'pointercancel', 'lostpointercapture']) {
  test(`${release} preserves the pending point and ends the contact exactly once`, () => {
    const p = panel();
    p.event('pointerdown', 10);
    p.advance(5); p.event('pointermove', 20);
    p.event(release, 25);
    p.event('pointerup', 25);
    p.advance(100);
    assert.deepEqual(p.messages, [touch(10),touch(20),touch(25,0)]);
    p.event('pointermove', 30);
    assert.equal(p.messages.length, 3, 'movement after release cannot resurrect a contact');
  });
}

test('rapid successive contacts stay ordered and a second pointer cannot release the first', () => {
  const p = panel();
  p.event('pointerdown', 10);
  p.event('pointerdown', 200, 100, 2);
  p.event('pointermove', 220, 100, 2);
  p.event('pointerup', 220, 100, 2);
  p.advance(2); p.event('pointerup', 10);
  p.event('pointerdown', 30);
  p.advance(2); p.event('pointermove', 40);
  p.event('pointerup', 40);
  p.advance(100);
  assert.deepEqual(p.messages, [touch(10),touch(10,0),touch(30),touch(40),touch(40,0)]);
});

test('captured movement outside the panel is clamped to the controller bounds', () => {
  const p = panel();
  p.event('pointerdown', -20, -10);
  p.advance(50); p.event('pointermove', 500, 600);
  p.event('pointerup', 500, 600);
  assert.deepEqual(p.messages, [touch(0,1,0),touch(367,1,447),touch(367,0,447)]);
});
