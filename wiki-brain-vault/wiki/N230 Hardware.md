# N230 Hardware

Measured on 2026-10-19 with `Monitor\…\MouseManager.exe --log` and the Monitor window. None of this is documented by Lenovo.

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

- **Bluetooth and wired device IDs:** not captured yet. Use `--log`, or Monitor → "Show all devices", and add them to the config's devices.
- **Battery:** see [[Battery Reading]].
- **Linux node layout:** which `/dev/input/event*` node carries `REL_X` + `BTN_SIDE` (spike 0c in `App/STEPS.md`).

Related: [[N230 Control]], [[Windows Input Backend]], [[Linux Input Backend]].
