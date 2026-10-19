# Battery Reading

**Status: open question.** It's not yet known whether the PC can read the N230's battery level.

## Lead

Feature report 6 on collection `0xFF02` ([[N230 Hardware]]) read `06 00 02 00 EA 64 87 79` on 2026-10-19. Byte 5 is **`0x64` = 100**, which may be the battery %. The mouse was likely charged at the time.

## Spike 0a (in `App/STEPS.md`)

1. Read report 6 at several battery levels (on Windows through HID, on Linux through `/dev/hidraw*`). Check whether byte 5 drops.
2. On Linux, also check whether the kernel already lists the mouse under `/sys/class/power_supply/`.
3. If report 6 never changes, look at the vendor collections `0xFF01` and `0xFFBC` while the mouse is low or charging.

## Rules for the UI

- **Never show a fake value.** If the spike fails, remove the battery UI completely.
- While the mouse is asleep or disconnected, show "—", never a stale number.
- Poll every 60 s, and send an event only when the value changes. Below 15%, show one notification.

Related: [[Tauri Rewrite]] (Phase 7), [[Design System]] (battery ring).
