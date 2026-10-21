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
