import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const html = readFileSync(new URL('../web/run.html', import.meta.url), 'utf8');
const emu = readFileSync(new URL('../web/emu.js', import.meta.url), 'utf8');
const workbench = readFileSync(new URL('../web/workbench.js', import.meta.url), 'utf8');

function page(rotate = 0) {
  const elements = new Map();
  const element = id => {
    if (!elements.has(id)) elements.set(id, { style: {}, textContent: '', classList: { toggle() {} } });
    return elements.get(id);
  };
  const context = vm.createContext({
    $: element, window: { dispatchEvent() {} }, CustomEvent: class {},
    document: { body: element('body'), querySelector: element },
    URLSearchParams, location: { search: '?rotate=' + rotate }, ArrayBuffer,
    boardName: null, lcdRotation: rotate, PANEL_ROTATE: {},
    setLcdRotation(value) { context.lcdRotation = +value; }, lineHint() { return ''; },
  });
  vm.runInContext(html.slice(html.indexOf('const PANEL_TEXT'), html.indexOf('const PANEL_ROTATE')), context);
  vm.runInContext(html.slice(html.indexOf('function syncTitle()'), html.indexOf('function setEmulatorStatus')), context);
  return { element, context, announce(name) { context.data = JSON.stringify({ t: 'board', name }); vm.runInContext('handleMessage(data)', context); } };
}

test('StopWatch is selectable and has its own workbench hardware specification', () => {
  const options = emu.match(/<select id="fw_board">(.*?)<\/select>/s)[1];
  assert.match(options, /<option>stopwatch<\/option>/);
  const context = vm.createContext({});
  vm.runInContext(workbench.slice(workbench.indexOf('const BOARD ='), workbench.indexOf('const HINTS =')) + '\nglobalThis.spec = BOARD.stopwatch;', context);
  assert.equal(context.spec.board, 'M5Stack StopWatch');
  assert.match(context.spec.peripherals, /CO5300 468×466 QSPI/);
  assert.match(context.spec.peripherals, /CST820 touch/);
  assert.match(context.spec.peripherals, /M5PM1 · M5IOE1/);
});

test('StopWatch announcement shows the native-size panel and button controls at each rotation', () => {
  for (const rotate of [0, 90, 180, 270]) {
    const p = page(rotate);
    p.announce('stopwatch');
    assert.equal(p.context.document.title, 'ESP32‑S3 emulator — M5Stack StopWatch');
    assert.equal(p.element('#lcdpanel').style.display, '');
    assert.equal(p.element('#lcdstat').textContent, 'CO5300 468×466 QSPI · CST820 touch');
    const quarter = rotate === 90 || rotate === 270;
    assert.equal(p.element('#lcd').width, quarter ? 466 : 468);
    assert.equal(p.element('#lcd').height, quarter ? 468 : 466);
    assert.equal(p.element('#lcd').style.width, (quarter ? 466 : 468) + 'px');
    assert.equal(p.element('#stopwatchbuttons').style.display, '');
    p.announce('atech14');
    assert.equal(p.element('#stopwatchbuttons').style.display, 'none');
  }
});

test('StopWatch A/B controls send the existing button protocol on GPIO2/1', () => {
  const markup = html.match(/<div class="row" id="stopwatchbuttons"[^>]*>(.*?)<\/div>/s)[1];
  const buttons = [...markup.matchAll(/data-pin="(\d+)">([^<]+)/g)].map(([, pin, label]) => {
    const classes = new Set();
    return { dataset: { pin }, label, classList: {
      contains: name => classes.has(name),
      toggle(name, on) { if (on) classes.add(name); else classes.delete(name); },
    } };
  });
  assert.deepEqual(buttons.map(b => [b.dataset.pin, b.label]), [['2', 'A (GPIO 2)'], ['1', 'B (GPIO 1)']]);
  const messages = [], timers = [];
  const context = vm.createContext({
    document: { querySelectorAll: () => buttons }, performance: { now: () => 0 },
    send: message => messages.push(message), setTimeout: callback => timers.push(callback),
  });
  vm.runInContext(html.slice(html.indexOf("document.querySelectorAll('.btn .cell, .boardbtn')"), html.indexOf('function turn(d)')), context);
  for (const button of buttons) { button.onmousedown(); button.onmouseup(); }
  timers.forEach(callback => callback());
  assert.equal(JSON.stringify(messages), JSON.stringify([
    { t: 'btn', pin: 2, v: 1 }, { t: 'btn', pin: 1, v: 1 },
    { t: 'btn', pin: 2, v: 0 }, { t: 'btn', pin: 1, v: 0 },
  ]));
});
