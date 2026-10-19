# Stack Decision

**Decision (2026-10-19): Tauri 2 + Rust**, for both the UI and the input manager, on Windows and Linux.

## How we got there

1. First plan: an **Electron** UI plus the C# app as a headless `--serve` helper over stdin/stdout JSON. Electron can't hook input itself, because Node doesn't pump Win32 messages.
2. The user asked about Rust for a faster, more reliable core. With a Rust core, Electron only adds ~150 MB and a second process, so → **Tauri** (the same web UI, calling Rust directly).
3. The user then added **Linux** as a target. That ruled out everything tied to Windows:
   - C# + WebView2 (WinForms and WebView2 are Windows-only)
   - the C# helper (`user32` P/Invoke)

## Options compared

| Option | Verdict |
|---|---|
| **Tauri 2 + Rust** | ✅ One process, ~5–10 MB, the system webview, `windows` + `evdev` crates, official tray/autostart/single-instance plugins |
| Electron + C# helper | ❌ Two processes, ~150 MB, and the helper is Windows-only |
| C# + WebView2 | ❌ Windows-only (it was the shortest path before Linux was added) |
| Native Rust UI (egui/Slint/iced) | ❌ Throws away the HTML design ([[Design System]]) |
| C++ | ❌ The same control as Rust, with more ways to crash |
| Go | ❌ Pinning the hook to one thread is awkward, and the GC can pause inside the hook |
| Zig | ❌ Too young for desktop apps |

## Why Rust (and what it doesn't change)

- C# was **not slow**: the hook does microseconds of work.
- The gains:
  - no GC pause inside the low-level hook ([[Windows Input Backend]] § hook timeout)
  - a small standalone binary with no runtime to install
  - memory safety
  - one codebase for both systems

## Costs accepted

- Rewriting about 375 lines of C# core ([[Monitor App]]).
- WebKitGTK rendering on Linux needs testing.
- Linux needs udev permissions ([[Linux Input Backend]]).

Related: [[Tauri Rewrite]], [[N230 Control]].
