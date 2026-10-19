# Wiki index

Catalogue of every page in this wiki. Maintained by Claude.

**Format:** `- [[Page name]]: one-line summary`

---

## Entities

- [[N230 Control]]: the product, a Synapse-style manager for the Lenovo N230 (X1/X2 remap, DPI, battery, themes) for Windows and Linux.
- [[N230 Hardware]]: measured facts about the mouse (device IDs, HID collections, what each button sends, feature report 6).
- [[Monitor App]]: the working C# WinForms tray app in `Monitor/`, the Windows reference until the rewrite ships.
- [[Tauri Rewrite]]: the new app in `App/` (Tauri 2 + Rust): architecture, platform interface, config, and the phases in `App/STEPS.md`.

## Concepts

- [[Windows Input Backend]]: low-level hook + Raw Input on one window, so only the N230's clicks are blocked and remapped.
- [[Linux Input Backend]]: take exclusive control of the N230 (evdev grab) and re-send through a virtual uinput device; udev permissions.
- [[DPI Measurement]]: DPI from counts ÷ inches, the short-slide bug, and software DPI scaling.
- [[Battery Reading]]: open question; the `0x64` byte in report 6 is the lead.
- [[Design System]]: Pulse layout, 5 color themes as CSS variables, and the design canvas link.

## Decisions

- [[Stack Decision]]: why Tauri 2 + Rust, and not Electron + C#, C# + WebView2 or a native Rust UI.

## Sources

_(summaries of sources ingested from `raw/`: none yet)_

## Analyses / known gotchas

- [[Windows Input Backend]] § gotchas: one Raw Input registration per process; nothing slow inside the hook.
- [[DPI Measurement]] § the ~11 DPI reading: a 20-count cutoff counts a 1 mm nudge as a full slide.

## Meta

- [[CLAUDE.md Rules]]: the project's hard rules.
- [[Wiki-Brain Setup]]: how this wiki is set up and maintained.

---

_Maintained by Claude. Don't edit by hand unless you know what you're doing._
