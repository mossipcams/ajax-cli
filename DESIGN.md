---
name: Ajax Cockpit
description: Mobile-first operator console for a local AI agent fleet — status, decision, terminal.
colors:
  soft-charcoal: "#161616"
  soft-charcoal-tint: "#1d1d1d"
  soft-charcoal-raised: "#242424"
  soft-charcoal-high: "#2d2d2d"
  ink: "#e6e6e6"
  ink-soft: "#c6c6c6"
  ink-muted: "#a8a8a8"
  ink-faint: "#808080"
  rule: "#2a2a2a"
  rule-strong: "#454545"
  soft-steel-blue: "#87afd7"
  soft-steel-blue-bright: "#a3c6e8"
  soft-steel-blue-deep: "#24384c"
  attention-amber: "#d7af5f"
  attention-amber-bright: "#e5c88a"
  fault-rose: "#d78787"
  fault-rose-bright: "#e3a5a5"
  fault-rose-deep: "#5f3535"
  done-sage: "#87af87"
  done-sage-bright: "#a5c8a5"
typography:
  micro:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "10.5px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "0.08em"
  label:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "0.08em"
  data:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 1.3
    letterSpacing: "normal"
  body-sm:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  body:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  heading:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "16px"
    fontWeight: 700
    lineHeight: 1.3
    letterSpacing: "0.01em"
  title:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "17px"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "0.18em"
  display:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "20px"
    fontWeight: 700
    lineHeight: 1.25
    letterSpacing: "0.01em"
  input:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: "normal"
  narrow-body:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  icon-glyph:
    fontFamily: "ui-sans-serif, Avenir Next, Helvetica Neue, system-ui, -apple-system, sans-serif"
    fontSize: "18px"
    fontWeight: 400
    lineHeight: 1
    letterSpacing: "normal"
  mono:
    fontFamily: "ui-monospace, SFMono-Regular, SF Mono, Menlo, Monaco, Consolas, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "normal"
rounded:
  sm: "6px"
  md: "10px"
  lg: "14px"
  pill: "999px"
spacing:
  1: "4px"
  2: "8px"
  3: "12px"
  4: "16px"
  5: "20px"
  6: "24px"
components:
  button-primary:
    backgroundColor: "{colors.soft-steel-blue}"
    textColor: "{colors.soft-charcoal}"
    rounded: "{rounded.pill}"
    padding: "10px 16px"
    height: "44px"
  button-primary-hover:
    backgroundColor: "{colors.soft-steel-blue-bright}"
    textColor: "{colors.soft-charcoal}"
    rounded: "{rounded.pill}"
    padding: "10px 16px"
    height: "44px"
  button-secondary:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.pill}"
    padding: "10px 16px"
    height: "44px"
  button-destructive:
    backgroundColor: "transparent"
    textColor: "{colors.fault-rose}"
    rounded: "{rounded.pill}"
    padding: "10px 16px"
    height: "44px"
  button-destructive-confirm:
    backgroundColor: "{colors.fault-rose}"
    textColor: "{colors.soft-charcoal}"
    rounded: "{rounded.pill}"
    padding: "10px 16px"
    height: "44px"
  pill-primary:
    backgroundColor: "{colors.soft-steel-blue}"
    textColor: "{colors.soft-charcoal}"
    rounded: "{rounded.pill}"
    padding: "8px 16px"
    height: "34px"
  input-field:
    backgroundColor: "{colors.soft-charcoal}"
    textColor: "{colors.ink}"
    rounded: "{rounded.sm}"
    padding: "10px 12px"
    typography: "{typography.input}"
  nav-new-task:
    backgroundColor: "{colors.soft-steel-blue}"
    textColor: "{colors.soft-charcoal}"
    rounded: "{rounded.sm}"
    height: "44px"
  status-dot:
    backgroundColor: "{colors.soft-steel-blue}"
    rounded: "{rounded.pill}"
    size: "9px"
---

# Design System: Ajax Cockpit

## 1. Overview

**Creative North Star: "Ajax"**

Ajax is the product and the visual brief: a pocket operator console for a local agent fleet. Soft Charcoal surfaces, Soft Steel Blue as the running signal, and a terminal pane that owns the work. Chrome is sparse, pill-decisive, and mobile-first for Safari on iPhone — status, required decision, then the raw tmux/xterm bridge.

Density stays operational, not decorative. Tone carries meaning (`running` / `waiting` / `error` / `done`); accents are CLI-locked to match Native Cockpit. Motion is short state feedback (≈140–220ms), never page choreography. The system rejects generic SaaS dashboards (card grids, metric strips, soft purple/indigo chrome) and overbuilt IDE shells that fight the terminal.

**Key Characteristics:**
- Soft Charcoal stack with hairline rules; depth mostly from paper steps
- Soft Steel Blue as primary action and running tone; semantic amber / rose / sage for wait / fault / done
- Pill actions (44px min) and uppercase tracked chrome labels
- Shell width capped at 560px; iOS safe-area and keyboard geometry are first-class
- Terminal is the signature surface; UI never competes with the pane
- Fixed rem type ramp in CSS (`--text-micro` … `--text-display`); no Inter in the stack

## 2. Colors

Restrained dark console palette: neutrals do the work; semantic accents speak only when status or a decisive action needs them. Values stay in lockstep with `ajax-tui` xterm 110 / 179 / 174 / 108.

### Primary
- **Soft Steel Blue** (`#87afd7`): Primary actions, current nav, running tone. Bright (`#a3c6e8`) on hover; deep (`#24384c`) for filled attention bands (update banner, selected nav wash).

### Secondary
- **Attention Amber** (`#d7af5f`): Waiting / needs-input / remediation. Bright (`#e5c88a`) for ready-for-review emphasis.

### Tertiary
- **Fault Rose** (`#d78787`): Errors and destructive intents. Bright (`#e3a5a5`) on confirm/hover fill; deep (`#5f3535`) for destructive borders at rest.
- **Done Sage** (`#87af87`): Success / done. Bright (`#a5c8a5`) when a brighter success cue is needed.

### Neutral
- **Soft Charcoal** (`#161616`): Body / app paper.
- **Soft Charcoal Tint / Raised / High** (`#1d1d1d` / `#242424` / `#2d2d2d`): Nested surfaces and hover washes — the main depth ladder.
- **Ink** (`#e6e6e6`): Primary text. Soft / muted / faint (`#c6c6c6` / `#a8a8a8` / `#808080`) for secondary, status line, and empty chrome.
- **Rule / Rule Strong** (`#2a2a2a` / `#454545`): Hairline dividers and resting control borders.

### Named Rules
**The One Tone Rule.** Status is a single `--tone` / `--tone-bg` pair consumed by dots, badges, rows, and interact pills. Never invent a parallel status color path.

**The Accent Rarity Rule.** Soft Steel Blue and semantic accents appear for running state, primary CTA, or attention — not as decorative fill across idle chrome.

## 3. Typography

**Display Font:** none (product UI — no display face)
**Body Font:** UI sans (`ui-sans-serif`, Avenir Next, Helvetica Neue, system-ui) — Inter removed
**Label/Mono Font:** `ui-monospace, SFMono-Regular, SF Mono, Menlo, Monaco, Consolas, monospace` for terminal-adjacent output (xterm stack)

**Character:** One technical sans for all chrome; mono only where the CLI would speak. Fixed rem product scale, not fluid marketing type.

### Hierarchy (CSS tokens)

| Token | rem | px | Role |
| --- | --- | --- | --- |
| `--text-micro` | 0.65625rem | 10.5 | Row status/time, copy chips, subheads |
| `--text-label` | 0.75rem | 12 | Buttons, pills, nav, labels, mono chrome |
| `--text-data` | 0.78125rem | 12.5 | Status line, tabular data |
| `--text-body-sm` | 0.8125rem | 13 | Summaries, notes, toast |
| `--text-body` | 0.9375rem | 15 | Base UI copy |
| `--text-heading` | 1rem | 16 | Task detail title |
| `--text-title` | 1.0625rem | 17 | Cockpit wordmark |
| `--text-display` | 1.25rem | 20 | Settings page heading |

**Tracking:** `--tracking-display` `0.18em` (wordmark / empty chrome); `--tracking-label` `0.08em` (working uppercase labels). Weight 700 reserved for wordmark + page titles; working labels use 600.

**Deliberate literal exceptions (not rem tokens):**
- Form controls and paste fallbacks: literal **16px** (iOS Safari anti-zoom)
- Narrow phones `@media (max-width: 380px)`: body **14px**, title **16px**
- Icon glyphs (`›` `⛶` `+` `⟳`): literal sizes (drawings, not type) — e.g. chevron **18px**
- xterm pane `fontSize`: user-pinch controlled, outside this ramp

### Named Rules
**The No-Display Rule.** Never introduce a display or serif face into Cockpit chrome.

**The Label Cadence Rule.** Uppercase + tracked labels are reserved for chrome (header, empty, settings), not for every section heading on a page.

**The No Side-Stripe Rule.** Never use `border-left` / `border-right` > 1px as a status accent — use `--tone` / section grouping.

## 4. Elevation

Mostly flat. Depth comes from the Soft Charcoal paper steps (`--soft-charcoal` → tint → raised → high; legacy aliases `--paper` / `--paper-tint` / …) and 1px rules. Shadows exist only for floating chrome.

### Shadow Vocabulary
- **Chrome lift** (`0 -6px 20px rgba(0,0,0,0.28)` / similar): Bottom nav and result panel — surfaces that sit above the scroll canvas.
- **Elev tokens** (`--elev-1`: `0 1px 2px rgba(0,0,0,0.28)`; `--elev-2`: `0 6px 20px rgba(0,0,0,0.34)`): Available for rare raised moments; do not stack them under every card.
- **Backdrop chrome**: Sticky header / bottom nav use `color-mix` + light blur — functional Safari chrome, not glassmorphism decoration.

### Named Rules
**The Flat-By-Default Rule.** Task rows and content panels stay tonal. If a shadow appears on a list card, remove it.

## 5. Components

Pill-decisive: full-radius operator actions, sparse hairline chrome, status by tone.

### Buttons
- **Shape:** Full pill (`999px`); primary tap target min-height `44px`.
- **Primary (`.action.primary` / `.pill.is-primary`):** Soft Steel Blue fill, Soft Charcoal text; hover → Soft Steel Blue Bright.
- **Secondary (`.action` / `.pill`):** Transparent + Rule Strong border; hover strengthens border to Ink Soft.
- **Remediation:** Attention Amber stroke (or fill when primary).
- **Destructive:** Fault Rose text/border; confirm state fills Fault Rose and nudges once. Drop uses a short pre-commit Undo window before the API runs.
- **Focus:** Border shift; no heavy glow rings.
- **Disabled:** `opacity: 0.4`.

### Chips / Status
- **Status dot:** 9px circle filled with `--tone`; running pulses (respects reduced motion).
- **Tone classes:** `tone-running|waiting|idle|error|ready|attention|danger|done|success|muted` set `--tone` / `--tone-bg` only.
- **Needs you:** Section grouping + tone on status text/dot — never a thick left stripe on the row.

### Cards / Containers
- **Corner Style:** Soft radius (`10px` panels, `6px` insets) — not pill.
- **Background:** Soft Charcoal Tint / Raised for list rows and sheets; never nested card-in-card chrome.
- **Shadow Strategy:** Flat at rest (see Elevation).
- **Border:** 1px Rule / Rule Strong.
- **Internal Padding:** Spacing scale 3–4 (`12–16px`); shell horizontal inset ~`20px`.

### Inputs / Fields
- **Style:** Soft Charcoal fill, Rule border, `6px` radius, **16px** type (iOS).
- **Focus:** Accent border (no large glow).
- **Error:** Fault Rose text below the field; keep the control itself calm.

### Navigation
- **Top chrome:** Sticky, blurred Soft Charcoal, hairline bottom rule; uppercase Ajax title + status line + settings pill.
- **Bottom nav:** Two-column fixed bar; **New** is Soft Steel Blue fill (sole new-task CTA); current page gets Soft Steel Blue Deep wash + accent border.
- **Connection recovery:** Retry is the sole primary; Reload / diagnostics / health are secondary.
- **Mobile:** Safe-area padding mandatory; keyboard-open and terminal-expanded modes collapse chrome so the terminal owns the band.

### Signature: Task terminal + interact strip
- Raw xterm pane is the default work surface on task detail.
- Interact panel is a flat hairline strip (`border-top` / `border-bottom` Rule) for structured Approve/Deny — never a composer that replaces the terminal.

## 6. Do's and Don'ts

### Do:
- **Do** lead every task screen with status + next safe action, then the terminal.
- **Do** use Soft Steel Blue only for primary CTA and running/selection state.
- **Do** keep actions pill-shaped with ≥44px height on touch.
- **Do** convey depth with Soft Charcoal paper steps before reaching for shadow.
- **Do** keep accents locked to CLI cockpit values (xterm 110 / 179 / 174 / 108).
- **Do** respect `prefers-reduced-motion` (collapse pulse/nudge/transitions).
- **Do** use the rem type ramp tokens for UI type; keep listed literal exceptions only.

### Don't:
- **Don't** ship generic SaaS dashboards: card grids, metric strips, soft purple/indigo chrome.
- **Don't** build overbuilt IDE shells: too many panels and tabs fighting the terminal.
- **Don't** use side-stripe borders (`border-left` / `border-right` > 1px) as status accents — use the tone system.
- **Don't** use gradient text, glass cards as decoration, or hero-metric templates.
- **Don't** put a browser composer or snapshot viewer where the raw terminal belongs.
- **Don't** invent a second status color vocabulary outside `--tone`.
- **Don't** duplicate the New-task CTA (bottom-nav New only).
