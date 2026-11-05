# Upscalee — Design System

Source of truth for tokens: [`apps/desktop/src/shared/tokens.css`](../apps/desktop/src/shared/tokens.css).
Shared components: [`base.css`](../apps/desktop/src/shared/base.css), [`Logo.tsx`](../apps/desktop/src/shared/Logo.tsx),
[`settings/controls.tsx`](../apps/desktop/src/settings/controls.tsx).
Everything below is **implemented** unless marked *(proposed)*.

## 1. Principles

1. **Zero-friction.** The product lives in a right-click. Every surface is
   transient, appears where the user is looking, and gets out of the way.
2. **One decision per surface.** The wheel asks only "how big?"; options are
   pre-filled with sensible defaults (keep format, Photo model).
3. **Never steal focus or files.** The activity panel never activates; outputs
   never overwrite anything.
4. **Honest about quality.** When AI isn't used (no GPU, engine missing, tiny
   source) the UI says so in plain words.
5. **Feels native on Windows 11.** Segoe UI Variable, light/dark from the
   system, rounded geometry, subtle elevation.

## 2. Brand

* Name: **Upscalee**. Mark: low-resolution pixels growing into a smooth diagonal
  "expand" arrow on a violet→cyan tile (`apps/desktop/app-icon.svg`; all icon
  sizes generated with `tauri icon`).
* Brand colours: violet `#5B3DF5`, cyan `#1FB8F0`; gradient used for the mark and
  progress bars only — never behind text.
* Voice: short, plain, verb-first ("Upscale images…", "Show in folder"). Errors
  say what happened and what to do ("Not enough memory for 16K, try 8K").

## 3. Themes

Light and dark follow the Windows app theme via `prefers-color-scheme`
(WebView2 inherits it). Overlay windows (radial, HUD) use near-opaque surfaces
(`--overlay`, 96 % alpha) because the native window is transparent and the
desktop behind can be anything.

## 4. Colour tokens (semantic)

| Token | Light | Dark | Use |
|-------|-------|------|-----|
| `--surface-0` | `#f6f6fa` | `#0f0f15` | Window background (Settings) |
| `--surface-1` | `#ffffff` | `#17171f` | Cards, centre disc |
| `--surface-2` | `#efeff5` | `#20202b` | Inputs, inner ring band, secondary buttons |
| `--surface-3` | `#e4e4ee` | `#2a2a37` | Hover, progress track |
| `--overlay` | `rgba(250,250,253,.96)` | `rgba(23,23,31,.96)` | Radial disc, HUD |
| `--border` / `--border-strong` | 12 % / 22 % ink | 10 % / 20 % white | Dividers, control outlines |
| `--text-1` / `-2` / `-3` | `#15151f` / `#4a4a5e` / `#6b6b80` | `#f2f2f7` / `#c0c0cf` / `#9494a8` | Primary / secondary / tertiary text |
| `--text-disabled` | `#a3a3b3` | `#5e5e70` | Disabled labels only |
| `--accent` / `--accent-hover` | `#5b3df5` / `#4a2de0` | `#8a70ff` / `#9d87ff` | Selection, primary buttons, focus ring |
| `--accent-contrast` | `#ffffff` | `#0f0f15` | Text on accent |
| `--accent-soft` / `-strong` | 12 % / 20 % accent | 16 % / 28 % accent | Highlighted wedge, active option |
| `--success` / `--warning` / `--danger` (+ `-soft`) | `#12804a` / `#995500` / `#c22a46` | `#3ccf8e` / `#f2b44c` / `#ff6b81` | Status icons, messages, pills |

### Measured contrast (WCAG 2.x, on `--surface-1`)

| Foreground | Light | Dark |
|------------|-------|------|
| `--text-1` | 18.1 : 1 | 16.0 : 1 |
| `--text-2` | 8.6 : 1 | 9.9 : 1 |
| `--text-3` | 5.2 : 1 | 6.0 : 1 |
| `--accent` | 6.1 : 1 | 5.0 : 1 |
| `--success` / `--warning` / `--danger` | 5.0 / 5.7 / 5.7 | 8.9 / 9.7 / 6.5 |
| `--accent-contrast` on `--accent` | 6.1 : 1 | 5.3 : 1 |
| `--text-disabled` | 2.5 : 1 (exempt: inactive) | 2.8 : 1 (exempt) |

Rule: body text uses `--text-1/2/3` only; status colours carry an icon or words,
never colour alone.

## 5. Typography

| Token | Value | Use |
|-------|-------|-----|
| `--font-display` | Segoe UI Variable Display | Titles, wedge labels, centre summary |
| `--font-sans` | Segoe UI Variable Text | Everything else |
| `--font-mono` | Cascadia Mono | Version numbers, paths when monospaced |
| Scale | 11 / 12 / 14 / 16 / 20 / 28 px (`--text-xs` … `--text-2xl`) | Radial wedge label 21 px bold, centre summary 17 px bold |
| Weights | 400 / 500 / 600 / 700 | |

## 6. Spacing, radius, elevation, layout

* Spacing: 4 px grid — `--space-1` 4 … `--space-8` 32.
* Radius: `--radius-sm` 6 (buttons, inputs) · `md` 10 (cards) · `lg` 14 (HUD) · `xl` 20 · `full` pills/toggles.
* Elevation: `--shadow-1` (cards), `--shadow-overlay` *(reserved)*; the radial disc uses a soft drop shadow inside the window bounds.
* Layout: Settings is a single 760 px column of titled cards; rows put label + hint on the left and the control on the right, stacking below 640 px.

## 7. Motion

| Token | Value | Use |
|-------|-------|-----|
| `--duration-fast` | 120 ms | Hover/fill transitions |
| `--duration-base` | 180 ms | Radial scale-in (0.88 → 1), HUD slide-up |
| `--ease-out` | `cubic-bezier(.2,.8,.2,1)` | All transitions |

`prefers-reduced-motion: reduce` sets both durations to 0 and stops the spinner.

## 8. Components

| Component | Where | States |
|-----------|-------|--------|
| **Quality wedge** (outer ring) | Radial | default · hover/highlighted (accent-soft fill, accent outline + label) · partial (`k of N` in warning colour) · disabled "Already larger" (`--text-disabled`, not-allowed cursor) · busy (whole wheel 60 % opacity, no pointer) |
| **Option wedge** (inner ring) | Radial | default · hover (`--surface-3`) · active (accent-soft fill, bold accent label) |
| **Centre disc** | Radial | summary + detail (`1920 × 1080` → on hover `→ 3840 × 2160`) + hint; warning line for no-GPU / engine missing / unreadable files |
| **Job row** | HUD | queued (static ring) · running (spinner, stage text, gradient progress bar) · done (green check, Open / Show in folder, warnings) · failed (red ×, message, suggestion) · skipped / cancelled (grey –) |
| **Button** `.btn` | All | default · hover · disabled · variants: primary, ghost, danger, small, icon |
| **Toggle** (`role="switch"`) | Settings | off · on · focus ring |
| **Segmented control** (`role="radiogroup"`) | Settings | selected · unselected |
| **Select** (native) | Settings | default · focus |
| **Pill** | Settings | neutral · success · warning · danger · accent |
| **Card / Section / Row** | Settings | — |
| **Save status** | Settings | Saving… · All changes saved · error |

Empty / loading / error states: HUD "No recent activity"; Settings "Loading…"
and inline error lines; radial error chip at the bottom when a selection fails.

## 9. Screens and surfaces

| Surface | Size | Behaviour |
|---------|------|-----------|
| Explorer menu entry | — | "Upscale with Upscalee" with the app icon |
| Radial menu | 420 × 420 logical px, transparent, frameless, topmost | Centred on the cursor, clamped to the work area, DPI-aware; closes on select / Esc / click outside / focus loss |
| Activity HUD | 380 px wide, content height | Bottom-right of the radial's monitor, never activates, auto-hides |
| Settings | 780 × 720 (min 640 × 540) | Normal window; closing it doesn't quit the tray app |
| Tray menu | — | Upscale images… · Show activity · Settings · Open logs folder · Quit Upscalee |

## 10. User flows

1. **Right-click → wheel → saved file** (primary): Explorer menu → wheel (pick size; optional Tab/F) → HUD progress → Open / Show in folder.
2. **Tray → Upscale images…**: file dialog → wheel at cursor → same as above.
3. **First run**: installer "Run Upscalee" → Settings shows how to use it and the integration status.
4. **Fix integration**: Settings → File Explorer integration → Repair / Remove.

## 11. Accessibility

* Keyboard: the wheel is fully operable (hotkeys, arrows, Enter, Tab/Shift+Tab, F, Esc); Settings uses native buttons/selects.
* Focus: visible 2 px accent ring (`--focus-ring`) on every focusable control.
* Semantics: wheel `role="menu"` with `menuitem` / `menuitemradio` + `aria-checked`/`aria-disabled`; live region announces the highlighted preset; HUD title and save status are `aria-live`; progress bars use `role="progressbar"` with values.
* Colour is never the only signal (status icons + text).
* Reduced motion honoured.
* *(Proposed)* Screen-reader pass with Narrator on the transient wheel.

## 12. Implementation checks (before merging UI changes)

- [ ] Uses tokens only — no new hex values outside `tokens.css`.
- [ ] Light **and** dark checked; text contrast ≥ 4.5 : 1.
- [ ] Every interactive element reachable by keyboard and has an accessible name.
- [ ] Transient windows keep transparent backgrounds (`html.overlay`).
- [ ] HUD height still reported correctly (no fixed heights inside rows).
- [ ] `pnpm --filter upscalee-desktop typecheck && pnpm --filter upscalee-desktop test` pass.
