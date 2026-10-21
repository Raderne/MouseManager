// DPI meter: sums the N230's raw counts per slide. Raw counts ignore pointer speed and
// acceleration, so counts per inch is the real hardware DPI. Pure; checked by tests/dpi.mjs.

export const SLIDE_END_MS = 400; // a slide ends after this long without movement
export const MIN_DPI = 120; // anything lower is a nudge, not a slide (the old "~11 DPI" bug)
export const AGREE = 0.03; // two slides in a row within 3% confirm a reading
export const JUMP = 0.15; // a confirmed reading this far from the last one = DPI button pressed
export const KEEP = 5;

export const dpiOf = (counts, cm) => counts / (cm / 2.54);

export class Meter {
  constructor(lastDpi = null) {
    this.last = lastDpi; // last confirmed reading
    this.recent = []; // last slides, newest first
    this.reset();
  }

  reset() {
    this.x = 0;
    this.y = 0;
    this.at = null;
  }

  move(dx, dy, now) {
    this.x += dx;
    this.y += dy;
    this.at = now;
  }

  // Call often. null while sliding or idle. Otherwise:
  //   { ignored: true }               a nudge
  //   { dpi }                          a slide, waiting for a second one that agrees
  //   { dpi, confirmed, changed }      two slides agree; confirmed = their average
  // Needing two agreeing slides also drops stray moves (like moving to the ruler's start).
  tick(now, cm) {
    if (this.at === null || now - this.at < SLIDE_END_MS) return null;
    const dpi = dpiOf(Math.hypot(this.x, this.y), cm); // net displacement, so wobble cancels
    this.reset();
    if (dpi < MIN_DPI) return { ignored: true };
    this.recent = [dpi, ...this.recent].slice(0, KEEP);
    const [a, b] = this.recent;
    if (b === undefined || Math.abs(a - b) / b > AGREE) return { dpi };
    const confirmed = (a + b) / 2;
    const changed = this.last !== null && Math.abs(confirmed - this.last) / this.last > JUMP;
    this.last = confirmed;
    return { dpi, confirmed, changed };
  }
}
