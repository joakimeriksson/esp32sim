// Compilation is nested inside runMs, not an extra time bucket.
export class DiagnosticTimer {
  reset(now, cycles) {
    this.start = this.last = this.end = now;
    this.startCycles = this.cycles = cycles;
    this.runMs = this.drainMs = this.gapMs = this.maxGapMs = this.catchupGapMs = 0;
    this.catchup = false;
  }
  enter(now) {
    const gap = now - this.end;
    this.gapMs += gap;
    if (this.catchup) this.catchupGapMs += gap;
    this.maxGapMs = Math.max(this.maxGapMs, gap);
  }
  leave(now, cycles, hz, jit, catchup) {
    this.end = now; this.catchup = catchup;
    const wallMs = now - this.last;
    if (wallMs < 1000) return null;
    const result = { elapsedSec: (now - this.start) / 1000, wallMs,
      speed: (cycles - this.cycles) / hz / (wallMs / 1000),
      averageSpeed: (cycles - this.startCycles) / hz / ((now - this.start) / 1000),
      runMs: this.runMs, drainMs: this.drainMs, gapMs: this.gapMs,
      catchupGapMs: this.catchupGapMs, maxGapMs: this.maxGapMs,
      otherMs: Math.max(0, wallMs - this.runMs - this.drainMs - this.gapMs), jit: { ...jit } };
    this.last = now; this.cycles = cycles;
    this.runMs = this.drainMs = this.gapMs = this.catchupGapMs = this.maxGapMs = 0;
    return result;
  }
}
