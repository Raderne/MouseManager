# Battery Reading

**Status: open question.** It's not yet known whether the PC can read the N230's battery level.

## Lead

Feature report 6 on collection `0xFF02` ([[N230 Hardware]]) read `06 00 02 00 EA 64 87 79` on 2026-10-19. Byte 5 is **`0x64` = 100**, which may be the battery %. The mouse was likely charged at the time.

## Progress (2026-10-19)

- **2026-11-07:** `--selftest` read report 6 as `06000200EB648779`. Byte 4 went `EA → EB` (a +1 step — looks like a rolling counter, not noise), while byte 5 is still `0x64` = 100. Battery still reads full, so still no drop to confirm the byte-5 hypothesis.
- **2026-11-07:** collection is now automated: a Windows scheduled task **"N230 Battery History"** runs `MouseManager.exe --hid` hourly (also on battery power, interactive-only) and appends report 6 to `%APPDATA%\MouseManager\report6.log`. Verified: the 12:18 run appended `06000200EB648779`, identical to 12:16 — byte 4 stayed `EB` across minutes, so it is not a per-read counter.

- The mouse has a **rechargeable battery** (USB), so test by comparing while charging and as it runs down.
- `MouseManager.exe --hid` dumps every report and appends report 6 to `%APPDATA%\MouseManager\report6.log`. Run it again later to build a history.
- There are two more candidates besides report 6: `0xFF01` input report 2 and `0xFFBC` input report 4 (all zeros so far). See [[N230 Hardware]].

## Spike 0a (in `App/STEPS.md`)

1. Read report 6 at several battery levels (on Windows through HID, on Linux through `/dev/hidraw*`). Check whether byte 5 drops.
2. On Linux, also check whether the kernel already lists the mouse under `/sys/class/power_supply/`.
3. If report 6 never changes, look at the vendor collections `0xFF01` and `0xFFBC` while the mouse is low or charging.

## Why no kernel driver (2026-11-05)

Asked whether a device driver is needed to get the battery and the rest. **No.** A driver would buy nothing:

- **User space already has the data.** Report 6 comes back through `HidD_GetFeature` on the `0xFF02` collection (`Monitor/Native.cs`, `MonitorForm.cs`), and on Linux through `/dev/hidraw*` (`hidapi` in Phase 9). A driver would hand over the same 8 bytes through a more expensive door.
- **There is no command channel to unlock.** No collection has output reports ([[N230 Hardware]]), so the PC cannot send anything regardless of mode. A driver cannot add a set-DPI or query protocol the firmware doesn't implement; the DPI button stays firmware-only.
- **The cost is real on both targets.** Windows needs WDK + KMDF + kernel-mode signing; Linux needs an out-of-tree module that breaks on kernel updates and fights Secure Boot. All to poll 8 bytes a minute from a tray app, against [[Stack Decision]]'s "one small binary, nothing to install".
- **Only OS-level integration would need a driver**, e.g. a `power_supply` device for upower on Linux, or the Settings battery readout on Windows (which also needs a standard HID battery usage page the N230 doesn't declare). Not needed for the app's own %.
- **If the kernel already exposes it, just read it.** `/sys/class/power_supply/` is poll-free plumbing worth checking in spike 0a; if it's there, the app reads the sysfs value and no HID code runs.
- **The blocker is the open question, not access.** If byte 5 never tracks charge, no driver changes that; the data isn't in the firmware's answers.

The only "driver-ish" plumbing that is actually needed is the Linux udev `TAG+="uaccess"` rule for `hidraw` ([[Linux Input Backend]]).

## Rules for the UI

- **Never show a fake value.** If the spike fails, remove the battery UI completely.
- While the mouse is asleep or disconnected, show "—", never a stale number.
- Poll every 60 s, and send an event only when the value changes. Below 15%, show one notification.

Related: [[Tauri Rewrite]] (Phase 7), [[Design System]] (battery ring).
