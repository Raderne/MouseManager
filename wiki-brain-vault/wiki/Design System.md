# Design System

**Canvas:** https://claude.ai/artifact/ShCfcoUsPKsJ3wegajBHxi (private; it's shared from the page's Share menu).

## Chosen direction (2026-10-19)

- **Layout and components: "B · Pulse"**:
  - rounded cards (radii 24 / 18 / 12 / pill)
  - pill tabs in the header (Buttons, Performance, Monitor, Settings)
  - a battery ring in the header
  - a mouse diagram that highlights the selected side button
  - an effective-DPI dial
- **Colors: user-selectable themes.** The user liked A · Ember's red, so it became a theme. A · Ember's angular layout was **not** chosen.
- **Fonts:** Space Grotesk (UI) and JetBrains Mono (numbers), **bundled** as woff2 files so WebView2 and WebKitGTK look the same.
- The canvas also has a "B · Settings" screen with a working theme picker.

## Themes (CSS variables on `:root[data-theme]`)

| Theme | `--accent` | `--on-accent` | `--bg` | `--panel` | `--panel2` | `--line` | `--text` | `--muted` |
|---|---|---|---|---|---|---|---|---|
| pulse (green, default) | #3DDC84 | #062612 | #0B0F0D | #111714 | #17201B | #2E3D36 | #E8F1EC | #8FA39A |
| ember (red) | #FF3B3B | #0A0A0C | #0A0A0C | #131317 | #1B1B21 | #33333D | #EDEDF0 | #9A9AA6 |
| frost (ice blue) | #38BDF8 | #03202E | #0A0D11 | #111720 | #172030 | #2B3A4D | #E6EEF6 | #8DA0B5 |
| amber (gold) | #FBBF24 | #241A02 | #0F0D09 | #18150F | #211D15 | #3D362A | #F3EEE4 | #A89F8E |
| violet (purple) | #A78BFA | #1A0F3A | #0D0B12 | #15121D | #1E1A29 | #383149 | #EEEAF6 | #A096B5 |

## Rules

- Colors come **only** from these variables. SVG uses `currentColor` and `var(--accent)`.
- The theme is saved in `config.theme`. Set `data-theme` before first paint, so there's no wrong-color flash.
- Muted text keeps a contrast ratio of at least 4.5:1 on `--panel` in every theme. Touch targets are at least 44 px. Use real `<button>`/`<select>`/`<label>`.
- No fake data: the battery shows "—" when unknown ([[Battery Reading]]).

Related: [[Tauri Rewrite]] (Phase 2), [[DPI Measurement]].
