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

## Build gotchas (Windows)

- **Two Visual Studio installs.** `rustc` picks the newest VS, which is *Visual Studio Community 2026* (`C:\Program Files\Microsoft Visual Studio\18\Community`). Its C++ toolset is partial: `VC\Tools\MSVC\14.51…\lib\x64` is missing, so linking fails with `LNK1104: cannot open file 'msvcrt.lib'`.
  - **Workaround:** build inside the *VS 2022 Build Tools* environment:
    `cmd /c '"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && npm run tauri dev'`, or use the "x64 Native Tools Command Prompt for VS 2022".
  - **Permanent fix:** add the *Desktop development with C++* workload (x64 libraries) to VS 2026 in the Visual Studio Installer.
- `cargo` lives in `%USERPROFILE%\.cargo\bin`. Shells opened before the install don't have it on `PATH` until restarted.
- A full Rust + Tauri debug build needs several GB of disk space. On 2026-10-21, with 0.5 GB free, the rustup install failed with "not enough space".

Related: [[N230 Control]].
