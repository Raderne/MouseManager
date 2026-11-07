# N230 Control: Bluetooth connection plan (phases B0–B9)

Written 2026-11-07. A plan, not a measurement: every hardware claim below is an assumption until phase B0 confirms it.

**Goal:** the manager finds the N230 over Bluetooth, pairs and connects it, recognizes it as "the configured mouse" so X1/X2 remapping, DPI and the monitor work exactly as on the 2.4G dongle, and reads battery and connection state, on **Windows and Linux**.

**Phase names:** `B0`–`B9`, so they don't collide with `App/STEPS.md` phases 0–10. Same rule as STEPS: each phase ends with a **Done when** check, and the next phase doesn't start until it passes.

**Where they fit in STEPS:**
- B0–B6 (Windows parts) can start now: they only need Phases 3–8, which are built.
- The Linux parts of B2–B7 need Phase 9 (Linux backend), which is blocked on Linux hardware.
- B8 and B9 must finish **before Phase 10** (packaging), since the installer ships the new udev rules and the retire-`Monitor/` checklist gains Bluetooth items.
- Phase 8's open item "capture the Bluetooth IDs and add them to the default devices" is covered by B0 + B3.

---

## Key idea: two layers, keep them apart

```
          ┌────────────────────────── connection layer (NEW) ───────────────────────────┐
UI ──►    │ scan · pair · connect · forget · link state · RSSI · BT battery (BAS 0x180F) │
          │ Windows: WinRT Windows.Devices.Bluetooth / Enumeration                       │
          │ Linux:   BlueZ over D-Bus (org.bluez.Adapter1 / Device1 / Battery1)          │
          └──────────────────────────────────────────────────────────────────────────────┘
                     │ once connected, the OS HID stack owns the mouse
                     ▼
          ┌────────────────────────── input layer (EXISTS) ─────────────────────────────┐
          │ Windows: WH_MOUSE_LL + Raw Input   (matches_device already parses BT paths)  │
          │ Linux:   evdev grab + uinput        (needs BT-aware matching + udev rules)   │
          └──────────────────────────────────────────────────────────────────────────────┘
```

- **The app never talks HID-over-GATT itself.** After pairing, Windows (`HidBth` / `HidBthLE`) and Linux (`hidp` / `uhid` via BlueZ `hog`) create a normal HID mouse. Remapping keeps using the existing backends; Bluetooth only changes **how the device is found and identified**.
- Windows blocks apps from the HID GATT service (`0x1812`) anyway, so "talk to the mouse directly over BLE" is not an option for input. It is only used for the standard **Battery Service** and **Device Information Service**.
- The connection layer is a **separate module** (`bt/`), not part of `platform/{windows,linux}.rs`. Hard rule kept: Raw Input is still registered once per process, in the remapper only.

---

## Phase B0: Bluetooth spike (does the N230 do Bluetooth, and how?)

Throwaway, about half a day. Nothing in the app changes.

1. **Confirm the mode exists.** Find the mode switch / button combo on the N230 (check the underside, the manual, or a long press of the DPI button). Write down how to enter pairing mode and how the LED shows it.
2. **Classic or Low Energy?**
   - Windows: pair through Settings → Bluetooth. In Device Manager, the HID device's parent is either `{00001124-…}` (**Classic HID**, `HidBth`) or `{00001812-…}` (**BLE HID-over-GATT**, `HidBthLE`).
   - Linux: `bluetoothctl` → `scan on` → `info <addr>`; the UUID list shows `Human Interface Device (00001124)` (Classic) or `00001812` (BLE).
3. **Capture the IDs** in each OS:
   - Windows: `MouseManager.exe --log 60` and Monitor → "Show all devices": the Raw Input device path, e.g. `HID\{00001812-…}_Dev_VID&02xxxx_PID&yyyy_REV&…`. Note the **vendor-ID source** prefix (`01` = Bluetooth SIG, `02` = USB-IF): the VID may differ from `3554`.
   - Linux: `cat /sys/class/input/event*/device/id/{bustype,vendor,product}` (bustype `0005` = Bluetooth), `udevadm info -a /dev/input/eventN`, `ls /sys/bus/hid/devices/` (names like `0005:VVVV:PPPP.000N`).
   - The Bluetooth **address** and the **advertised name** (e.g. "Lenovo N230" or something else).
4. **Dump the HID side** over Bluetooth: `MouseManager.exe --hid` on Windows, `/sys/bus/hid/devices/*/report_descriptor` on Linux.
   - Same 6 collections as the dongle, or fewer? Does **feature report 6** still answer?
   - Do X1/X2 still arrive as buttons 4/5 (`BTN_SIDE` / `BTN_EXTRA`)?
5. **List GATT services** (BLE only): with nRF Connect on a phone or `bluetoothctl` → `menu gatt` → `list-attributes`. Look for **Battery Service `0x180F`** (Battery Level `0x2A19`), **Device Information `0x180A`** (PnP ID `0x2A50` holds VID/PID + source), and any vendor service.
6. **Sleep behaviour:** how long until the mouse drops the link when idle, and how it reconnects (first click, any move?). Time it.
7. **Rate:** with the existing DPI meter, measure counts per 10 cm over Bluetooth. Compare with the 1424 DPI on the dongle.

**Done when:** `wiki/N230 Hardware.md` has a "Bluetooth" section with: Classic vs BLE, VID/PID + VID source, bus type, address format, name, HID collections, which reports answer, the GATT service list, sleep timeout, and the measured DPI. If the N230 has **no** Bluetooth mode, stop here and record that; the rest of this plan is dropped.

---

## Phase B1: Config and device identity

Extend the shared Rust core so one config describes the dongle and the Bluetooth link of the same mouse.

- `config.rs`: each device gets an optional `bus` and an optional `address`:

  ```json
  { "devices": [
      { "vid": "3554", "pid": "FC01", "bus": "usb" },
      { "vid": "xxxx", "pid": "yyyy", "bus": "bluetooth", "address": "AA:BB:CC:DD:EE:FF", "name": "N230" }
  ] }
  ```

  - `bus` missing = "any bus" (old configs keep working, serde defaults).
  - **Matching stays VID/PID (+ bus)**, per the hard rule. `address` is only used by the connection layer to reconnect/forget the *same* physical mouse, never to match input.
  - `name` is display only.
- A new `DeviceId` type with one parser per OS source, all in Rust:
  - Windows Raw Input paths (already in `windows.rs::matches_device`; move the parsing into a shared, tested function).
  - Linux `bustype:vendor:product` from sysfs / the `0005:VVVV:PPPP.N` HID name.
  - The BLE PnP ID characteristic (`0x2A50`: source, VID, PID, version, little-endian).
- `event.rs`: `device {connected, permission}` grows to `device {bus, connected, permission, address?, rssi?}`. A new `link {state: "searching"|"pairing"|"connected"|"sleeping"|"disconnected", address}` event for the connection screen.
- Add the B0 IDs to the **default** `devices`.

**Done when:** `cargo test` covers: old config without `bus` loads; a BT path, a sysfs triple and a PnP ID blob all parse to the same `DeviceId`; a different PID or a USB-only entry doesn't match a BT device.

---

## Phase B2: Input matching over Bluetooth (remap works once paired)

No scanning yet. Pair by hand in the OS, and make the **existing** remapper handle the Bluetooth mouse.

**Windows** (`platform/windows.rs`):
- `matches_device` already accepts both BT path forms; switch it to the shared `DeviceId` parser and the `bus` field.
- Register Raw Input with **`RIDEV_DEVNOTIFY`** and handle **`WM_INPUT_DEVICE_CHANGE`**: a BT mouse that sleeps and reconnects gets a **new `HANDLE`**, so the cached "is this the N230?" decision per handle must be refreshed on arrival/removal. Never compare handles across a reconnect.
- On removal, if a button is held, synthesize nothing: let the pending button-up pass through untouched (same treatment as its down; see the hard rule).

**Linux** (`platform/linux.rs`, needs Phase 9):
- Match on `input_id {bustype, vendor, product}`, accepting `BUS_BLUETOOTH` (0x05) when the config says `bluetooth` or no bus.
- **Skip our own uinput device** when scanning (its name/phys), so we never grab ourselves.
- The 2 s scan also covers reconnects; on `ENODEV` drop the fd at once (this also ends the grab).
- **udev rules:** `ATTRS{idVendor}` only exists on USB parents, so the current rule never matches a Bluetooth mouse. Add:

  ```
  # Bluetooth N230 (fill in VVVV/PPPP from B0)
  SUBSYSTEM=="input", KERNEL=="event*", ATTRS{id/bustype}=="0005", ATTRS{id/vendor}=="vvvv", ATTRS{id/product}=="pppp", TAG+="uaccess"
  SUBSYSTEM=="hidraw", KERNELS=="0005:VVVV:PPPP.*", TAG+="uaccess"
  ```

**Done when:** with the mouse paired in OS settings, X2 → `Ctrl+Shift+T` works over Bluetooth on Windows (and Linux once Phase 9 exists); a second mouse is unaffected; letting the mouse sleep and waking it keeps the remap working with no restart; holding X1 across a disconnect never leaves a stuck button.

---

## Phase B3: Discovery ("Search for the mouse")

New module `src-tauri/src/bt/` with the same pattern as `platform/`: `bt/windows.rs`, `bt/linux.rs`, one interface picked by `#[cfg]`, and a `bt/stub.rs` for tests.

```rust
pub fn start(events: Sender<Event>) -> BtHandle;
impl BtHandle {
    pub fn adapter(&self) -> AdapterState;                       // present / off / missing
    pub fn scan(&self, on: bool);                                // emits `found` events
    pub fn pair(&self, address: &str) -> Result<(), String>;     // B4
    pub fn connect(&self, address: &str) -> Result<(), String>;  // B4
    pub fn forget(&self, address: &str) -> Result<(), String>;   // B4
}
```

- **Windows:** WinRT through the `windows` crate (the full one; `windows-sys` has no WinRT), features `Devices_Bluetooth`, `Devices_Enumeration`, `Devices_Bluetooth_Advertisement`, `Foundation`.
  - BLE: `BluetoothLEAdvertisementWatcher` (active scan) for name, RSSI, service UUIDs, manufacturer data.
  - Classic: `DeviceWatcher` with `BluetoothDevice::GetDeviceSelectorFromPairingState(false)`.
  - Already-paired devices: `GetDeviceSelectorFromPairingState(true)` for the "known" list.
  - Run WinRT on its own thread (MTA); never on the hook thread.
- **Linux:** BlueZ over D-Bus with the `bluer` crate (tokio).
  - `Adapter1.SetDiscoveryFilter({Transport: "auto"})` + `StartDiscovery`; listen for `InterfacesAdded` / `PropertiesChanged` (`RSSI`, `Name`, `UUIDs`).
  - Build dep: `libdbus-1-dev` (add to the STEPS Phase 0 toolchain list).
- **Filtering** (in Rust): show only devices that look like a mouse (Appearance `0x03C2`, Class-of-Device "Peripheral / pointing", or HID UUID `1124`/`1812`). Mark a device as "N230" when its name or PnP ID matches B0's capture. A "Show all" toggle shows the rest.
- Scan stops after 30 s or when the user leaves the screen (it costs radio time and battery on laptops).
- Adapter states: no adapter → show "No Bluetooth adapter"; adapter off → "Bluetooth is off" with a button that opens the OS Bluetooth settings (`ms-settings:bluetooth`, or `gnome-control-center bluetooth` / `bluedevil`), **never** toggle the radio silently.

**Done when:** with the N230 in pairing mode, "Search" lists it within 10 s marked as N230, with RSSI updating; other nearby devices only appear with "Show all"; no adapter / adapter off show the right message instead of an error.

---

## Phase B4: Pair, connect, forget

- **Windows:** `DeviceInformation::CreateFromIdAsync(id)` → `Pairing().Custom()` with `DevicePairingKinds::ConfirmOnly | DisplayPin | ProvidePin`, answering `ConfirmOnly` automatically (mice use "Just Works"). Any PIN request is shown to the user in the UI, never auto-filled.
  - Connect: for HID the OS connects by itself after pairing; the app only waits for the Raw Input arrival from B2 (timeout 15 s).
  - Forget: `DeviceInformation.Pairing().UnpairAsync()`.
- **Linux:** `Device1.Pair()` with a registered BlueZ **agent** (`NoInputNoOutput`, or `KeyboardDisplay` that forwards a passkey to the UI), then `Trusted = true` (so it reconnects by itself after sleep / reboot), then `Connect()`. Forget: `Adapter1.RemoveDevice()`.
- On success, write the device into `config.devices` (VID/PID from PnP ID or from the input arrival, plus `bus` and `address`), through the same Rust save path as `set_devices`.
- **Errors** become plain messages: "Mouse not in pairing mode", "Pairing was rejected", "Timed out: move the mouse to wake it". Never leave a half-paired entry: on failure, `forget` what was created.
- Pairing, unpairing and trusting are **only** started by a click in the app, never automatically.

**Done when:** from a forgotten state, Search → Pair → the remap from B2 works with no trip to OS settings, on Windows (and Linux after Phase 9); Forget removes it from both the app and the OS list; a refused or timed-out pairing leaves nothing behind.

---

## Phase B5: Link state and reconnection

- One **link state machine** per configured Bluetooth device, in Rust: `disconnected → connecting → connected ⇄ sleeping`, fed by:
  - Windows: `BluetoothLEDevice.ConnectionStatusChanged` / `BluetoothDevice.ConnectionStatusChanged`, plus the Raw Input arrival/removal from B2.
  - Linux: `Device1.Connected` property changes, plus the evdev node appearing/disappearing.
- "Sleeping" = the OS reports disconnected but the device is paired and was connected less than the B0 sleep timeout ago. The UI shows "Asleep, click to wake", not an error.
- Emit `link` on every change and `device {connected}` as before, so the existing header dot keeps working.
- **Dongle and Bluetooth at once:** if both are present, both are remapped (both are configured). Show which one is active (the last one that sent input).
- Optional auto-reconnect button: Linux `Device1.Connect()`; Windows has no direct "connect HID" call, so show "Move the mouse to reconnect".

**Done when:** sleep → wake, switching mode dongle ↔ Bluetooth on the mouse, turning the PC's Bluetooth off/on and a reboot each end with the right state shown within 5 s and the remap working again, with no app restart.

---

## Phase B6: Info over Bluetooth (battery, firmware, signal)

Fills the gap that [[Battery Reading]] has on the dongle: over BLE there is usually a **standard** battery.

Order of sources for `battery {percent}` (first that works wins; record which in the wiki):
1. **Linux kernel / BlueZ:** `org.bluez.Battery1.Percentage` on the device object, or `/sys/class/power_supply/hid-<addr>-battery/capacity`.
2. **Windows device property:** `DEVPKEY_Bluetooth_Battery` (`{104EA319-6EE2-4701-BD47-8DDBF425BBE5}`, pid 2) read with `CM_Get_DevNode_PropertyW` on the BT device node; this is what Windows Settings shows. No GATT access needed.
3. **GATT Battery Service** `0x180F` / `0x2A19` (BLE only): read once, then **subscribe to notifications** instead of polling. On Windows use `GattDeviceService` with `BluetoothCacheMode::Uncached` for the first read.
4. **HID**: report 6 byte 5 (if B0 showed it still answers over Bluetooth), same plumbing as Phase 7.

Also:
- **Device Information `0x180A`:** manufacturer, model, firmware revision → shown in Settings → Devices. Read once per connection.
- **RSSI** (BLE advertisements / `Device1.RSSI`): shown only during Search; not polled while connected (Windows doesn't give live RSSI for a connected link).
- Rules from Phase 7 still hold: "—" while asleep or disconnected, never a stale value; send `battery` only on change; one notification below 15%.

**Done when:** the shown % matches what Windows Settings / `upower -d` shows for the mouse, updates when it changes, and shows "—" while the mouse is asleep. If no source works, the battery UI stays "—" (no fake value) and the wiki says why.

---

## Phase B7: UI ("Connect" flow)

- **Settings → Devices** becomes a list of cards, one per configured device: icon for dongle / Bluetooth, name, link state, battery, firmware, "Forget" (Bluetooth only), "Remove from app".
- **"Add Bluetooth mouse"** opens a small flow:
  1. "Turn on pairing mode on the N230" with the B0 instructions (LED pattern).
  2. Live search list (N230 on top, "Show all" toggle, RSSI bars).
  3. Pairing progress, PIN box only if asked.
  4. "Done: try a side button" with a live X1/X2 indicator from `button` events.
- Header: a small dongle / Bluetooth glyph next to the battery ring showing the active link.
- Monitor tab: tag each line with the bus (`usb` / `bt`).
- All colors from the theme variables; no new color tokens (an error uses `--text` + accent ✕, as on the Buttons screen).
- JS only shows state and sends clicks (`bt_scan`, `bt_pair`, `bt_forget`); all parsing, matching and config writes stay in Rust.

**Done when:** a first-time user can add the N230 over Bluetooth from the app alone, in all 5 themes; the flow survives cancel at every step; keyboard and screen-reader labels work (checked through UI Automation on Windows).

---

## Phase B8: Permissions and platform edges

- **Windows:**
  - A packaged (NSIS) desktop app can use WinRT Bluetooth without a manifest capability; check this on a clean account (B9).
  - Bluetooth privacy setting ("Let apps access Bluetooth"/radio access): if `Radio.RequestAccessAsync` says denied, show where to enable it.
- **Linux:**
  - BlueZ discovery/pairing for the logged-in user works through the default D-Bus policy (`at_console` / polkit); confirm on GNOME and KDE.
  - The new udev lines from B2 go into `70-n230-control.rules` and the in-app "Grant access" (`pkexec`) flow.
  - Our BlueZ agent must not fight the desktop's agent: register it only during the in-app pairing, unregister after.
  - Wayland and X11 both fine (input path unchanged).
- **No adapter, USB BT dongle hotplug, adapter reset** (`rfkill block/unblock bluetooth`) → `AdapterState` events, never a crash.

**Done when:** on a clean user account per OS, the whole B7 flow works without admin rights (except the one-time udev install on Linux), and every adapter edge case shows a message.

---

## Phase B9: Tests and checklist

- `cargo test` (no framework, per the hard rule):
  - `DeviceId` parsing (Windows paths, sysfs triples, PnP ID blobs, bad input).
  - Link state machine transitions from a scripted event list (no radio needed; uses `bt/stub.rs`).
  - Discovery filter (mouse vs not, N230 vs other mouse).
  - Config round-trip with `bus` / `address`, and an old config without them.
- `npm run check`: any pure JS helper (e.g. RSSI → bars).
- Add to the **Final checklist** in `App/STEPS.md`:
  - [ ] Bluetooth: Search → Pair → remap works with no OS settings trip
  - [ ] Bluetooth: sleep/wake and mode switch keep the remap working, no stuck buttons
  - [ ] Bluetooth: a second Bluetooth mouse is never remapped
  - [ ] Bluetooth: battery shows a real % or "—", never a guess
  - [ ] Bluetooth: Forget leaves nothing in the app or the OS list

**Done when:** all tests pass on both systems and the checklist items pass by hand on Windows and Linux. Then Phase 10 packaging can go ahead.

---

## Dependencies to add (when the phase needs them, not before)

| Phase | Windows | Linux |
|---|---|---|
| B3–B6 | `windows` crate (WinRT): `Devices_Bluetooth`, `Devices_Bluetooth_Advertisement`, `Devices_Bluetooth_GenericAttributeProfile`, `Devices_Enumeration`, `Devices_Radios`, `Foundation` | `bluer` (+ `tokio`), system `libdbus-1-dev` |
| B6 | `windows-sys` `Win32_Devices_DeviceAndDriverInstallation` (for `CM_Get_DevNode_PropertyW`) | — |

## Open questions (B0 answers these)

1. Does the N230 have a Bluetooth mode at all, and is it Classic or BLE?
2. Are the Bluetooth VID/PID the same as the dongle (`3554:FC01`) or different?
3. Does feature report 6 exist over Bluetooth?
4. Does the mouse expose the standard Battery Service?
5. What is the idle sleep timeout, and what wakes it?
6. Is the DPI the same over Bluetooth?

## Risks

- **No BT mode on this model**: the plan stops at B0. Cheap to find out first.
- **Windows HID-over-GATT lock:** apps can't read `0x1812`; that's fine, the plan doesn't need it. If the Battery Service is also blocked while connected, B6 falls back to `DEVPKEY_Bluetooth_Battery`.
- **Linux phases depend on Phase 9**, which is blocked on hardware.
- **Handle churn on Windows** after each sleep is the most likely bug source (B2): test it early.
