// Run: npm run check
import assert from "node:assert/strict";
import { Meter, dpiOf, LEVELS, snapLevel, confirmLevel } from "../src/dpi.js";

const near = (a, b, tol = 0.5) => assert.ok(Math.abs(a - b) < tol, `${a} is not ~${b}`);
let t = 0;
const slide = (m, dx, dy = 0) => {
  m.move(dx, dy, (t += 1000));
  assert.equal(m.tick(t + 100, 10), null, "still sliding");
  return m.tick(t + 500, 10);
};

near(dpiOf(3150, 10), 800.1, 0.1); // port of SelfTest(): DPI from counts over 10 cm

const m = new Meter(1424);
assert.equal(m.tick(0, 10), null, "idle");

let r = slide(m, 2000); // stray move to the ruler's start
assert.equal(r.confirmed, undefined);
r = slide(m, 5605); // first real slide: doesn't agree with the stray one
near(r.dpi, 1423.7);
assert.equal(r.confirmed, undefined);
r = slide(m, 5604); // second agrees within 3%
near(r.confirmed, 1423.6);
assert.equal(r.changed, false);

assert.deepEqual(slide(m, 43), { ignored: true }, "a 1 mm nudge");

m.move(3000, 300, (t += 1000)); // wobble cancels: net displacement
m.move(2604, -300, t + 10);
r = m.tick(t + 500, 10);
near(r.dpi, 1423.4);
assert.ok(r.confirmed, "agrees with the previous slide");

slide(m, 3130); // DPI button pressed, then two slides at ~795
r = slide(m, 3135);
assert.equal(Math.round(r.confirmed), 796);
assert.equal(r.changed, true);

for (let i = 0; i < 4; i++) slide(m, 3130);
assert.equal(m.recent.length, 5, "keeps the last 5");

// Levels (the N230's DPI button): real slides from 2026-11-08 snap to the manual's levels.
assert.equal(snapLevel(845), 1000, "step 1 read 800–890: user error, the level is 1000");
assert.equal(snapLevel(890), 1000);
assert.equal(snapLevel(1159), 1200);
assert.equal(snapLevel(1243), 1200);
assert.equal(snapLevel(1515), 1600);
assert.equal(snapLevel(1595), 1600);
assert.equal(snapLevel(1424), 1600, "the old ~1424 (short slides at 1600)");
assert.equal(snapLevel(400), null, "far from every level: a bad slide, not a level");
assert.equal(snapLevel(2400), null);
assert.equal(snapLevel(0), null);
assert.equal(snapLevel(NaN), null);

assert.equal(confirmLevel([1243, 1159]), 1200, "two slides 7% apart still confirm the same level");
assert.equal(confirmLevel([1595, 1515, 845]), 1600, "only the two newest count");
assert.equal(confirmLevel([1595, 1159]), null, "different levels: the button was pressed between slides");
assert.equal(confirmLevel([1595]), null, "one slide isn't enough");
assert.equal(confirmLevel([400, 410]), null, "agreeing but not near a level");
assert.deepEqual(LEVELS, [1000, 1200, 1600]);

console.log("dpi check OK");
