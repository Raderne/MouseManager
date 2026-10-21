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
