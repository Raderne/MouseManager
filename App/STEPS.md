# N230 Control: implementation plan (Tauri 2 + Rust, Windows and Linux)

**Design:** the **B · Pulse** layout plus a color theme setting (Pulse green, Ember red, Frost, Amber, Violet). Canvas: https://claude.ai/artifact/ShCfcoUsPKsJ3wegajBHxi (screens "B · Pulse" and "B · Settings").

**Features:** remap X1/X2 on the N230 only; show and measure DPI; target DPI through software scaling (if Phase 0 says yes); battery % (if Phase 0 says yes); a live monitor; themes; tray; start with the OS.

Each phase ends with a **Done when** check. Don't start the next phase until it passes. The C# app in `../Monitor` stays as the working Windows reference until Phase 9.

## Architecture

One process. The UI is HTML/JS in the system webview (WebView2 on Windows, WebKitGTK on Linux), and the core is Rust.

```
UI (HTML/CSS/JS: Pulse screens, themes, DPI meter)
   │  invoke() commands ↓        ↑ emitted events
Rust core: config · mapping parser · event types        (shared)
   │  start / apply / set_scale / set_all_devices / battery
   ├── platform/windows.rs   low-level mouse hook + Raw Input + SendInput, HID feature report
   └── platform/linux.rs     evdev grab + uinput virtual mouse/keyboard, hidraw
```

**Windows** (a port of `Remapper.cs`):
- A dedicated thread runs a message-only window, Raw Input and a `WH_MOUSE_LL` hook, plus a Win32 message loop.
- On button-down, drain `WM_INPUT` to learn which device sent the click, and remember that per button so the button-up gets the same decision.
- Run actions on `WM_APP`, never inside the hook. Ignore injected input.

**Linux:**
- Find the N230 input node whose vendor:product is `3554:FC01` and that has `BTN_SIDE`.
- Take exclusive control of it with `EVIOCGRAB`, and create a `uinput` virtual device with the same capabilities plus keyboard keys.
- Pass every event through, except X1 (`BTN_SIDE`) and X2 (`BTN_EXTRA`), which run their mapping. Because only that one device is grabbed, other mice are never touched.

**Platform interface:** the same few functions in both modules, picked with `#[cfg]`. No trait object is needed for two modules that can never both be compiled at once.

```rust
pub fn start(config: Config, events: Sender<Event>) -> Handle;
impl Handle {
    pub fn apply(&self, config: Config) -> Result<(), String>;  // Err names the button, nothing changes
    pub fn set_scale(&self, factor: f64);                        // 1.0 = native DPI
    pub fn set_all_devices(&self, on: bool);                     // monitor: report other devices too
}
```

**Events to the UI:**
- `button {device, target, button, down}`
- `move {target, dx, dy}`, summed every 50 ms
- `battery {percent | null}`
- `settings {hex}`
- `device {connected, permission}`
- `error {message}`

**Config** (JSON in Tauri's `app_config_dir()`, read and written only by Rust with serde, using defaults for missing keys):

```json
{ "devices": [{ "vid": "3554", "pid": "FC01" }],
  "buttons": { "X1": "Default", "X2": "Default" },
  "theme": "pulse", "startMinimized": true, "lowBatteryWarning": true }
```

Devices are stored as VID/PID, not Windows path text, so one config works on both systems. Mapping strings use the same syntax as today (`Ctrl+Shift+T`, `VolumeUp`, `Back`, `run:calc.exe`, `Disabled`). Each OS module translates the shared key names to its own codes (Windows VK codes, Linux `KEY_*`).

---

## Phase 0: Setup and spikes

**Toolchains**
- **Windows:** Rust (`rustup`, MSVC target), Node LTS, WebView2 (already part of Windows 11).
- **Linux** (Debian/Ubuntu names): `build-essential libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev libevdev-dev`, plus Rust and Node.
- Linux testing needs a real Linux machine or a live USB with the dongle plugged in. WSL2 doesn't see USB input devices without extra setup.

**Spikes** (throwaway, about half a day each):
- **0a. Battery.**
  - Read feature report 6 at a few battery levels. Byte 5 is `0x64` = 100 today, which may be the battery level.
  - On Linux, read the same report through `/dev/hidraw*`. Also check whether the kernel already lists the mouse under `/sys/class/power_supply/`.
  - If report 6 never changes, look at the vendor collections `0xFF01` and `0xFFBC`.
- **0b. Software DPI scaling on Windows:**
  - Swallow `WM_MOUSEMOVE` from the N230 in the hook and re-inject the scaled movement, carrying the fractional remainder over between moves.
  - Measure latency and CPU use at 1000 Hz.
  - On Linux this is just multiplying `REL_X`/`REL_Y` in the pass-through, so there's no spike to run.
- **0c. Linux device layout:**
  - With `evtest`, list the N230's input nodes and find the one with `REL_X` + `BTN_SIDE` + `BTN_EXTRA`.
  - Confirm X1 = `BTN_SIDE` and X2 = `BTN_EXTRA`.
  - Note which nodes carry the consumer and system controls, so they're left alone.

**Done when:** the three results are written in the wiki (`N230 Hardware`, `Battery Reading`, `DPI Measurement`): yes or no for battery and scaling, plus the Linux node layout.

**Status (2026-10-19):**
- ✅ Windows toolchain: Rust 1.99.1 (MSVC), Node 24, .NET 10, WebView2, VS Build Tools 2022. (On 2026-10-21 Rust turned out to be missing and was reinstalled. Build from the VS 2022 environment; see the wiki's `Tauri Rewrite` → Build gotchas.)
- ✅ 0b: scaling works (about 15 µs per hook call, under 1% CPU).
- ⏳ 0a: in progress.
- ⏳ 0c: needs a Linux machine.

The spike tools are `Monitor\…\MouseManager.exe --hid` and `--scale <factor> <seconds>`.

## Phase 1: Tauri skeleton

- `npm create tauri-app@latest` with the **vanilla** JS template (no framework, no bundler), placed in `App/`.
- Plugins: `tauri-plugin-single-instance` (a second launch focuses the first), `tauri-plugin-autostart` (Start with Windows, or `~/.config/autostart` on Linux).
- Tray icon (Tauri's built-in tray): Open, Exit. Closing the window hides it to the tray, and Exit quits.
- `index.html` with the Pulse header and pill tabs (Buttons, Performance, Monitor, Settings). The tabs switch sections; there's no router.

**Done when:** `npm run tauri dev` shows the window and the tray icon on Windows **and** Linux, and a second launch focuses the first window.

**Status (2026-10-21):** started before Phase 0 was finished (the user's call; Phase 1 doesn't depend on the spikes).
- ✅ Windows: the window, tray (Open / Exit), close-to-tray, and a second launch focusing the first (checked even while the window was hidden). Autostart plugin registered; its toggle comes in Phase 8.
- ⏳ Linux: not tested yet (needs a Linux machine, like 0c).

## Phase 2: Themes and design tokens

All colors are CSS variables, so a theme switch is one attribute change on `<html>`.

```css
:root[data-theme="pulse"]  { --bg:#0B0F0D; --panel:#111714; --panel2:#17201B; --line:#2E3D36; --text:#E8F1EC; --muted:#8FA39A; --accent:#3DDC84; --on-accent:#062612; }
:root[data-theme="ember"]  { --bg:#0A0A0C; --panel:#131317; --panel2:#1B1B21; --line:#33333D; --text:#EDEDF0; --muted:#9A9AA6; --accent:#FF3B3B; --on-accent:#0A0A0C; }
:root[data-theme="frost"]  { --bg:#0A0D11; --panel:#111720; --panel2:#172030; --line:#2B3A4D; --text:#E6EEF6; --muted:#8DA0B5; --accent:#38BDF8; --on-accent:#03202E; }
:root[data-theme="amber"]  { --bg:#0F0D09; --panel:#18150F; --panel2:#211D15; --line:#3D362A; --text:#F3EEE4; --muted:#A89F8E; --accent:#FBBF24; --on-accent:#241A02; }
:root[data-theme="violet"] { --bg:#0D0B12; --panel:#15121D; --panel2:#1E1A29; --line:#383149; --text:#EEEAF6; --muted:#A096B5; --accent:#A78BFA; --on-accent:#1A0F3A; }
```

- `styles.css` holds the rest:
  - Space Grotesk and JetBrains Mono, **bundled** as woff2 files (no Google Fonts request; both look the same in WebView2 and WebKitGTK)
  - radii (24 / 18 / 12 / pill)
  - the card, pill, chip, button and gauge styles
- The theme is saved in `config.theme`. Rust passes it in the window's init script, so `data-theme` is set before first paint and there's no flash of the wrong color.
- Icons and gauges use `currentColor` and `var(--accent)` only.

**Done when:** all 5 themes recolor everything, muted text has a contrast ratio of at least 4.5:1 on `--panel` in every theme, and Windows and Linux look the same side by side.

**Status (2026-10-21):**
- ✅ Windows: all 5 themes recolor from first paint, and the Settings theme picker saves to `config.json` and survives a restart.
- ✅ `cargo test` checks the CSS themes against the Rust list, and contrast ≥ 4.5:1 for muted on `--panel` and `--panel2`, text on `--panel`, and on-accent on accent. The lowest is 5.6:1 (ember on-accent).
- ⏳ Linux side-by-side: not tested yet.
- `config.rs` started early, holding only `theme` with load/save/defaults. Phase 3 adds the other keys.

## Phase 3: Shared Rust core

- `config.rs`: load and save, with defaults for missing keys.
- `action.rs`: the mapping parser, a port of `ButtonAction.cs`. It returns an enum: `PassThrough | Disabled | Mouse(Btn) | Keys(Vec<Key>) | Run(String)`. Mouse button names are checked before key names (`Back` is a mouse button, not Backspace), and bad text returns `Err("X2: unknown key 'Nope'")`.
- `event.rs`: the event types (serde, sent to the UI).
- `platform/mod.rs`: the interface above. Leave a stub for whichever OS isn't implemented yet.
- `#[cfg(test)]` tests: every check from `SelfTest()` in `../Monitor/Program.cs` (presets, modifiers, F-keys, aliases, the rejected strings), plus a config round-trip.

**Done when:** `cargo test` passes on both systems.

**Status (2026-10-21):**
- ✅ Windows: `cargo test` passes all 11 tests, with no warnings.
- ⏳ Linux: not run yet.
- Not ported, because they don't belong to the shared core:
  - the `INPUT` struct size and the raw-input decode checks → Phase 4
  - the DPI-from-counts check → Phase 6, in JS
- `Key` is an OS-neutral enum, so the Phase 4 and Phase 9 key maps must be exhaustive `match`es. The parser also accepts the old WinForms `Keys` names (`Return`, `Prior`, `OemMinus`, …).
- `platform/stub.rs` serves every OS for now. Phase 4 swaps in `windows.rs`.
- Events go to the UI on one channel, tagged `{"type": "button", …}`.

## Phase 4: Windows backend

Port `Remapper.cs`, `Native.cs` and `RawInputLogger.cs` to `platform/windows.rs` using the `windows` crate.
- One thread owns the window, the hook and the message loop. The hook callback is an `extern "system" fn`, and its state lives in a `thread_local!`.
- Commands from `Handle` arrive through a channel, and `PostMessage(WM_APP)` wakes the thread to read them.
- Movement is summed and sent every 50 ms. Scaling is implemented here if 0b said yes.
- If the hook disappears (Windows removed it after a timeout), re-install it and send an `error` event.

**Done when:** the event stream matches `../Monitor`'s `--log` output, X2 → `Ctrl+Shift+T` works, and clicks from a second mouse are never changed.

## Phase 5: Buttons screen

- The mouse diagram as inline SVG. The selected side button is highlighted with `--accent`.
- Rows for X2 (Forward) and X1 (Back): a chip button and a `<select>` of presets plus "Custom…", which opens a text field.
- Save calls `invoke('apply', {config})`. An error is shown on that row and the old mapping stays. Success shows a short "Saved".
- Mapping text is parsed only in Rust. Don't write a second parser in JS.

**Done when:** valid mappings work right away, `Ctrl+Nope` shows an error and changes nothing, and other mice are unaffected.

## Phase 6: Performance screen (DPI)

- **Measuring** (in JS, from `move` events):
  - End a slide after 400 ms without movement, then DPI = counts ÷ (cm ÷ 2.54).
  - Ignore slides under 30% of the expected count for the last known DPI. That stops the "~11 DPI" nudge.
  - Keep the last 5 slides, and flag a jump of more than 15%.
- **Dial:** effective DPI = target, or the measured value when the target is Native.
- **Stages** (Native / 400 / 800 / 1200 / 1600) call `set_scale(target / measured)`.
  - On Linux they're always available. On Windows, only if 0b said yes; otherwise hide them and show "DPI is set by the button behind the wheel".

**Done when:** two 10 cm slides agree within 3%, and the 800 stage moves the pointer about 56% as far as Native.

## Phase 7: Battery

Only if 0a said yes:
- Poll every 60 s, and send `battery` only when the value changes. Show `null` ("—") while the mouse is asleep or disconnected, never a stale number.
- The battery ring is in the header and on a Buttons card.
- Below 15%, show one notification (if `lowBatteryWarning` is on).

If 0a said no, remove the battery UI completely. Don't show a fake value.

**Done when:** the shown % matches the value from the 0a experiment, and unplugging the receiver shows "—".

## Phase 8: Monitor and Settings screens

- **Monitor:**
  - The event list, capped at 1000 lines and redrawn at most every 100 ms.
  - "Show all devices" (`set_all_devices`).
  - The settings report hex.
- **Settings** (as on the canvas):
  - Theme cards (`radiogroup`) with a live preview.
  - Start with the OS, start minimized, low-battery warning.
  - The device list as VID:PID.
- Capture the Bluetooth and wired IDs on both systems and add them to the default `devices`.

**Done when:** settings survive a restart, and all connection modes are remapped once their IDs are added.

## Phase 9: Linux backend

`platform/linux.rs` with the `evdev` crate (uinput support included) and `hidapi` (hidraw backend).
- Scan `/dev/input/event*` every 2 s for matching devices, which also handles hotplug and the receiver waking up. Grab the mouse node from 0c.
- Create the virtual device: `REL_X/Y`, `REL_WHEEL`, `REL_HWHEEL` (and their hi-res versions if the N230 sends them), `BTN_LEFT` to `BTN_EXTRA`, plus every key the parser can produce.
- Release the grab when the app exits **or crashes**. The grab ends automatically when the file is closed, so never keep the device open in a child process.
- **Permissions:** this udev rule gives the logged-in user access, with no `input` group needed:

  ```
  # /etc/udev/rules.d/70-n230-control.rules
  SUBSYSTEM=="input", KERNEL=="event*", ATTRS{idVendor}=="3554", ATTRS{idProduct}=="fc01", TAG+="uaccess"
  SUBSYSTEM=="hidraw", ATTRS{idVendor}=="3554", ATTRS{idProduct}=="fc01", TAG+="uaccess"
  KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"
  ```

- If opening the device fails with "permission denied", send `device {permission: false}`. The UI then shows a "Grant access" button that installs the rule with `pkexec` and reloads udev.
- Tray: KDE, XFCE and Cinnamon show it as normal. GNOME needs the AppIndicator extension, so mention that in the README.

**Done when:** on X11 **and** Wayland, X1/X2 remapping, DPI scaling and the monitor work; a second mouse is unaffected; and killing the app with `kill -9` gives the normal mouse back right away.

## Phase 10: Package and release

- **Windows:** `npm run tauri build` → NSIS installer. Unsigned builds get a SmartScreen warning on first run.
- **Linux:** `.deb` and `.rpm`, which ship the udev rule (Tauri bundle `files`) and run `udevadm control --reload && udevadm trigger` after install; plus an AppImage, which uses the in-app "Grant access" flow.
- Build each OS on that OS. Optional: GitHub Actions with `tauri-action` on a Windows + Ubuntu matrix.
- Retire `../Monitor` once the Rust version has passed the checklist below on both systems.

**Done when:** a clean user account on each OS can install, use and uninstall the app, and uninstalling removes the autostart entry (and the udev rule for the deb/rpm).

---

## Final checklist (run on Windows and Linux)

- [ ] Other mice are never remapped
- [ ] Every button-down has its button-up (hold a remapped X1, then release)
- [ ] A bad mapping shows an error and the old mapping keeps working
- [ ] A crash or forced kill leaves the mouse working normally
- [ ] All 5 themes are readable, with no color flash on start
- [ ] Start with the OS works after a reboot
