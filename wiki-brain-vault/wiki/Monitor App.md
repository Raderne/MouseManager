# Monitor App

The working Windows tray app in **`Monitor/`**: .NET 10 WinForms, C#, no NuGet packages. All OS access goes through P/Invoke in `Native.cs`. It's the reference for the [[Tauri Rewrite]]: port its behavior, then retire it in Phase 10.

## Files

| File | Role |
|---|---|
| `Program.cs` | Entry point: tray app, `--log [s]`, `--selftest` (the only test: assert-based) |
| `Remapper.cs` | Hook + Raw Input device selection: see [[Windows Input Backend]] |
| `ButtonAction.cs` | Mapping-string parser |
| `Config.cs` | `%APPDATA%\MouseManager\config.json` |
| `RawInputLogger.cs` | Raw Input collection list + event text (`Describe`) |
| `MonitorForm.cs` | Live event list + DPI meter ([[DPI Measurement]]) + polling of report 6 |
| `TrayApp.cs` | Tray menu, Settings form, Start with Windows (`HKCU\…\Run`) |
| `Native.cs` | P/Invoke declarations |

## Mapping strings

- `Default` (the click passes through), `Disabled`
- A mouse button: `Left`, `Right`, `Middle`, `Back`, `Forward`
- `run:<target>`, opened with shell-execute
- A key combo joined with `+` (`Ctrl+Shift+T`, `VolumeUp`, `F5`)

Mouse button names are checked **before** key names, because `Keys.Back` is Backspace. Invalid text throws `FormatException`. `Remapper.Apply` adds the button name to the message and changes nothing.

## Config

`Devices` holds substrings matched against the device path (default `VID_3554&PID_FC01`). `Buttons` is keyed `Middle`, `Back`, `Forward`. `Config.Save()` re-serializes the class, so **keys it doesn't know about are lost**.

## Gotchas

- The project is `WinExe`, so PowerShell doesn't wait for it. Pipe the console modes into `| Out-Host`.
- Stop the running app before `dotnet build`, because the exe is locked.

The Rust port is `App/src-tauri/src/action.rs` ([[Tauri Rewrite]] → Rust core).

Related: [[N230 Control]], [[N230 Hardware]].
