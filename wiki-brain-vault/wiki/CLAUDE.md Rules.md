# CLAUDE.md Rules

The repo-root `CLAUDE.md` holds the hard rules for [[N230 Control]]. This page is a summary; `CLAUDE.md` is the authority.

## Input handling

- Only the configured devices are remapped. A button-down and its button-up never get different treatment. See [[Windows Input Backend]] and [[Linux Input Backend]].
- Windows: one Raw Input registration per process. Nothing slow and no actions inside the hook.
- Linux: the device grab must end with the process, so never hold the device open in a child process.
- Mapping text is parsed in one place only (C# in `Monitor/`, Rust in `App/`), never in JS.

## Product

- No fake values in the UI ([[Battery Reading]]). Colors come only from theme variables ([[Design System]]).
- Devices are stored as VID/PID in the rewrite ([[Tauri Rewrite]]).

## Process

- Follow the phases in `App/STEPS.md` in order. Each phase's "Done when" check must pass first.
- Tests: `--selftest` in `Monitor/`, `cargo test` in `App/`. Non-trivial logic gets a check.
- Keep `Monitor/` until Phase 10 is done.

## Wiki-Brain

Read the wiki first, keep it updated, and log each session. See [[Wiki-Brain Setup]].
