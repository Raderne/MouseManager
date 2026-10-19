# Windows Input Backend

How to remap the buttons of **one** mouse on Windows. It's implemented in `Monitor/Remapper.cs` ([[Monitor App]]) and will be ported to `platform/windows.rs` (Phase 4 of the [[Tauri Rewrite]]).

## The problem

- A **low-level mouse hook** (`WH_MOUSE_LL`) can block a click, but it doesn't know which device sent it.
- **Raw Input** (`WM_INPUT`) knows the device, but it can't block anything.

## The solution: both, on one message-only window

1. On a button-down, the hook drains pending `WM_INPUT` messages with `PeekMessage`. The click's raw input is already queued by then, so this tells it which device the click came from.
2. That decision is stored in `held[button]` and reused for the matching button-up. A down/up pair is never split.
3. If the click is from a configured device and has a mapping, swallow it (return 1) and **queue** the action. Actions run on `WM_APP`, never inside the hook.
4. Ignore injected input (`LLMHF_INJECTED`), which stops the app reacting to its own `SendInput` calls.

## Gotchas

- **One Raw Input registration per process.** Windows sends each usage page to only one window per process. The remapper therefore registers every collection and re-publishes all events. Other parts of the program (the monitor) subscribe to that and must never register Raw Input themselves.
- **The hook can run `RawInput` handlers** (because it drains `WM_INPUT`). Keep them cheap: buffer the data and touch the UI or stdout later from a timer.
- **Hook timeout:** if the callback takes longer than about 300 ms, Windows silently removes the hook. The Rust port should detect that and re-install it.
- **Software DPI scaling** would also need movement swallowed and re-injected in the hook. Spike 0b ([[DPI Measurement]]).

## Rust port notes

One thread owns the window, the hook and a `GetMessage` loop. The hook is an `extern "system" fn` with its state in `thread_local!`. Commands arrive over a channel, and `PostMessage(WM_APP)` wakes the thread.

Related: [[Linux Input Backend]], [[N230 Hardware]].
