# N230 Hardware

Measured on 2026-10-19 with `Monitor\…\MouseManager.exe --log` and the Monitor window. None of this is documented by Lenovo.

## From the product manual (2026-11-08)

The user added `mouse info.md` (repo root, Lenovo's spec sheet + manual text). Facts:

- Sensor **PAW3212** (PixArt), "6D" buttons, connection **2.4G + Bluetooth**, a **mode switch on the bottom** (2.4G / BT).
- DPI levels **1000 / 1200 / 1600**, cycled by the DPI button.
- Battery **500 mAh lithium**, charged over **USB-C**.

> **Contradiction (not resolved):** our measurements were **~1424 DPI** (two 10 cm slides, 2026-10-19) and **~795** after a possible DPI-button press ([[DPI Measurement]]). Neither is one of the manual's levels. Possible causes: slide length not exactly 10 cm, Windows pointer settings, the manual being generic, or a firmware that differs from the spec. Re-measure at each DPI-button step to settle it.

## 2.4G dongle

- USB **`VID_3554&PID_FC01`**, product string "Lenovo Services N230" (Compx chip).
- 6 HID collections:
  1. Mouse: 5 buttons, wheel, horizontal scroll (Windows path `…&Col01…`)
  2. Vendor page `0xFF01`
  3. Consumer controls
  4. System control
  5. Vendor page `0xFFBC`
  6. Feature-only `0xFF02`: answers **report ID 6** only (8 bytes). Read on 2026-10-19: `06000200EA648779`
- Full report scan (`MouseManager.exe --hid`, 2026-10-19), polling all 256 IDs:

  | Collection | Input / output / feature bytes | Answers |
  |---|---|---|
  | Mouse `0x01/0x02` | 8 / 0 / 0 | — (opened by Windows, not polled) |
  | Vendor `0xFF01` | 8 / 0 / 0 | input report **2**: `0200000000000000` |
  | Consumer `0x0C/0x01` | 3 / 0 / 0 | — |
  | System `0x01/0x80` | 2 / 0 / 0 | — |
  | Vendor `0xFFBC/0x88` | 2 / 0 / 0 | input report **4**: `0400` |
  | Vendor `0xFF02/0x02` | 0 / 0 / 8 | feature report **6**: `06000200EA648779` |

  No output reports exist, so the PC can't send commands to the mouse through standard HID reports. Every reading is passive.
- **Power:** a built-in battery, recharged over USB (per the user).

## Bluetooth (spike B0, in progress)

Seen 2026-11-07 from this PC (Intel adapter `USB\VID_8087&PID_0026`) with a throwaway WinRT scanner (`btspike scan`), see [[Bluetooth Connection]]:

- The N230 **does have Bluetooth**, and it's **Bluetooth Low Energy** (found by the LE enumerator; no Classic entry).
- Advertised name: **`Lenovo Services N230 BT2`**. "BT2" suggests several Bluetooth slots (BT1/BT2) plus the dongle: to confirm.
- Address `DB:13:6A:63:29:D1`, **random** type (top bits `11` = random static), so lookups need `BluetoothAddressType.Random`.
- Appearance **`0x03C2`** (962) = HID Mouse. Not paired yet.
- It stopped advertising within about a minute, so the pairing window is short.
- **Two slots, BT1 and BT2**, each advertising under its own name and its own random address. The address changed between pairing sessions (BT2 `DB:13:6A:63:29:D1`, BT1 `E3:CF:93:28:6D:C9` then `FE:46:E9:24:04:91`), so **don't rely on the address** to identify the mouse; use the PnP ID.
- Adverts carry HID service `1812`, appearance `0x03C2`, and Microsoft (`0x0006`) manufacturer data `030080` = a **Swift Pair** beacon. The name comes in the scan response.

### GATT, read before pairing (2026-11-07, BT1)

| Service | Characteristic | Value |
|---|---|---|
| Generic Access `1800` | Device Name `2A00` | `Lenovo Services N230 BT1` |
| | Appearance `2A01` | `C203` = `0x03C2` mouse |
| | Preferred conn. params `2A04` | `0700 0700 3000 2C01` = interval 8.75 ms, latency 48, timeout 3 s |
| Device Information `180A` | Manufacturer `2A29` | `BEKEN` (the BT chip vendor) |
| | Software rev `2A28` | `5.3.5.8` |
| | **PnP ID `2A50`** | `02 A725 A2FA 0805` = source USB-IF, **VID `25A7`, PID `FAA2`**, version `0508` |
| **Battery `180F`** | **Battery Level `2A19`** (Read, Notify) | **`0x53` = 83%** |
| HID `1812` | — | **AccessDenied**, as expected (Windows blocks apps) |
| Vendor `FFF0` | `FFF1` (WriteWithoutResponse) | — a **possible command channel**; not written to |
| | `FFF2` (Read) | `01 18` + `Lenovo Services N2…` (looks like length 24 + name) |
| | `FFF3` (Read) | `02 0A` + spaces + zeros |

- **The Bluetooth VID/PID (`25A7:FAA2`) is different from the dongle's (`3554:FC01`)**, so the config needs a second device entry.
- **Battery is solved over Bluetooth**: the standard Battery Service, with notifications. See [[Battery Reading]].
- Unlike the dongle (no output reports), BT has a writable vendor characteristic `FFF1`. Its protocol is unknown; **don't write to it** without a plan, since it could change settings.

### After pairing (Windows Settings, 2026-11-07, slot BT1)

Device nodes (`Get-PnpDevice`): `BTHLE\DEV_FE46E9240491` ("Lenovo Services N230 BT1"), one `BTHLEDEVICE\{uuid}_DEV_VID&0225A7_PID&FAA2_REV&0508_FE46E9240491` per GATT service (`1800`, `180A`, `180F`, `1812`, `FFF0`), and **two HID collections** (the dongle has six):

| Collection | Path (Raw Input / HID interface) | Caps |
|---|---|---|
| Keyboard `0x01/0x06` | `\\?\HID#{00001812-…}_Dev_VID&0225a7_PID&faa2_REV&0508_fe46e9240491&Col01#…\KBD` | in 9 / out 2 (LEDs) / feature 0 |
| **Mouse `0x01/0x02`** | `\\?\HID#{00001812-…}_Dev_VID&0225a7_PID&faa2_REV&0508_fe46e9240491&Col02#…` | in 7 / out 0 / feature 0 |

- **No vendor collections and no feature report 6 over Bluetooth.** Report 6 only exists on the dongle, and the dongle kept answering it (byte 5 = `0x64`) while the mouse was on Bluetooth, so it's the **dongle's** state, not the mouse's.
- The path is **lowercase** and embeds the address. `windows.rs::matches_device` upper-cases it and accepts `_Dev_VID&0225a7_PID&faa2`, so a config entry `{vid:"25A7", pid:"FAA2"}` should be enough for the remapper (to confirm with real clicks).
- **Battery, two ways, both 84%:**
  - GATT `2A19` read while connected (Windows lets apps read `180F` on a HID device; only `1812` is denied).
  - Windows device property **`{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2`** (`DEVPKEY_Bluetooth_Battery`, type Byte) on the `BTHLE\DEV_…` node, readable with `Get-PnpDeviceProperty` or `CM_Get_DevNode_PropertyW`. Property 3 read `False`, property 7 held a timestamp of the last update.

### Buttons over Bluetooth (2026-11-07, `n230-control.exe --log 180`)

- Left, Right, **X1 and X2** all arrive through Raw Input as normal button events from the **`Col02`** mouse path above, with down/up pairs. Same as the dongle, so the remapper needs no new decoding.
- They showed as `[other]` only because `25A7:FAA2` wasn't in the default device list yet (B1 adds it).
- Not re-checked: that rear = X1 and front = X2 physically over BT (assumed the same as the dongle).
- The C# `--log` can't be redirected to a file (it `AttachConsole`s to the parent); use the Rust debug build's `--log`, which prints to stdout.

### Pairing

- Pairing from a console app (`DeviceInformationCustomPairing.PairAsync`, ConfirmOnly, protection None, then Default) returned `Failed` with no pairing request, probably because the pairing window had already closed (about 1 minute). To retry in B4.
- Still open: Raw Input path after pairing, whether report 6 answers over BT, sleep timeout, DPI over BT.
- Another LE device nearby, `FB:1E:ED:D7:2D:C2` (no name), sends *directed* adverts (aimed at one bonded host). Unknown, probably not the mouse (it was there while the mouse showed as BT1 elsewhere).

## Buttons

| Button | Sends | Remappable |
|---|---|---|
| Left / Right / wheel | Standard; wheel ±120 per notch | Not needed |
| Side, rear | **X1** (Back). On Linux expected `BTN_SIDE`, to confirm in spike 0c | Yes |
| Side, front | **X2** (Forward). On Linux expected `BTN_EXTRA`, to confirm in spike 0c | Yes |
| Behind the wheel (DPI) | **Nothing** on any channel; feature report 6 doesn't change | No. The firmware handles it |

## DPI

- Measured **~1424 DPI** (two 10 cm slides: 5605 and 5604 counts), then one reading of **~795** after a possible DPI button press. See [[DPI Measurement]].

## Unknown

- **Bluetooth and wired device IDs:** not captured yet (Bluetooth: spike B0 in [[Bluetooth Connection]]). Use `--log`, or Monitor → "Show all devices", and add them to the config's devices.
- **Battery:** see [[Battery Reading]].
- **Linux node layout:** which `/dev/input/event*` node carries `REL_X` + `BTN_SIDE` (spike 0c in `App/STEPS.md`).

Related: [[N230 Control]], [[Windows Input Backend]], [[Linux Input Backend]].
