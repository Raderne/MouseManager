# Bluetooth Connection

Full plan: `raw/bluetooth-phases-plan.md` (phases **B0–B9**, named so they don't collide with `App/STEPS.md` 0–10).

## Status

- **2026-11-07, B0 done (Windows):** the N230 is **Bluetooth LE**, VID/PID **`25A7:FAA2`** (USB-IF source), slots BT1/BT2, standard Battery Service, X1/X2 arrive normally over Raw Input. Details in [[N230 Hardware]] → Bluetooth. Not measured yet, moved to the phases that need them: the **sleep timeout** (B5) and **DPI over Bluetooth** (re-run the Performance meter before B6/B7).
- **2026-11-07, B1 done (Windows, `cargo test` 24 pass):**
  - New `src-tauri/src/device.rs`: `Bus {Usb, Bluetooth}` and `DeviceId {bus, vid, pid}`, with the only parsers for Windows paths, Linux sysfs ids / HID names (`0005:25A7:FAA2.N`), and the BLE PnP ID (`2A50`). `BUS_VIRTUAL` (our uinput device) never parses, so Linux can't grab itself.
  - `config::Device` gained optional `bus`, `address`, `name` (left out of the file when unset). `Device::matches(&DeviceId)` = VID + PID + bus if set; address never matters; a bad VID/PID never matches.
  - Default devices: `3554:FC01` usb ("N230 receiver") + `25A7:FAA2` bluetooth ("N230 Bluetooth"). A saved list that is **exactly** the old default (`[{3554, FC01}]`, no bus) is upgraded on load; any edited list is left alone.
  - `windows.rs::matches_device` now just calls the shared parser.
  - Events: `device` gained `bus`; new `link {bus, state: searching|pairing|connected|sleeping|disconnected, address}`.
  - Settings device list keeps `bus`/`address`/`name` when a row is edited (before, saving dropped them) and shows the name as a muted tag.
  - Linux `cargo test` not run (no Linux machine, as for Phase 9).
- **2026-11-07, B2 done on Windows** (`cargo test` 25 pass; hands-on by the user with X1 = Ctrl+C, X2 = Ctrl+V): copy/paste from the BT mouse works; after switching the mouse off and on it keeps working with no restart; holding X1 while switching off left no stuck Ctrl. The `--log` showed `device {bluetooth, connected:false}` about 9 s after X1-down with no X1-up, i.e. the release path ran. Reconnect after power-on took about 10–16 s. Linux half waits for Phase 9.
  - Raw Input is registered with `RIDEV_INPUTSINK | RIDEV_DEVNOTIFY`. On `WM_INPUT_DEVICE_CHANGE` the handle's cached "is it the N230?" entry is dropped (arrival re-resolves it), because Windows may **reuse an old handle for another mouse**.
  - A configured device's arrival/removal sends `device {bus, connected}`.
  - **Stuck-button fix:** each held remap remembers the device handle that pressed it (`held_dev`). If that device is removed while holding, the action's release is queued (`release_held`), so e.g. a held `Ctrl` can't stay down when the mouse drops the link.
  - **Startup race found and fixed:** Windows sends an arrival for every present mouse right after registration, *before* the first `Apply` reaches the input thread, so the device list was empty and the BT mouse's `connected` event was lost (clicks were fine: `Apply` clears the cache). `setup()` now gets the device list directly.
  - Sleep: with the mouse idle 15:30:37 → 15:39:32 the BLE link stayed **connected**, so the sleep timeout is **over 9 minutes** (one unexplained disconnect/reconnect at 15:30:09–15:30:37).

- **2026-11-07, B3 done on Windows** (`cargo test` 30 pass). Hands-on by the user: with the mouse in pairing mode, Search listed the N230 at the top with its badge; Stop and a new Search respond at once. Linux half waits for Phase 9. **Next: B4** (pair / connect / forget from the app).
  - `src-tauri/src/bt/`: `mod.rs` (interface, `Scan` merge + classification, tests), `windows.rs` (WinRT), `stub.rs` (other OSes). Uses the full **`windows` 0.61** crate (`Devices_Bluetooth`, `_Advertisement`, `Devices_Radios`, `Foundation`, `Foundation_Collections`, `Storage_Streams`).
  - LE only (B0: the N230 has no Classic side). `BluetoothLEAdvertisementWatcher` in **Active** mode, since the name only comes in the scan response. Adverts for one address are merged: name, appearance and UUIDs stick once seen.
  - Classification in Rust: mouse = appearance `0x03C2`, or HID `1812` with no appearance yet; a HID keyboard (`0x03C1`) is not a mouse. N230 = name contains "N230". `bars` 0–4 from RSSI (−60/−70/−80/−90). An update is sent only for a new device, name/kind change or a bar change (not RSSI jitter).
  - Adapter: `BluetoothAdapter::GetDefaultAsync` + `IsLowEnergySupported` + `GetRadioAsync().State()` → `on|off|missing`. Off → "Open Bluetooth settings" (`explorer ms-settings:bluetooth`); the app never switches the radio itself.
  - WinRT calls run on a fresh thread and are joined (blocking `.get()` must not run on the UI's STA thread). Search stops itself after `SCAN_SECS` = 30 s; a generation counter stops an old timer from ending a newer search.
  - Commands `bt_adapter`, `bt_scan(on)`, `bt_open_settings`. Events `btAdapter {state}`, `btScan {active}`, `btFound {address, name, rssi, bars, mouse, n230}`. (Changed from the plan: `btScan` instead of `link {searching}`; `link` stays per device.)
  - Settings → **Bluetooth** card: adapter line, Search/Stop, "Show all devices", result list (N230 first with an accent badge and border, then by RSSI; signal bars).
  - `n230-control.exe --scan [secs]` (debug builds): one search printed to stdout; with `secs` it's stopped by hand after that long (the Stop button's path), otherwise by the 30 s auto-stop.
  - **Bug found by the user, fixed:** the Stop button did nothing. The "search ended" event came only from the watcher's WinRT `Stopped` event, but we drop our last reference to the watcher right after `Stop()`, and then Windows **sometimes never raises `Stopped`**. The UI stayed on "Stop" while Rust had no search left to stop. (Same cause as one `--scan` run that never ended.) Fix: `stop_watcher` sends `btScan {active:false}` itself; the `Stopped` handler only reports stops we didn't ask for (radio off), and only if its search is still current; `bt_scan(false)` with nothing running still sends `active:false`, so a stale button always recovers. The lock is released around `Start()` so a synchronous `Stopped` can't deadlock. Verified: 3 hand stops (end reported in ≤10 ms) + 1 auto-stop at 30.0 s.
  - Nearby non-mouse devices seen: `49:7D:07:E6:7D:BF`, `2C:92:FD:6D:3D:86`, `26:35:07:84:E7:C7` (no names).

- **2026-11-07, B4 done on Windows** (`cargo test` 31 pass). Hands-on by the user: Forget removed the N230 from the app and from Windows; Search + Pair re-paired it from the app (this time pairing in code worked, unlike the B0 attempt after the window had closed); X1/X2 remap works afterwards. Linux half waits for Phase 9.
  - `bt::pair(address)`: `BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync` (Random first, then Public) → `DeviceInformation.Pairing.Custom` with **ConfirmOnly** only, `ProtectionLevel::Default`; the `PairingRequested` handler accepts ConfirmOnly and nothing else, so a PIN request fails with "pair it in Windows settings" instead of being answered for the user. Any search is stopped first. On any failure a half-made pairing is undone (`UnpairAsync`). Then the **PnP ID** (`180A`/`2A50`) is read to get the link's VID/PID.
  - `wait()` polls `IAsyncOperation.Status()` with a timeout and `Cancel()`s on expiry (pairing 30 s, lookups 10 s) instead of `.get()`, so a mouse that leaves pairing mode can't hang the app.
  - `bt::forget(address)`: `UnpairAsync`; `bt::paired()`: `GetDeviceSelectorFromPairingState(true)` + `FindAllAsyncAqsFilter`, kept if appearance `0x03C2` or the name has "N230"; reports `connected`.
  - Config: `remember_bluetooth(id, address, name)` sets bus/address/name on the entry matching the PnP VID/PID (adds one if none; a USB-only entry isn't reused). `forget_bluetooth(address)` clears only the address, so the VID/PID entry still remaps a later pairing.
  - Commands `bt_pair`, `bt_forget`, `bt_paired` are `async` and use `spawn_blocking` (the UI stays live during pairing). `bt_pair` returns the device list + a status line.
  - UI: **Pair** on every mouse in the search results; **Paired mice** list with Connected state and a two-click **Forget**. After pairing, the next `device {bluetooth, connected}` shows "Connected. Try a side button."
  - New dependency **`windows-future` 0.2**: since `windows` 0.61, `IAsyncOperation`/`AsyncStatus` live there and aren't re-exported.
  - `n230-control.exe --paired` lists paired mice (verified: `FE:46:E9:24:04:91` "Lenovo Services N230 BT1", connected).

- **2026-11-07, B5 mostly done on Windows** (`cargo test` 32 pass). Hands-on by the user:
  - Mouse off/on: `link` went connected → sleeping (37.8 s) → connected (44.9 s); remap kept working.
  - Receiver ↔ Bluetooth: link sleeping while on the receiver (76 s → 156 s). `--log` capture: BT removal at 24.8 s, `active {usb}` at 28.9 s, BT back at 40.6 s, `active {bluetooth}` at 41.9 s.
  - **UI fix after the first test:** the user never saw "using the USB receiver". The event *was* sent (capture above); the line was small muted text under the adapter status, easy to miss. Now: bold `.bt-active` line at the top of the Bluetooth card, and while the active bus is `usb` an N230 paired entry that isn't connected reads **"Not in use: the N230 is on the USB receiver"** instead of "Asleep or off". Confirmed by the user.
  - **2026-11-08, radio off/on:** `--watch` recorded `btAdapter off` (47.0 s) → `on` (66.8 s) → link `sleeping` (66.8 s) → `connected` (70.2 s). **Gotcha:** Windows raised no `ConnectionStatusChanged` while the radio was off, so the link would read "connected" for the whole off period. Fix: the radio `StateChanged` handler marks every watched mouse not connected on `off`. **Verified:** `btAdapter off` and link `sleeping` both at 47.568 s; `on` at 61.9 s; `connected` at 63.8 s.
  - **Still open:** a **reboot** test. Do it before B6.
- B5 details:
  - **The OS can't tell asleep / switched off / on another channel apart**: all are "paired, not connected". So `link_state(paired, connected, was_connected)`: connected → `connected`; paired and seen connected this session → `sleeping` (UI: "Asleep or off: move the mouse to wake it"); otherwise `disconnected`. `Links` keeps per-address state and reports only changes. No sleep-timeout timer is needed.
  - `bt::watch(addresses)`: for each configured Bluetooth address, `BluetoothLEDevice.ConnectionStatusChanged` → `link {bluetooth, state, address}`; plus the adapter `Radio.StateChanged` → `btAdapter {on|off}`. Runs on its own thread (lookups take seconds), one watch at a time; re-run at startup and after `bt_pair`, `bt_forget`, `set_devices`.
  - `active {bus}` from the input thread when N230 input switches between the receiver and Bluetooth (path parsed only when the device handle changes, so it's cheap at 1000 Hz).
  - **Address rotation confirmed:** re-pairing in B4 gave the mouse a new address (`FE:46:E9:24:04:91` → `CC:CD:72:DF:59:E5`); the config got the new one from `remember_bluetooth`. Matching never uses the address, only VID/PID.
  - CLI: `n230-control.exe --watch <address> [secs]` prints link/adapter changes.
- **2026-11-08, B6 done on Windows** (`cargo test` 33 pass). Hands-on by the user: the header shows the battery % on Bluetooth and "—" on the receiver. B5's reboot test is still open, by the user's choice:
  - **Source: the GATT Battery Service** (`180F`/`2A19`). When a watched link becomes `connected`, a thread reads Battery Level (uncached), sends `battery {percent}`, then subscribes (`WriteClientCharacteristicConfigurationDescriptorAsync(Notify)` + `ValueChanged`). Any non-connected state, radio off, or a re-watch → unsubscribe and `battery {null}` ("—", never the last number). A `battery_gen` counter drops a read that finishes after a disconnect. `battery` is sent only on change.
  - Verified: `--watch` showed `Battery 83%` 2.2 s after connect; Windows' own `DEVPKEY_Bluetooth_Battery` read **83** at the same time. The DEVPKEY path wasn't needed, so it isn't built.
  - **Low-battery warning:** `bt::LowBattery` warns once below 15% and re-arms at ≥ 20% (so 14/15/14 jitter warns once); shown with **`tauri-plugin-notification` 2** (new dependency) if `lowBatteryWarning` is on. Not tested on the real mouse (needs < 15%).
  - The header ring + % follow `battery` events. `get_battery` returns the last value, so a page that loads after the first read still shows it.
- **2026-11-08, B7 done on Windows** (`cargo test` 34 pass). Hands-on by the user: Cancel closes the flow cleanly; the 4-step flow paired BT2; X1/X2 lit up on the Try-it step; cards for the receiver, BT1 and BT2 all show:
  - **Settings → Devices** replaces the old Devices + Bluetooth groups: "The N230 is using …" line, adapter line (+ Open Bluetooth settings when off), one **card** per receiver entry, per Bluetooth entry paired from the app (with address), and per mouse Windows paired on its own ("Paired in Windows"). Cards show status, battery (receiver: "— (not reported over the receiver)"), firmware, maker, address, and two-click **Forget** / **Remove from app**. The raw VID:PID editor moved into a collapsed **Advanced** `<details>`.
  - **Add Bluetooth mouse** flow, 4 steps with a step indicator: (1) switch to BT + pairing mode (light blinks; from the manual), (2) search list with Pair, (3) pairing progress ("Hide" lets it finish in the background), (4) "Try it": live **X1/X2** keys light up on `button {bus: bluetooth}` events. Cancel/Hide/Finish on every step; Escape closes; focus moves to each step's first button.
  - **Header:** `BT`/`USB` chip next to the battery ring from `active`. **Monitor:** button lines tagged `usb`/`bt`.
  - Rust: `button` events carry `bus` (cached per device handle, so no parsing per event); new `btInfo {address, manufacturer, firmware}` from Device Information `2A29` + `2A26`, else `2A28` (read once per connection; N230: BEKEN, 5.3.5.8).
  - **`get_status`** (replaces `get_battery`): latest battery, receiver/Bluetooth presence and active link, replayed on page load. These events fire at startup before the page listens (the receiver card read "Receiver" instead of "Plugged in" until this).
  - **Bug found in B6 and fixed:** the header ring never filled. `ring.hidden = false` does nothing on an SVG `<circle>` (`.hidden` is HTML-only); now `toggleAttribute("hidden", …)`.
  - **Two Bluetooth channels:** `remember_bluetooth(id, address, name, still_paired)` reuses an entry only if it's this address, has none, or holds an address Windows no longer has paired (a re-pair of the same channel); otherwise it adds one, so pairing BT2 no longer overwrites BT1's card. `bt_pair` passes `bt.paired()` addresses.
  - Checked by capturing the app window (`PrintWindow` via a Windows PowerShell 5.1 script; PS7's Add-Type can't find `System.Drawing.Bitmap`).
- **2026-11-08, B8 (Windows) in progress** (`cargo test` 34 pass):
  - **Adapter hotplug/reset:** a `DeviceWatcher` on `BluetoothAdapter::GetDeviceSelector()` (kept in `Inner.adapters`, started in `bt::start`). On Added/Removed (Updated needs a handler too or Windows raises neither) → after 500 ms: re-subscribe the radio (`subscribe_radio`, which drops the old subscription), send `btAdapter`, and `drop_all_links` if not on. Before this the radio was subscribed once, so a reset/swapped adapter went unheard. Verified: `btAdapter on` at startup from the enumeration's Added.
  - **Radio access:** `adapter_state` checks `Radio::RequestAccessAsync()`; not `Allowed` → new state **`denied`** with "Allow it in Settings → Privacy & security → Radios" and an "Open privacy settings" button. Desktop apps are normally allowed (it read allowed here).
  - `bt_open_settings(page)` takes `bluetooth` | `radios` and refuses anything else, so page text never reaches `explorer.exe`.
  - Release NSIS installer build: pending.
- **Next (when work resumes):** finish B5's radio off/on + reboot checks, then **B6** (battery over Bluetooth: `DEVPKEY_Bluetooth_Battery` first, GATT `2A19` notifications as fallback; "—" while sleeping). B2–B5 code is not committed yet (only B0–B1 are, in "phase b1").

## Idea

Two layers, kept apart:

- **Connection layer (new, `src-tauri/src/bt/`):** scan, pair, connect, forget, link state, battery. Windows uses WinRT (`windows` crate, not `windows-sys`); Linux uses BlueZ over D-Bus (`bluer`).
- **Input layer (exists):** once paired, the OS HID stack owns the mouse, so remapping stays in the [[Windows Input Backend]] and [[Linux Input Backend]]. Bluetooth only changes how the device is found and identified.

The app never speaks HID-over-GATT itself (Windows blocks `0x1812` for apps anyway). GATT is only used for the standard Battery (`0x180F`) and Device Information (`0x180A`) services.

## Phases

| Phase | What | Needs |
|---|---|---|
| B0 | Spike: does the N230 have BT, Classic or BLE, IDs, reports, GATT services, sleep timeout, DPI | Mouse only |
| B1 | Config `bus` + `address` per device; one `DeviceId` parser; `link` event | — |
| B2 | Remap a hand-paired BT mouse: `RIDEV_DEVNOTIFY` / handle refresh on Windows; `BUS_BLUETOOTH` match + new udev lines on Linux | Phase 9 for Linux |
| B3 | Discovery ("Search") with mouse/N230 filter, adapter-off handling | — |
| B4 | Pair / connect / forget from the app; Linux agent + `Trusted` | B3 |
| B5 | Link state machine: sleep, wake, mode switch, adapter off/on | B2 |
| B6 | Battery (BlueZ `Battery1` / sysfs, `DEVPKEY_Bluetooth_Battery`, GATT `0x2A19`, then report 6), firmware info | B5 |
| B7 | Settings → Devices cards and the "Add Bluetooth mouse" flow | B3–B6 |
| B8 | Permissions and adapter edge cases on clean accounts | B7 |
| B9 | `cargo test` + new Bluetooth items in the final checklist; before Phase 10 | all |

## Gotchas already known

- `windows.rs::matches_device` already parses both BT Raw Input path forms (`{00001124-…}_VID&0002vvvv_PID&pppp` Classic and `{00001812-…}_Dev_VID&02vvvv_PID&pppp` BLE). The VID's first byte is the **vendor-ID source** (`01` Bluetooth SIG, `02` USB-IF).
- A BT mouse that sleeps and wakes gets a **new Raw Input handle**: per-handle decisions must be refreshed on `WM_INPUT_DEVICE_CHANGE`.
- The Linux udev rule matches `ATTRS{idVendor}`, which **only exists on USB parents**: it never matches a Bluetooth mouse. BT needs `ATTRS{id/bustype}=="0005"` + `id/vendor`/`id/product` for input, and `KERNELS=="0005:VVVV:PPPP.*"` for hidraw.
- Matching stays VID/PID (+ bus), per [[CLAUDE.md Rules]]; `address` is only for reconnect/forget.
- Bluetooth may solve [[Battery Reading]]: BLE mice usually expose the standard Battery Service.

Related: [[N230 Hardware]], [[Tauri Rewrite]], [[Design System]].
