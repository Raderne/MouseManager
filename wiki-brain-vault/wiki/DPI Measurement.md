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

## Software DPI scaling (planned)

The target stages (Native / 400 / 800 / 1200 / 1600) scale the N230's movement by `target / measured`, for that device only.
- **Linux:** trivial. Multiply `REL_X`/`REL_Y` in the pass-through ([[Linux Input Backend]]).
- **Windows:** swallow `WM_MOUSEMOVE` from the N230 in the hook and re-inject the scaled movement, carrying the fractional remainder over. Latency and CPU use at 1000 Hz still need measuring: **spike 0b** ([[Windows Input Backend]]).
- If the measured DPI changes (someone pressed the button on the mouse), re-send the scale.

Related: [[Tauri Rewrite]] (Phase 6), [[Design System]] (dial).
