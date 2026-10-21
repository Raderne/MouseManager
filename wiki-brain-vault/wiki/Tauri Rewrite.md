# Tauri Rewrite

The new app in **`App/`**: Tauri 2, a Rust core, and a vanilla HTML/CSS/JS UI. It targets **Windows and Linux** ([[Stack Decision]]). The step-by-step plan, with a "Done when" check for each phase, is **`App/STEPS.md`**. This page is the overview.

## Architecture

```
UI (HTML/CSS/JS: Pulse screens, themes, DPI meter)
   │  invoke() commands ↓        ↑ emitted events
Rust core: config · mapping parser · event types        (shared)
   ├── platform/windows.rs   → [[Windows Input Backend]]
   └── platform/linux.rs     → [[Linux Input Backend]]
```

One process. Each OS module exposes the same functions, picked with `#[cfg]` (no trait):

```rust
pub fn start(config: Config, events: Sender<Event>) -> Handle;
impl Handle {
    pub fn apply(&self, config: Config) -> Result<(), String>;
    pub fn set_scale(&self, factor: f64);
    pub fn set_all_devices(&self, on: bool);
}
```

**Events:**
- `button {device, target, button, down}`
- `move {target, dx, dy}`, summed every 50 ms
- `battery {percent | null}`
- `settings {hex}`
- `device {connected, permission}`
- `error {message}`

## Config

JSON in Tauri's `app_config_dir()`, owned by Rust with serde, using defaults for missing keys:

```json
{ "devices": [{ "vid": "3554", "pid": "FC01" }],
  "buttons": { "X1": "Default", "X2": "Default" },
  "theme": "pulse", "startMinimized": true, "lowBatteryWarning": true }
```

- Devices are stored as **VID/PID**, not Windows path text, so one config works on both systems.
- Mapping syntax is the same as [[Monitor App]]. Each OS module translates the shared key names (Windows VK codes, Linux `KEY_*`).
- Mapping text is parsed **only in Rust**, never in JS.

## Phases (detail in `App/STEPS.md`)

0. Toolchains and spikes: battery (0a), Windows DPI scaling (0b), Linux node layout (0c)
1. Tauri skeleton (tray, single instance, autostart)
2. Themes ([[Design System]])
3. Shared core + `cargo test` (ports `SelfTest()`)
4. Windows backend
5. Buttons screen
6. Performance / DPI ([[DPI Measurement]])
7. Battery ([[Battery Reading]])
8. Monitor and Settings screens
9. Linux backend
10. Packaging (NSIS installer; `.deb`/`.rpm` with the udev rule; AppImage), then retire `Monitor/`

## Status

- 2026-10-19: plan written; no code yet. Next: Phase 0.
- 2026-10-21: **Phase 1 skeleton** done on Windows: `create-tauri-app` vanilla template, then `src-tauri/src/lib.rs` (tray Open/Exit, close hides to tray, `tauri-plugin-single-instance` focuses the first window, `tauri-plugin-autostart` registered) and `src/` (Pulse header, pill tabs, empty sections). Started while Phase 0 spikes 0a/0c were still open, by the user's choice. Linux check still pending.
- 2026-10-21: **Phase 2 themes** done on Windows: 5 themes, bundled fonts, component styles, and a Settings theme picker that saves through the `set_theme` command. `config.rs` was started early (theme only). Details: [[Design System]] → Implementation. Linux side-by-side still pending.
- 2026-10-21: **Phase 3 shared core** done on Windows (11 `cargo test` checks): `config.rs` (full config), `action.rs`, `event.rs`, and `platform/` with a stub backend. Linux `cargo test` still pending.

## Rust core (Phase 3)

- **`action.rs`** is the only mapping parser. `parse(text) -> Result<Action, String>`, where `Action` is `PassThrough | Disabled | Mouse(Btn) | Keys(Vec<Key>) | Run(String)`.
  - `parse_buttons` prefixes the button to the error: `X2: unknown key 'Nope'`.
  - Rules carried over from [[Monitor App]]:
    - Names ignore case.
    - A whole mapping is checked against mouse names before key names (`Back` = mouse).
    - Inside a combo, `Back` is Backspace (`Ctrl+Back`), as in the C# app.
- **`Key` is OS-neutral:**
  - `Letter`, `Digit`, `F(1..=24)` and `NumPad(0..=9)` carry a value; the rest are named variants.
  - The parser accepts both friendly names (`Esc`, `Minus`) and the old WinForms `Keys` names (`Return`, `Prior`, `OemMinus`, …), so old mapping text keeps working.
  - Each backend maps `Key` with an exhaustive `match`, so a new key can't be forgotten on one OS.
- **Config differs from the C# app:**
  - Buttons are keyed `X1`/`X2` (C#: `Middle`/`Back`/`Forward`).
  - Devices are VID/PID objects (C#: path substrings).
  - As in C#, saving drops keys the struct doesn't know.
- **`event.rs`:** all events go to the UI on one channel as `{"type":"button"|"move"|"battery"|"settings"|"device"|"error", …}`. Battery `null` = unknown.
- **`platform/`:** `stub.rs` serves every OS for now; its `apply` only validates. Phase 4 adds `windows.rs` behind `#[cfg(windows)]`.

## Build gotchas (Windows)

- **Two Visual Studio installs.** `rustc` picks the newest VS, which is *Visual Studio Community 2026* (`C:\Program Files\Microsoft Visual Studio\18\Community`). Its C++ toolset is partial: `VC\Tools\MSVC\14.51…\lib\x64` is missing, so linking fails with `LNK1104: cannot open file 'msvcrt.lib'`.
  - **Workaround:** build inside the *VS 2022 Build Tools* environment:
    `cmd /c '"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && npm run tauri dev'`, or use the "x64 Native Tools Command Prompt for VS 2022".
  - **Permanent fix:** add the *Desktop development with C++* workload (x64 libraries) to VS 2026 in the Visual Studio Installer.
- `cargo` lives in `%USERPROFILE%\.cargo\bin`. Shells opened before the install don't have it on `PATH` until restarted.
- A full Rust + Tauri debug build needs several GB of disk space. On 2026-10-21, with 0.5 GB free, the rustup install failed with "not enough space".

Related: [[N230 Control]].
