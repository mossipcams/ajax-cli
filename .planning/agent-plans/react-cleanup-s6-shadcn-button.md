# Slice 6 — shadcn foundation and Button

Master plan: `react-migration-cleanup.md`
Depends on: slice 5 (`44bfdde`)

## Measurement before planning

- `.pill` styling lives at `styles.css:774-820` and is built entirely from Ajax
  custom properties: `--rule-strong`, `--ink`, `--ink-soft`, `--accent`,
  `--accent-bright`, `--danger`, `--danger-bright`, `--paper`, `--text-label`.
- The Tailwind `@theme inline` block (`styles.css:2140`) maps **only six** of
  those: paper, ink, accent, warn, danger, ok. `--rule-strong`, `--ink-soft`,
  `--accent-bright`, `--danger-bright`, `--text-label` are **not** exposed as
  Tailwind utilities.
- 12 ad-hoc `className="pill…"` button usages across `ResultPanel`,
  `SettingsView`, `NewTaskSheet`, `TaskLoadError`.
- Other button treatments that exist: `.settings-link` (:217), `.terminal-key`
  (:1567).

## Key decision — CVA selects Ajax classes; it does not re-implement them

Two ways to build the Button:

**(A) Emit Tailwind utilities.** Requires extending `@theme` with five more
tokens and rewriting `.pill` as utility strings. Every hover, focus-visible,
transition, and disabled state would be re-expressed by hand.

**(B) CVA maps variants onto the existing Ajax class names.** `buttonVariants`
returns `"pill"`, `"pill is-primary"`, `"pill is-danger"`. CSS stays the single
source of visual truth.

**Chosen: (B).** The program's hardest constraint is "preserve its current visual
identity" and "do not import the default shadcn appearance". (B) is
pixel-identical by construction; (A) risks silent drift on a mobile-only surface
that is awkward to verify, in exchange for no user-visible benefit.

Every structural requirement from the brief is still met by (B):
`React.ComponentProps<"button">`, `VariantProps<typeof buttonVariants>`, CVA,
`asChild` via Slot, `data-slot` / `data-variant` / `data-size`, and
`cn(buttonVariants({ variant, size, className }))`.

Re-expressing `.pill` as utilities remains available later as a token-migration
step, once more Ajax tokens are in `@theme`.

## Interpretation — "do not retain parallel .pill … styling"

Under (B) the **ad-hoc usage** disappears: no component hand-writes
`className="pill is-primary"` any more; they render `<Button variant="default">`.
The `.pill` rules remain as the variant implementation, which is where the
pixels are defined. This is the "no parallel ad hoc button styling" intent —
one owner for button appearance — without a visual rewrite.

## Variants — only those Ajax actually has

The brief lists six variants and five sizes. Ajax has distinct treatments for
three of them, plus a terminal key style:

| Variant | Ajax class |
| --- | --- |
| `default` | `pill is-primary` (filled accent — the primary action) |
| `secondary` | `pill` (hairline outline) |
| `destructive` | `pill is-danger` |

`outline` and `ghost` have **no distinct Ajax treatment**. Inventing one is new
visual design, which is out of scope; aliasing them to `secondary` is dead
flexibility. They are therefore **not created** in this slice.

`terminal` / `terminal-key` are **not created either**: the brief separately
forbids migrating terminal toolbar keys in this program unless exact behaviour
is preserved, so a variant nothing may use would be speculative. It lands with
the terminal work.

## Non-goals

- No task rows, no swipe surfaces, no xterm interaction surface, no terminal
  toolbar (all explicitly excluded by the brief).
- No CSS rewrite, no new tokens, no `@theme` changes.
- `components/ui` must hold **no** task, terminal, polling, or API behaviour.

## Delegation decision

`Delegation decision: delegated via model-router`
Packet: `.planning/packets/react-cleanup-s6-shadcn-button.md`

## Baselines

- Suite: **366 tests / 41 files**
- mobile-webkit e2e: **92 passed / 2 skipped**
- `visual.test.ts` asserts computed styles — the visual-regression guard

## Deviations

- Approach (B) rather than a Tailwind-utility rewrite (justified above).
- `outline`, `ghost`, `terminal` variants and `terminal-key` size not created.
