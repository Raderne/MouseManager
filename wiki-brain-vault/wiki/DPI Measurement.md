# DPI Measurement

The N230's DPI button sends nothing ([[N230 Hardware]]), so the PC can't read or set the hardware DPI. It can **measure** it, and possibly **scale** it in software.

## Measuring (implemented in `Monitor/MonitorForm.cs`)

- Sum the raw movement counts (dx, dy) of the N230. A slide ends after **400 ms** without movement.
- Use the net displacement, `sqrt(dx² + dy²)`, so wobble cancels out.
- DPI = counts ÷ (distance in cm ÷ 2.54). The user slides a known distance (default 10 cm).
- Flag "DPI CHANGED" on a jump of more than **15%** from the previous slide.

### Gotcha: the ~11 DPI reading

`MinStrokeCounts = 20` is far too low. A 1 mm nudge (43 counts) was counted as a full 10 cm slide and read as "~11 DPI". That produced two false "changed" flags: one on the nudge, and one on the next real slide. **Fix in the rewrite:** ignore slides under 30% of the expected count for the last known DPI.

### Readings (2026-10-19)

1424, 1423 (both reliable), then 795. The 795 is either a real DPI press (about 800) or a short slide; not confirmed.

## Spike 0b result (2026-10-19): Windows scaling works ✅

Run with `MouseManager.exe --scale <factor> <seconds>` (temporary spike code in `Monitor/Remapper.cs`; it ends by itself).

| Measure | Result |
|---|---|
| Time spent in the hook per move | **10–26 µs average**, 1.4 ms worst case (Windows removes a hook after about 300 ms) |
| Raw counts → scaled move re-sent | **54–85 µs average**, 1.9 ms worst case |
| CPU | under 1% while moving |
| Pointer px ÷ sensor counts | ×1: **1.0–1.4** · ×0.25: **0.11–0.25** |

- The pointer moved far less at ×0.25, so it works. The drop is bigger than 4× because **"Enhance pointer precision" is on** (`HKCU\Control Panel\Mouse\MouseSpeed = 1`): slower input gets less speed-up. The real app should either accept that (it's how a lower-DPI mouse would behave) or offer a "match exactly" option that sends absolute moves. Decide in Phase 6.
- Feel: at ×0.5 the user didn't notice a change. Suspected cause: the moves happened in the first seconds while reading the instructions. Use ×0.25 or the measured ratio, not feel, to confirm.
- The hook counts injected moves as well. Swallowed ≈ the N230's own moves; the rest are our re-sent moves (and any other mouse).

## Software DPI scaling (planned)

The target stages (Native / 400 / 800 / 1200 / 1600) scale the N230's movement by `target / measured`, for that device only.
- **Linux:** trivial. Multiply `REL_X`/`REL_Y` in the pass-through ([[Linux Input Backend]]).
- **Windows:** swallow `WM_MOUSEMOVE` from the N230 in the hook and re-inject the scaled movement, carrying the fractional remainder over. Latency and CPU use at 1000 Hz still need measuring: **spike 0b** ([[Windows Input Backend]]).
- If the measured DPI changes (someone pressed the button on the mouse), re-send the scale.

## As built in the rewrite (Phase 6, 2026-10-21)

- **Meter:** `App/src/dpi.js` (pure, checked by `npm run check`) reads the `move` events where `target` is true. Those are raw counts, so the meter reads the hardware DPI even while scaling is on.
  - A slide ends after 400 ms still.
  - Net displacement is used.
  - Slides under **120 DPI** are ignored as nudges (this replaced "30% of the last DPI", which could lock out a real large drop).
- **Two-slide confirmation:** a reading counts only when **two slides in a row agree within 3%**; the saved value is their average. This drops stray moves, like moving the mouse to the ruler after clicking "Measure DPI". A confirmed reading more than **15%** off the previous one shows "DPI changed".
- **Stages:** the stage and the measured DPI are saved (`measuredDpi`, `targetDpi`). Rust's `Config::scale()` = target ÷ measured, clamped to 0.1–10, and is applied at startup. A new measurement re-applies the scale.
- **Pointer precision:** accepted as is. With "Enhance pointer precision" on, a scaled-down N230 behaves like a lower-DPI mouse, so expect less than the exact ratio.

Related: [[Tauri Rewrite]] (Phase 6), [[Design System]] (dial).

## Can the app change the mouse's own DPI? (2026-11-08)

**No: not readable, not settable without an unknown protocol.** Software DPI (movement scaling) is what the app offers.

- **Receiver:** the DPI button changes nothing in feature report 6, and no vendor channel sends anything when it's pressed (`hidspike listen`, see [[Battery Reading]] → Receiver). No output reports, so nothing to send.
- **Bluetooth:** vendor service `FFF0` = `FFF1` (WriteWithoutResponse), `FFF2` and `FFF3` (Read only, no Notify). A read-only poll every second for 120 s while the user pressed the DPI button 3 times: **no change**. `FFF2` = `01 18` + "Lenovo Services N2…" (record type 1, length 24: the name); `FFF3` = `02 0A` + spaces/zeros (type 2, length 10, empty). They look like a small record store (name / ID), and `FFF1` probably selects or writes records or is the OTA channel. Nothing there tracks DPI.
- Setting it would mean writing undocumented bytes to `FFF1` or feature report 6. There's no Lenovo software to learn from, and a wrong write could change or brick settings. **Not done.**

## Performance screen: levels (2026-11-08)

- **Mouse DPI picker 1000 / 1200 / 1600:** the user taps the level the button is on, since the PC can't see it. That exact value becomes `measuredDpi`, so `Config::scale()` = target ÷ level.
- **"Check with a ruler"** (was "Measure DPI"): each slide snaps to the nearest level (`snapLevel`, within 30 %). Two slides in a row on the same level confirm it (`confirmLevel`), replacing the 3 % agreement that ±5–10 % hand slides kept failing. A slide far from every level says so.
- **Pointer-speed stages** gained **1000** (Native / 400 / 800 / 1000 / 1200 / 1600).
- An old free-form value (this PC had 752) shows "(old reading: pick a level)" instead of being scaled from silently.
- Checks: `tests/dpi.mjs` asserts the real 2026-11-08 slides snap to 1000/1200/1600, the old 1424 snaps to 1600, and two slides on different levels don't confirm.

## Manual vs measured (2026-11-08)

The product manual says the DPI button cycles **1000 / 1200 / 1600** ([[N230 Hardware]] → From the product manual).

**Measured per button step (2026-11-08, receiver, two 10 cm slides each):**

| Step | Slides | Average | Manual |
|---|---|---|---|
| 1 | 800, 890 | ~845 | 1000 |
| 2 | 1159, 1243 | ~1200 | 1200 ✓ |
| 3 | 1595, 1515 | ~1555 | 1600 ✓ |

- **Decision (user, 2026-11-08): the levels are 1000 / 1200 / 1600, as the manual says.** Step 1's low readings (and the October ~795) are put down to slide error; step 1 wasn't rechecked. If a later careful measurement keeps landing near 800, revisit this.
- Hand slides vary by about ±5 % here (pairs differ 7–11 %), more than the meter's 3 % "agree" rule assumes. Several slides per step give a better number.
- The DPI button is firmware-only and invisible to the PC, so `measuredDpi` (and the software-scaling math) is only right for the step that was measured. The app can't tell when the user presses the button.
- PAW3212 sensor: up to 2400 CPI in ~38 CPI steps, 30 ips, adaptive up to 4000 fps (third-party datasheet mirror). The 3 levels are a firmware choice within that.
