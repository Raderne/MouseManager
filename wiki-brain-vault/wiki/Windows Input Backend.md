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

### As built (`App/src-tauri/src/platform/windows.rs`, 2026-10-21)

- **Crate:** `windows-sys` 0.61, which Tauri already depends on.
- **Re-entrancy:** low-level hook calls are delivered as *sent messages*, so the `PeekMessage` that drains `WM_INPUT` inside the hook can call the hook again. The state is a `RefCell`, borrowed only briefly via `with()` (which uses `try_borrow_mut`) and never across `PeekMessage` or `SendInput`. A panic inside the `extern "system"` hook would abort the process, so nothing in there unwraps.
- **Device match:** VID/PID from the config.
  - USB paths contain `VID_3554&PID_FC01`.
  - Bluetooth paths contain `_VID&0002046d_PID&b01a` (classic) or `_VID&02046d_PID&b01a` (LE).
  - Entries that aren't 4 hex digits match nothing, so a blank entry can't remap every mouse.
- **Actions:**
  - Key combos go out as **one** `SendInput` call (down in order, up in reverse).
  - `run:` uses `ShellExecuteW` on its own thread, so a slow launch can't stall the hook thread and get the hook removed.
- **Hook liveness:** every second, compare the non-injected events Raw Input saw with those the hook saw. If Raw Input saw ≥ 10 and the hook saw 0, Windows removed the hook: re-install it and send an `error` event.
- **Movement:** summed per 50 ms timer tick into `move` events (the N230 always; other devices only with "show all devices"). With a scale ≠ 1, N230 `WM_MOUSEMOVE` is swallowed and re-sent scaled, carrying the fraction over, as in spike 0b ([[DPI Measurement]]).
- **Raw Input pages:** only the mouse page (0x01/0x02) is registered. The C# `--log` also listened to the consumer and vendor pages.
- **Debug tool:** `n230-control.exe --log <seconds>` prints button events (all devices) and N230 moves.

Related: [[Linux Input Backend]], [[N230 Hardware]].
