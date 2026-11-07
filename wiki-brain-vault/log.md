# Wiki log

Append-only chronological record of everything that happened in this wiki.

**Format:**

```
## [YYYY-MM-DD HH:MM] <type> | <title>
Touched: <comma-separated wiki pages, or "none">
```

**Types:** `session`, `ingest`, `query`, `lint`, `rebuild`

**Quick look:** `grep "^## \[" log.md | tail -5` shows the last 5 entries.

---

## [2026-10-19 13:23] session | Bootstrap wiki-brain for N230 Control
Touched: N230 Control, N230 Hardware, Monitor App, Windows Input Backend, Linux Input Backend, Tauri Rewrite, Stack Decision, DPI Measurement, Battery Reading, Design System, CLAUDE.md Rules, Wiki-Brain Setup, index

## [2026-10-19 14:00] session | Phase 0: toolchain, HID scan, scaling spike
Touched: N230 Hardware, DPI Measurement, Battery Reading

## [2026-10-21 21:35] session | Phase 1 Tauri skeleton on Windows
Touched: Tauri Rewrite

## [2026-10-21 21:43] session | Phase 2 themes and design tokens
Touched: Design System, Tauri Rewrite

## [2026-10-21 21:47] session | Phase 3 shared Rust core
Touched: Tauri Rewrite, Monitor App

## [2026-10-21 21:54] session | Phase 4 Windows backend code
Touched: Windows Input Backend

## [2026-10-21 22:19] session | Phase 5 Buttons screen
Touched: Tauri Rewrite

## [2026-10-21 22:27] session | Phase 6 Performance screen and DPI
Touched: DPI Measurement, Design System, Tauri Rewrite

## [2026-10-31 12:00] session | Pulled repo, summarized phase status
Touched: none

## [2026-11-05 22:44] session | Driver question: answer is no driver
Touched: Battery Reading, index

## [2026-11-07 12:06] query | Phase status check
Touched: none

## [2026-11-07 12:13] session | Built and ran Monitor; new report-6 reading
Touched: Battery Reading

## [2026-11-07 12:19] session | Hourly battery-collection scheduled task
Touched: Battery Reading

## [2026-11-07 13:40] session | Phase 8 Monitor + Settings built; Rust link blocked
Touched: Tauri Rewrite

## [2026-11-07 14:08] session | C++ workload installed; cargo test passes (18)
Touched: Tauri Rewrite

## [2026-11-07 14:12] session | Phase 9 blocked on Linux hardware; stopped
Touched: Tauri Rewrite

## [2026-11-07 14:40] ingest | raw/bluetooth-phases-plan.md
Touched: Bluetooth Connection, index

## [2026-11-07 14:40] session | Bluetooth connection phases plan written
Touched: Bluetooth Connection, N230 Hardware, Tauri Rewrite, index

## [2026-11-07 15:00] session | B0 spike: N230 found over BLE
Touched: N230 Hardware

## [2026-11-07 15:20] session | B0: N230 BLE GATT, PnP ID, battery 83%
Touched: N230 Hardware, Battery Reading

## [2026-11-07 15:30] session | B0: N230 paired over BLE, paths and battery
Touched: N230 Hardware

## [2026-11-07 15:50] session | B0 closed, B1 device identity built
Touched: Bluetooth Connection, N230 Hardware, Tauri Rewrite

## [2026-11-07 16:00] session | B2 device-change handling built
Touched: Bluetooth Connection

## [2026-11-07 16:20] session | B2 done on Windows (hands-on)
Touched: Bluetooth Connection

## [2026-11-07 16:50] session | B3 Bluetooth search built (Windows)
Touched: Bluetooth Connection

## [2026-11-07 17:10] session | B3 Stop button bug fixed
Touched: Bluetooth Connection

## [2026-11-07 17:20] session | B3 done on Windows (hands-on)
Touched: Bluetooth Connection
