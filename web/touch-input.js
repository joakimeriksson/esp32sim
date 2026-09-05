// One active contact, sampled at most once per interval. Always deliver the
// newest pending move, including before a release; never assign guest times.
window.installTouchInput = function (canvas, send, { intervalMs = 40, trace = null } = {}) {
  let pointer = null, last = 0, pending = null, timer = null, sequence = 0;
  const now = () => performance.timeOrigin + performance.now();
  const sample = (e) => {
    const r = canvas.getBoundingClientRect();
    return { x: Math.max(0, Math.min(canvas.width - 1, Math.round((e.clientX - r.left) * canvas.width / r.width))),
      y: Math.max(0, Math.min(canvas.height - 1, Math.round((e.clientY - r.top) * canvas.height / r.height))),
      id: ++sequence, event: e.type, arrivalMs: now() };
  };
  const publish = (point, down) => {
    last = performance.now();
    const timing = trace ? { ...point, down: down ? 1 : 0, sentMs: now() } : undefined;
    if (timing) trace({ stage: 'page-send', ...timing });
    send(JSON.stringify({ t: 'touch', x: point.x, y: point.y, down: down ? 1 : 0 }), timing);
  };
  const flush = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    if (pending) { const point = pending; pending = null; publish(point, true); }
  };
  canvas.addEventListener('pointerdown', (e) => {
    if (pointer !== null) return;
    pointer = e.pointerId;
    canvas.setPointerCapture(pointer);
    publish(sample(e), true);
  });
  canvas.addEventListener('pointermove', (e) => {
    if (e.pointerId !== pointer) return;
    pending = sample(e);
    if (trace) trace({ stage: 'page-arrival', ...pending });
    const remaining = intervalMs - (performance.now() - last);
    if (remaining <= 0) flush();
    else if (timer === null) timer = setTimeout(flush, remaining);
  });
  const release = (e) => {
    if (e.pointerId !== pointer) return;
    flush();
    publish(sample(e), false);
    pointer = null;
  };
  canvas.addEventListener('pointerup', release);
  canvas.addEventListener('pointercancel', release);
  canvas.addEventListener('lostpointercapture', release);
};
