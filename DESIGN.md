---
name: Ajax Cockpit
description: Mobile-first operator console for a local AI agent fleet — status, decision, chat, terminal.
colors:
  soft-charcoal: "#1c1c1c"
  soft-charcoal-tint: "#232323"
  soft-charcoal-raised: "#2a2a2a"
  soft-charcoal-high: "#323232"
  ink: "#e6e6e6"
  ink-bright: "#f5f5f5"
  ink-soft: "#c6c6c6"
  ink-muted: "#a8a8a8"
  ink-faint: "#808080"
  rule: "#2a2a2a"
  rule-strong: "#454545"
  soft-steel-blue: "#85bcf5"
  soft-steel-blue-bright: "#a9d6ff"
  soft-steel-blue-deep: "#192f46"
  attention-amber: "#ddae6c"
  attention-amber-bright: "#f0ca96"
  fault-rose: "#eb827b"
  fault-rose-bright: "#fda19a"
  fault-rose-deep: "#442322"
  done-sage: "#82cb9b"
  done-sage-bright: "#a7e1ba"
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
    backgroundColor: "{colors.ink}"
    textColor: "{colors.soft-charcoal}"
    rounded: "{rounded.sm}"
    padding: "10px 16px"
    height: "44px"
  button-primary-hover:
    backgroundColor: "{colors.ink-bright}"
    textColor: "{colors.soft-charcoal}"
    rounded: "{rounded.sm}"
    padding: "10px 16px"
    height: "44px"
  button-secondary:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.sm}"
    padding: "10px 16px"
    height: "44px"
  button-destructive:
    backgroundColor: "transparent"
    textColor: "{colors.fault-rose}"
    rounded: "{rounded.sm}"
    padding: "10px 16px"
    height: "44px"
  button-destructive-confirm:
    backgroundColor: "{colors.fault-rose}"
    textColor: "{colors.soft-charcoal}"
    rounded: "{rounded.sm}"
    padding: "10px 16px"
    height: "44px"
  pill-primary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.soft-charcoal}"
    rounded: "{rounded.sm}"
    padding: "8px 16px"
    height: "34px"
  input-field:
    backgroundColor: "{colors.soft-charcoal}"
    textColor: "{colors.ink}"
    rounded: "{rounded.sm}"
    padding: "10px 12px"
    typography: "{typography.input}"
  nav-new-task:
    backgroundColor: "{colors.ink}"
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

Ajax is the product and the visual brief: a pocket operator console for a local agent fleet. Ink-on-charcoal chrome, hue reserved for task status, and a chat conversation that owns the work. Chrome is sparse and uses one shape language: 6px corners, hairline rules, and status carried by a single tone palette. Chrome is status, required decision, then the conversation; the terminal is one tab away for the raw tmux/xterm bridge. Mobile-first for Safari on iPhone.

Density stays operational, not decorative. Tone carries meaning (`running` / `waiting` / `error` / `done`); blue, amber and sage share one OKLCH lightness and chroma (L≈0.78, C≈0.10); Fault Rose sits slightly darker and more saturated (L≈0.72, C≈0.13) so errors read as red, not salmon, on the CLI-locked charcoal neutrals. Motion is short state feedback (≈140–220ms), never page choreography. The system rejects generic SaaS dashboards (card grids, metric strips, soft purple/indigo chrome) and overbuilt IDE shells that fight the conversation.

**Key Characteristics:**
- Soft Charcoal stack with hairline rules; depth mostly from paper steps
- Ink fill with Soft Charcoal text as primary action; Soft Steel Blue for running / live / selection; semantic amber / rose / sage for wait / fault / done
- 6px corners on every control and panel; round only for status dots
- Uppercase tracked labels only for eyebrows and stat labels
- Shell width capped at 560px; iOS safe-area and keyboard geometry are first-class
- Chat is the default work surface; Terminal is a tab and an escape hatch
- Fixed rem type ramp in CSS (`--text-micro` … `--text-display`); no Inter in the stack

## 2. Colors

Restrained dark console palette: neutrals do the work, and hue speaks only for task status. Charcoal surfaces stay in lockstep with `ajax-tui`. Blue, amber and sage share one OKLCH lightness and chroma (L≈0.78, C≈0.10); Fault Rose sits slightly darker and more saturated (L≈0.72, C≈0.13) so errors read as red, not salmon.

### Primary
- **Soft Steel Blue** (`#85bcf5`): Running, live, and selection signal only — never a CTA or chrome fill. Bright (`#a9d6ff`) for emphasis; deep (`#192f46`) reserved for rare tinted running/selection washes.

### Secondary
- **Attention Amber** (`#ddae6c`): Waiting / needs-input / remediation. Bright (`#f0ca96`) for ready-for-review emphasis.

### Tertiary
- **Fault Rose** (`#eb827b`): Errors and destructive intents. Bright (`#fda19a`) on confirm/hover fill; deep (`#442322`) for destructive borders at rest.
- **Done Sage** (`#82cb9b`): Success / done. Bright (`#a7e1ba`) when a brighter success cue is needed.

### Neutral
- **Soft Charcoal** (`#1c1c1c`): Body / app paper, one step lighter than the `#161616` terminal pane. Keep the ladder neutral (R=G=B): the 2026-10 Apple grays (`#1c1c1e`) read as a muddy blue-violet cast.
- **Soft Charcoal Tint / Raised / High** (`#232323` / `#2a2a2a` / `#323232`): Nested surfaces and hover washes — the main depth ladder. Tint is the panel surface.
- **Ink** (`#e6e6e6`): Primary text, primary-button fill, and the New-task fill. Bright (`#f5f5f5`) on hover/focus of those fills. Soft / muted / faint (`#c6c6c6` / `#a8a8a8` / `#808080`) for secondary text, status line, timestamps and empty chrome.
- **Rule / Rule Strong** (`#2a2a2a` / `#454545`): Hairline dividers, panel borders, and resting control borders.

### Named Rules
**The One Tone Rule.** Status is a single `--tone` / `--tone-bg` pair consumed by dots, badges, rows, and interaction controls. Never invent a parallel status color path.

**The Accent Rarity Rule.** Soft Steel Blue appears only for running, live, and selection state. Semantic amber / rose / sage appear only for task status or a destructive/remediation intent — never as decorative fill across idle chrome.

**The Color Means Status Rule.** Chrome and CTAs are ink and charcoal. Hue appears only to show task state (`running` / `waiting` / `error` / `done`); a primary button is never blue, and a status-free surface is never tinted.

## 3. Typography

**Display Font:** none (product UI — no display face)
**Body Font:** UI sans (`ui-sans-serif`, Avenir Next, Helvetica Neue, system-ui) — Inter removed
**Label/Mono Font:** `ui-monospace, SFMono-Regular, SF Mono, Menlo, Monaco, Consolas, monospace` for code, commands, paths, hardware tags and terminal output

**Character:** One technical sans for all chrome, titles and conversation text; mono only where the CLI would speak. Fixed rem product scale, not fluid marketing type.

### Hierarchy (CSS tokens)

| Token | rem | px | Role |
| --- | --- | --- | --- |
| `--text-micro` | 0.65625rem | 10.5 | Eyebrows, stat labels, subheads |
| `--text-label` | 0.75rem | 12 | Buttons, chips, nav, labels, row timestamps |
| `--text-data` | 0.78125rem | 12.5 | Status line, tabular data |
| `--text-body-sm` | 0.8125rem | 13 | Summaries, notes, context lines, toast |
| `--text-body` | 0.9375rem | 15 | Base UI copy and task titles |
| `--text-heading` | 1rem | 16 | Section headings and stat values |
| `--text-title` | 1.0625rem | 17 | Cockpit wordmark |
| `--text-display` | 1.25rem | 20 | Settings page heading |

**Tracking:** `--tracking-display` `0.18em` (wordmark / empty chrome); `--tracking-label` `0.08em` (uppercase eyebrows and stat labels only). Weight 700 reserved for wordmark + page titles; working labels use 600.

**Numbers and times** use `font-variant-numeric: tabular-nums` so columns of counts, durations and timestamps line up.

**Deliberate literal exceptions (not rem tokens):**
- Form controls and paste fallbacks: literal **16px** (iOS Safari anti-zoom)
- Narrow phones `@media (max-width: 380px)`: body **14px**, title **16px**
- Icon glyphs (`›` `⛶` `+` `⟳`): literal sizes (drawings, not type) — e.g. chevron **18px**
- xterm pane `fontSize`: user-pinch controlled, outside this ramp

### Named Rules
**The No-Display Rule.** Never introduce a display or serif face into Cockpit chrome.

**The Label Cadence Rule.** Uppercase and tracked labels are reserved for eyebrows and stat labels. Section headings, task titles, buttons and back controls are sentence case. The Ajax wordmark is the one uppercase display title.

**The Mono Rule.** Monospace is for code, commands, paths, hardware tags and terminal output. Prose, titles, labels and buttons are never mono.

**The No Side-Stripe Rule.** Never use `border-left` / `border-right` > 1px as a status accent — use `--tone` / section grouping.

## 4. Elevation

Mostly flat. Depth comes from the Soft Charcoal paper steps (`--soft-charcoal` → tint → raised → high; legacy aliases `--paper` / `--paper-tint` / …) and 1px rules. Shadows exist only for floating chrome.

### Shadow Vocabulary
- **Chrome lift** (`0 -6px 20px rgba(0,0,0,0.28)` / similar): Bottom nav and result panel — surfaces that sit above the scroll canvas.
- **Elev tokens** (`--elev-1`: `0 1px 2px rgba(0,0,0,0.28)`; `--elev-2`: `0 6px 20px rgba(0,0,0,0.34)`): Available for rare raised moments; do not stack them under every card.
- **Backdrop chrome**: Sticky header / bottom nav use `color-mix` + light blur — functional Safari chrome, not glassmorphism decoration.

### Named Rules
**The Flat-By-Default Rule.** Task rows and content panels stay tonal with no shadow. If a shadow appears on a list card or panel, remove it.

## 5. Components

Square-shouldered and calm: 6px corners on every operator control, sparse hairline chrome, status by tone. Round shapes appear only for status dots.

### Shape
- **Default radius:** `--radius-sm` (6px) for buttons, chips, task rows, panels, inputs and stat tiles.
- **Sheet top corners:** `--radius` (10px), and only there. Nothing else exceeds 6px.
- **Round:** status dots only. No pill (`999px`) buttons or chips, and no 20px panels.

### Buttons
- **Shape:** 6px radius (`--radius-sm`); primary tap target min-height `44px`.
- **Primary (`.action.primary` / `.pill.is-primary`):** Ink fill, Soft Charcoal text; hover → Ink Bright.
- **Secondary (`.action` / `.pill`):** Transparent + Rule Strong border; hover strengthens border to Ink Soft.
- **Remediation:** Attention Amber stroke (or fill when primary).
- **Destructive:** Fault Rose text/border; confirm state fills Fault Rose and nudges once. Drop uses a short pre-commit Undo window before the API runs.
- **Focus:** Border shift; no heavy glow rings.
- **Disabled:** transparent fill, Rule border, Ink Faint text, full opacity. A disabled control reads as unavailable, not as faded.

### Chips / Status
- **Status dot:** 9px circle filled with `--tone`, 7px in the dashboard list; running pulses (respects reduced motion). The only round element.
- **Tone classes:** `tone-running|waiting|idle|error|ready|attention|danger|done|success|muted` set `--tone` / `--tone-bg` only.
- **Filter chips:** 6px radius, transparent with a Rule border. The selected chip takes Soft Charcoal High fill, Ink Bright text and a Rule Strong border.
- **Needs you:** Section grouping by attention band, plus tone on the status dot and on waiting/error label text — never a thick left stripe on the row.

### Panels / Containers
- **Corner Style:** 6px (`--radius-sm`) for panels, rows and insets. Not pill, not 20px.
- **Background:** Soft Charcoal Tint (`--paper-tint`) for panels, list rows and sheets; never nested card-in-card chrome.
- **Heading:** Each panel opens with a section heading plus one muted context line (`--text-body-sm`, Ink Muted).
- **Border:** 1px Rule.
- **Shadow Strategy:** None on panels; flat at rest (see Elevation).
- **Stat tiles:** Allowed only where the numbers are live operational data (queue counts, loss, durations). A tile is a Rule-bordered 6px box on Soft Charcoal Raised with an uppercase tracked stat label and a `--text-heading` value in Ink Bright with tabular numerals. Stat tiles are never a decorative strip or a grid of identical cards.
- **Internal Padding:** Spacing scale 3–4 (`12–16px`); shell horizontal inset ~`20px`.

### Task identity
A task looks the same on every screen: the dashboard row, the chat head, the terminal header and the task details sheet all show the same three pieces.
- **Title:** sans. `--text-body` at weight 400 in lists, `--text-heading` at weight 600 as the page title. Never mono, never uppercase.
- **List tones:** the dashboard list softens `--tone` to pastels, scoped to `.tasks`.
- **Status:** a dot in `--tone` plus a sentence-case label (`Running`, `Needs input`, `Error`, `Done`). The label is neutral (Ink Muted) and takes `--tone` only for waiting and error. No glyph-and-uppercase variant.
- **Back:** neutral Ink text, sans, not uppercase. The same treatment on every screen, never blue and never tinted by status.

### Inputs / Fields
- **Style:** Soft Charcoal fill, Rule border, `6px` radius, **16px** type (iOS).
- **Focus:** Accent border (no large glow).
- **Error:** Fault Rose text below the field; keep the control itself calm.

### Navigation
- **Top chrome:** Sticky, blurred Soft Charcoal, hairline bottom rule; Ajax wordmark, status line and a settings control.
- **Bottom nav:** Two-column fixed bar. **New** is the filled, sole new-task CTA; the current page is a raised neutral (`--paper-raised`, no border, Ink text).
- **Connection recovery:** Retry is the sole primary; Reload / diagnostics / health are secondary.
- **Mobile:** Safe-area padding mandatory; keyboard-open and terminal-expanded modes collapse chrome so the active pane owns the band.

### Dashboard task list
- One soft panel (`--paper-tint`, no border, 6px radius) with faint hairline separators between rows. No shadow, no card per task.
- A **Needs you** group heading sits above rows in core's `needs-you` and `review` attention bands. An **Other tasks** heading sits above the rest, and appears only when both groups exist.
- Group headings stay quiet: `--text-label`, weight 400, below the row titles in the hierarchy.
- Grouping reads each row's `attention` band, not its status tone. Order inside each group is the task list's own order; the browser adds no ranking policy.
- Row timestamps use Ink Muted.

### Signature: Task chat
Ajax Chat is the default task surface. Opening a task lands on the conversation, and the composer is the work surface for talking to the agent. Chat is on by default and can be turned off in Settings.

The head sits above the conversation and carries the live state: task identity, the running tool and any pending decision. Permission *buttons* stay in the head, never inline: a control in a scrolling column can leave the screen mid-decision on a phone.

The conversation is **turn-as-chapter**: each operator prompt opens a turn holding the operator's message, one activity disclosure, and the agent's answer. Only that — plus an unanswered permission ask, an error, and a hairline divider for cancellations, reconnects, harness switches and context resets — is allowed in the column. Agent answers render as Markdown and are revealed a completed paragraph at a time (never token by token, never split inside a fence); the whole response lands when the turn ends.

The disclosure is where ACP's separation is kept. Collapsed, it reads out the current operation while the turn runs (`Running cargo test…`, replaced rather than appended, a completed call leaving it the moment ACP says so) and a counted summary once the turn settles (`Read 6 files · edited 2 files · ran 4 commands · 38s`). Opened, it is the full timeline: reasoning as one collapsed line per thought, tool calls as cards that revise in place, the plan as a checklist, a marker row where permission was asked. It opens itself for failure, blocked and approval-required work; a manual open or close by the operator wins from then on. While a turn is in flight the head's tool row and any in-progress plan step may appear together; with no tool or plan step the head shows the latest ACP thought, else `Thinking…` before anything arrives — not a timer placeholder or human status detail line. The 60s `No recent activity` label remains a freshness warning only (#889). Tool calls use a mono mark column (glyph per kind, no icon library); the visible kind word is omitted while `data-kind` stays for tests. A tool call's output — a command's text, a file diff — is the substance of a turn and stays in the record one tap away; a summary line *in its place* threw it away, and a full trace in the column buried the answer.

Typography follows what is being said, not where it sits: conversation text and tool labels are proportional, and monospace is reserved for code, commands, paths and output. The operator's own messages take a raised surface with a hairline — a side and a step, not a saturated fill that shouts over the answer. A follow-up typed while a turn runs is queued as a muted `Queued` message that stays editable; the composer action names what Enter does next (`Send` / `Queue` / `Stop & send`).

The Task details sheet is a thin operator dossier, not a settings dump. Its first viewport leads with task identity (title, handle, branch), the current session model with a Change disclosure (shortlist of popular options plus **Show all** for the rest — not the full harness catalog in the first viewport), and the Terminal tab reachable without scrolling past the model list. Deeper registry fields reuse the shared `TaskMetaDetails` grid; Rust Debug annotation strings are shown as human Notes lines, not raw struct dumps.

On the terminal route (`#/t/<handle>`), the header Details sheet and footer Task details disclosure lead with **Ajax chat** in a pinned primary tools row (sibling of the sheet header, not inside the scrolling body) for session-capable tasks. The sheet uses contained overflow with `.session-details-body` as the bounded scroller so Ajax chat stays in the first viewport on iOS. While `html.terminal-expanded`, the header Details control remains reachable (overlay, not band space) even though back/title/status chrome is hidden.

### Terminal: a tab and an escape hatch
- The raw xterm pane is one tab on the task page, opened on demand for the tmux-backed session. It is not the default view and it is not the signature surface.
- When the terminal is open it owns the band, with the same task identity above it. Chrome collapses around it on small screens.
- The interact strip under the terminal is a flat hairline strip (`border-top` / `border-bottom` Rule) for structured Approve/Deny. It never replaces the pane with a composer. The terminal is frozen, so its toolbar and overlay controls keep their existing pill shape and are the one exception to the Shape rules.

## 6. Do's and Don'ts

### Do:
- **Do** lead every task screen with status + next safe action, then the conversation.
- **Do** keep chrome and CTAs in Ink and charcoal; use Soft Steel Blue only for running, live, and selection state.
- **Do** keep every control and panel on the 6px radius, with ≥44px height on touch for primary actions.
- **Do** give every panel a section heading and one muted context line.
- **Do** keep uppercase tracking to eyebrows and stat labels; set titles, labels and buttons in sentence case.
- **Do** use tabular numerals for counts, durations and times.
- **Do** convey depth with Soft Charcoal paper steps before reaching for shadow.
- **Do** keep the status palette matched: blue, amber and sage share one OKLCH lightness and chroma (L≈0.78, C≈0.10); Fault Rose sits slightly darker and more saturated (L≈0.72, C≈0.13) so errors read as red, not salmon; neutrals stay locked to the CLI cockpit charcoal/ink values.
- **Do** respect `prefers-reduced-motion` (collapse pulse/nudge/transitions).
- **Do** use the rem type ramp tokens for UI type; keep listed literal exceptions only.

### Don't:
- **Don't** ship generic SaaS dashboards: card grids, metric strips, soft purple/indigo chrome, or hero-metric templates. Stat tiles are for live operational numbers only.
- **Don't** build overbuilt IDE shells: too many panels and tabs fighting the conversation.
- **Don't** use pill shapes or corners above 6px, except the 10px sheet top corners. No `999px` buttons or chips.
- **Don't** use side-stripe borders (`border-left` / `border-right` > 1px) as status accents — use the tone system.
- **Don't** use gradient text, glass cards as decoration, or hero-metric templates.
- **Don't** make the terminal the default task view, or put a snapshot viewer in its place. Chat is the default; the terminal is a tab.
- **Don't** use uppercase tracking on section headings, task titles, buttons or back controls.
- **Don't** set prose, titles or buttons in monospace.
- **Don't** invent a second status color vocabulary outside `--tone`.
- **Don't** duplicate the New-task CTA (bottom-nav New only).
