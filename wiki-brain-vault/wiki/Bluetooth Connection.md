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
- **Next: B2** (remap the paired BT mouse for real; `RIDEV_DEVNOTIFY` handle refresh on sleep/wake).

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
