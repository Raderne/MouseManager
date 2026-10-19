# N230 Control

A peripheral manager, similar in spirit to Razer Synapse, for **one mouse: the Lenovo N230** ([[N230 Hardware]]). It runs on **Windows and Linux**.

## Features

- Remap the two side buttons, **X1 (Back)** and **X2 (Forward)**, **for the N230 only**. Other mice keep their normal buttons.
- Show and measure DPI. Set a target DPI through software scaling if possible ([[DPI Measurement]]).
- Battery %, if the mouse reports it ([[Battery Reading]]).
- A live input monitor, tray icon, start with the OS, and 5 color themes ([[Design System]]).

## Two codebases

| Folder | What | Status |
|---|---|---|
| `Monitor/` | [[Monitor App]]: C# WinForms, Windows only | Works. Reference implementation |
| `App/` | [[Tauri Rewrite]]: Tauri 2 + Rust + HTML UI | Planned, see `App/STEPS.md` |

`Monitor/` is kept until the rewrite passes its final checklist on both systems (Phase 10).

## History

- 2026-10-19: built and verified the C# app (`--selftest`, `--log`), measured the mouse, moved the app to `Monitor/`, chose the Pulse design with themes, and picked Tauri + Rust for Windows and Linux ([[Stack Decision]]).

Related: [[Wiki-Brain Setup]].
