# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**N230 Control**: a peripheral manager (in the spirit of Razer Synapse) for one mouse, the **Lenovo N230**. It remaps the side buttons X1/X2 for that mouse only, measures DPI (and scales it in software if possible), shows battery % if readable, and offers a live monitor, tray icon and color themes. Targets: **Windows and Linux**.

| Folder | What |
|---|---|
| `Monitor/` | The working Windows app: .NET 10 WinForms, C#, no NuGet packages, P/Invoke in `Native.cs`. The reference implementation. Keep it until Phase 10 of the rewrite is done. |
| `App/` | The rewrite: **Tauri 2 + Rust core + vanilla HTML/JS UI**, for Windows and Linux. Plan: `App/STEPS.md` (phases 0–10). Phase 1 skeleton exists. |
| `wiki-brain-vault/` | The project knowledge base. **Read it first** (see below). |

## Commands

From the repo root, for the C# app:

```powershell
Stop-Process -Name MouseManager -ErrorAction SilentlyContinue                  # the running exe is locked, so stop it before building
dotnet build Monitor
Monitor\bin\Debug\net10.0-windows\MouseManager.exe --selftest | Out-Host       # assert-based self-check; exit code 0 = pass
Monitor\bin\Debug\net10.0-windows\MouseManager.exe --log 60 | Out-Host         # console dump of raw input for 60 s, tagged by device
Monitor\bin\Debug\net10.0-windows\MouseManager.exe                             # tray app (single instance via named mutex)
```

The project is `WinExe`, and PowerShell doesn't wait for GUI executables, so pipe the console modes into `| Out-Host`.

For the rewrite (once Phase 1 exists), from `App/`: `npm run tauri dev` runs the app, `cargo test --manifest-path src-tauri/Cargo.toml` runs the tests, and `npm run tauri build` builds the installers. Build each OS on that OS.

## Hard rules

- **Only configured devices are remapped.** Other mice must never be affected, and a button-down and its button-up always get the same treatment.
- **Windows:** register Raw Input only once per process, in the remapper. Everything else subscribes to its events. Inside the low-level hook: no actions, no I/O, no UI. Queue the work and run it on `WM_APP` or a timer.
- **Linux:** the evdev grab must end with the process (so a crash gives the mouse back). Never keep the device open in a child process.
- **Mapping strings** are parsed in one place only: `ButtonAction.cs` in `Monitor/`, the Rust `action.rs` in `App/`. Never in JS. Mouse button names are checked before key names (`Back` isn't Backspace).
- **Config in the rewrite** stores devices as VID/PID (not Windows paths), and Rust owns the file.
- **No fake values in the UI:** battery and DPI show "—" when unknown.
- **Colors come only from the theme CSS variables** (5 themes, see the wiki's Design System page).
- **Follow `App/STEPS.md` phase by phase.** A phase's "Done when" check must pass before the next phase starts.
- **Tests:** there's no test framework. Use `SelfTest()` in `Monitor/Program.cs`, `#[cfg(test)]` with `cargo test` in `App/`, and `npm run check` (plain `node:assert` scripts in `App/tests/`) for pure JS modules. Non-trivial logic gets a check.

## Wiki-Brain: knowledge base (READ FIRST, KEEP UPDATED)

This repo has a persistent, cross-linked knowledge base at **`wiki-brain-vault/`**. It holds what we know about this project: architecture, hardware facts, decisions and gotchas. Treat it as primary context.

### Use it

- **At the start of a task, read the wiki before digging through the code.** Entry point: `wiki-brain-vault/wiki/index.md`. Follow the `[[Page]]` links to the relevant pages.
- Key pages:
  - `N230 Hardware`: measured device facts
  - `Windows Input Backend`: how the hook + Raw Input device selection works
  - `Linux Input Backend`
  - `Tauri Rewrite`
  - `DPI Measurement`
  - `Battery Reading`
  - `Design System`
  - `Stack Decision`
- `graphify` and the `/wiki-brain` skill aren't installed here, so read `wiki/` and `index.md` directly.
- Only read files in `wiki-brain-vault/raw/` when a page points there or the user asks. **Never modify `raw/`**: those sources are immutable.

### Keep it updated (mandatory)

- **Claude owns `wiki-brain-vault/wiki/`.** When a task produces lasting knowledge (a decision, a resolved bug, a hardware measurement, an architecture change, a completed phase), create or update the relevant page(s) before finishing. Don't ask permission per page; just report what changed.
- **Cross-link heavily** with Obsidian `[[Page Name]]` links. A page with no inbound links is a dead end.
- **Always update `wiki-brain-vault/wiki/index.md`** when you create or rename a page.
- **End every session with a log entry** appended to `wiki-brain-vault/log.md`:

  ```text
  ## [YYYY-MM-DD HH:MM] session | <title in 3-8 words>
  Touched: <comma-separated wiki pages, or "none">
  ```

  If the session was trivial (a one-off fix, a routine chore, pure exploration), just add the log entry and skip the wiki update.
- **Flag contradictions; don't silently resolve them.** If new information conflicts with an existing page, tell the user.

### Ingesting sources

To fold in a source (a captured `--log` dump, an HID descriptor, notes) dropped in `wiki-brain-vault/raw/`: summarize it into a wiki page, cross-link it, list it under **Sources** in `index.md`, and add an `ingest` line to `log.md`.
