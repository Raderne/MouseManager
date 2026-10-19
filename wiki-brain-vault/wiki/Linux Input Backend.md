# Linux Input Backend

Planned for `App/src-tauri/src/platform/linux.rs` (Phase 9 of the [[Tauri Rewrite]]). Not built yet.

## Approach: grab and pass through

1. Find the `/dev/input/event*` node with vendor:product `3554:FC01` that has `REL_X` + `BTN_SIDE` (spike 0c confirms which node).
2. `EVIOCGRAB` it, so only this app receives its events.
3. Create a **uinput** virtual device: `REL_X/Y`, `REL_WHEEL`, `REL_HWHEEL` (plus their hi-res versions if sent), `BTN_LEFT`…`BTN_EXTRA`, plus every key the mapping parser can produce.
4. Pass every event through, except `BTN_SIDE` (X1) and `BTN_EXTRA` (X2), which run their mapping. Software DPI scaling is just multiplying `REL_X`/`REL_Y` ([[DPI Measurement]]).

Simpler than Windows: grabbing one device means other mice are never touched, without any device-matching tricks ([[Windows Input Backend]]). It runs below the compositor, so it works on **X11 and Wayland**.

## Rules

- Scan for the device every 2 s (this handles hotplug and the receiver waking up).
- The grab ends when the file is closed, so a crash or `kill -9` gives the mouse back. **Never** keep the device open in a child process.
- Crates: `evdev` (includes uinput), `hidapi` with the hidraw backend for report 6 ([[Battery Reading]]).

## Permissions

```
# /etc/udev/rules.d/70-n230-control.rules
SUBSYSTEM=="input", KERNEL=="event*", ATTRS{idVendor}=="3554", ATTRS{idProduct}=="fc01", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="3554", ATTRS{idProduct}=="fc01", TAG+="uaccess"
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"
```

- `uaccess` gives the logged-in user access; no `input` group is needed.
- The `.deb` and `.rpm` packages install the rule automatically. An AppImage uses an in-app "Grant access" button that installs it with `pkexec`.
- When opening the device fails with "permission denied", the backend sends `device {permission: false}`.

## Desktop caveats

- Tray icon: KDE, XFCE and Cinnamon show it normally; GNOME needs the AppIndicator extension.
- The UI runs in WebKitGTK, not WebView2. The fonts are bundled so it looks the same ([[Design System]]).
- Testing needs real hardware (a Linux machine or live USB). WSL2 doesn't see USB input devices.

Related: [[N230 Hardware]], [[Stack Decision]].
