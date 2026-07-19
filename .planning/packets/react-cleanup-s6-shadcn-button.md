# TDD implementation packet — slice 6: shadcn foundation and Button

PACKET_STATUS: READY
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED

## Task contract

Add the shadcn primitive foundation (`cn`, `Button`) and replace the 12 ad-hoc
`className="pill…"` buttons with it. **Pixel output must not change.**

## Allowed files

- `package.json` / `package-lock.json` (four new deps, listed below)
- `crates/ajax-web/web/src/lib/utils.ts` (new)
- `crates/ajax-web/web/src/components/ui/button.tsx` (new)
- `crates/ajax-web/web/src/components/ui/button.test.tsx` (new)
- `crates/ajax-web/web/src/components/ResultPanel.tsx`
- `crates/ajax-web/web/src/components/SettingsView.tsx`
- `crates/ajax-web/web/src/components/NewTaskSheet.tsx`
- `crates/ajax-web/web/src/components/TaskLoadError.tsx`

## Forbidden changes

- **Do not edit `styles.css`.** No new CSS, no new tokens, no `@theme` changes.
  The `.pill` rules stay exactly as they are — they are the variant implementation.
- **Do not touch** `TaskList.tsx` (task rows), `TaskTerminal.tsx` (xterm surface
  and terminal keys), or anything under `e2e/`. All explicitly out of scope.
- **`components/ui` must contain no task, terminal, polling, or API behaviour.**
  No `taskHandle`, `action`, `agent`, `repo`, or `terminalConnection` props on
  the primitive.
- Do not add `eslint-disable` anywhere.
- **Never redirect command output into `/tmp`** — auto-rejected, kills your round.

## Dependencies to add

`class-variance-authority`, `clsx`, `tailwind-merge`, `@radix-ui/react-slot`.

Install with `npm install`. If npm reports an unavoidable peer conflict, report
BLOCKED with the exact error — do not use `--force` or `--legacy-peer-deps`.

## `src/lib/utils.ts` — exactly this, nothing else

```ts
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
```

Do not add other helpers to this file. Unrelated utilities live in
purpose-named modules.

## `src/components/ui/button.tsx` — required shape

Follow the shadcn Button structure concretely:

- typed as `React.ComponentProps<"button">`
- plus `VariantProps<typeof buttonVariants>`
- plus `asChild?: boolean`
- CVA for variants and sizes
- `asChild` renders through `Slot` from `@radix-ui/react-slot`
- emits `data-slot="button"`, `data-variant={variant}`, `data-size={size}`
- composes with `cn(buttonVariants({ variant, size, className }))`
- spreads remaining native button props onto the element
- **no Ajax domain behaviour**

### Variant mapping — return existing Ajax class names

CVA must select Ajax classes; it must **not** re-implement the styling:

| variant | classes |
| --- | --- |
| `default` | `pill is-primary` |
| `secondary` | `pill` |
| `destructive` | `pill is-danger` |

Default variant: `default`. Sizes: `default` only (Ajax has no distinct button
sizes today) — CVA still needs a `size` axis so `data-size` is meaningful, so
give it a single `default` entry mapping to `""`.

**Do not create `outline`, `ghost`, or `terminal` variants**, and do not create a
`terminal-key` size. Ajax has no distinct treatment for them and inventing one
is new visual design. If you believe one is required, report it — do not invent
CSS.

## Call-site migration — mapping

| Current | Becomes |
| --- | --- |
| `className="pill is-primary"` | `<Button variant="default">` |
| `className="pill"` | `<Button variant="secondary">` |
| `className="pill is-danger"` | `<Button variant="destructive">` |

Preserve every existing prop exactly: `type`, `onClick`, `disabled`,
`aria-*`, `data-*`. `type="submit"` must stay `type="submit"`.

`TaskLoadError.tsx` currently renders `className="pill"` → `variant="secondary"`.

## Task 1 — RED

`src/components/ui/button.test.tsx` covering:

1. renders a native `<button>` with `data-slot="button"`
2. `data-variant` and `data-size` reflect the props
3. each variant emits its mapped Ajax classes (`pill is-primary`, `pill`,
   `pill is-danger`)
4. `className` is merged, not replaced
5. native props pass through (`type`, `disabled`, `aria-label`, `onClick`)
6. `asChild` renders the child element instead of a `<button>` (e.g. an `<a>`)
   while keeping the classes

Prove failure first:

```bash
npm run web:test -- --run src/components/ui/button.test.tsx
```

Record the nonzero exit as RED evidence.

## Task 2 — GREEN

Implement `cn`, `Button`, then migrate the four call-site files.

## Verification commands

```bash
npm run web:test -- --run src/components/ui/button.test.tsx
npm run web:test -- --run
npm run web:check
npm run web:lint
npm run web:smoke -- --project=mobile-webkit
```

- Vitest: **41+ files**, **at least 366 tests**, zero failures.
- e2e: **92 passed**, 2 skipped, **0 failed**. `visual.test.ts` asserts computed
  styles — it passing is the proof the appearance did not change.
- `git diff --stat crates/ajax-web/web/src/styles.css` must print nothing.
- Existing component tests must pass **unmodified**; if one fails you changed
  rendered output, which this slice must not.

## Stop conditions

- An existing test would need editing.
- You need to change `styles.css` or add a token.
- A variant has no Ajax class to map to.
- Total tests drop below 366, or any test fails.
- The patch would exceed roughly 400 changed lines.
