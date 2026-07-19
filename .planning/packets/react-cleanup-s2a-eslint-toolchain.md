# TDD implementation packet — slice 2a: ESLint toolchain

PACKET_STATUS: READY
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED (build/tooling config only — no application source)

## Task contract

Add ESLint 9 (flat config) to the Ajax web frontend and wire it into local
verification and CI. To do that, the TypeScript dependency alias must be
inverted, because `typescript-eslint` cannot run under TypeScript 7.

Exactly one bounded outcome: **`npm run web:lint` exists, passes, and runs in
CI, while `npm run web:check` still typechecks with TypeScript 7.**

## Verified facts — do not re-litigate these

These were established empirically by the parent. Trust them.

1. `typescript-eslint@8.64.0` crashes at module load under `typescript@7.0.2`:
   `TypeError: Cannot read properties of undefined (reading 'Cjs')` from
   `@typescript-eslint/typescript-estree/dist/create-program/shared.js:59`.
   Its peer range is `typescript >=4.8.4 <6.1.0`. No release supports TS 7.
2. Making `typescript@5.9.3` the primary dependency and adding
   `typescript-7@npm:typescript@7.0.2` makes ESLint work. Verified.
3. **After the inversion, `node_modules/.bin/tsc` is TypeScript 5.9.3.**
   `node node_modules/typescript-7/bin/tsc --version` prints `7.0.2`.
4. React 19.2.7 exports a stable `useEffectEvent`.
5. `src/` and `e2e/` currently contain zero `any` — `no-explicit-any` is clean.

## Allowed files

- `package.json`
- `package-lock.json`
- `crates/ajax-web/web/eslint.config.mjs` (new)
- `crates/ajax-web/web/src/toolchain.test.ts` (new)
- `.github/workflows/ci.yml`

## Forbidden changes

- **Do not edit any file under `crates/ajax-web/web/src/` other than the new
  `toolchain.test.ts`.** No application source, no components, no hooks.
- **Do not remove or modify the two `eslint-disable-next-line
  react-hooks/exhaustive-deps` comments in `src/components/App.tsx`.** They are
  slice 2b's job. They must remain and must remain *used*.
- **Do not change the TypeScript major used by `web:check`.** `web:check` must
  execute TypeScript 7.0.2 after your change. If you cannot achieve that, stop
  and report BLOCKED.
- Do not weaken any rule to `warn` to make the run pass.
- Do not add any `eslint-disable` comment anywhere.
- Do not run `npm audit fix`, upgrade unrelated dependencies, or reformat files.
- Do not touch Rust, Playwright specs, or `vite.config.mts`.

## Task 1 — RED: guard test for the typechecker version

Create `crates/ajax-web/web/src/toolchain.test.ts`.

It must assert, by reading `package.json` from the repo root:

- the `web:check` script does **not** invoke a bare `tsc` (which would resolve
  to the hoisted TypeScript 5 binary) — it must reference the `typescript-7`
  alias path explicitly;
- a `typescript-7` devDependency exists and is an `npm:typescript@` alias
  pinned to a `7.` version;
- a `web:lint` script exists and invokes `eslint`;
- the `verify` script includes `web:lint`.

Rationale to encode in a comment: after the alias inversion `node_modules/.bin/tsc`
is TypeScript 5, so a bare `tsc` in `web:check` would silently downgrade the
typechecker with no failing signal. This test is that signal.

Run it and prove it fails **before** any other change:

```bash
npm run web:test -- --run src/toolchain.test.ts
```

Record the nonzero exit and the assertion message as RED evidence.

## Task 2 — GREEN: invert the alias and add the lint toolchain

### 2.1 `package.json` dependencies

- Change `typescript` from `^7.0.2` to `5.9.3`.
- Add `"typescript-7": "npm:typescript@7.0.2"`.
- Add devDependencies: `eslint@^9`, `typescript-eslint@^8.64.0`,
  `eslint-plugin-react-hooks@^7`, `eslint-plugin-jsx-a11y@^6`,
  `eslint-plugin-testing-library@^7`, `@vitest/eslint-plugin@^1`,
  `eslint-plugin-import-x@^4`.

Install with `npm install`. If npm reports an unavoidable peer conflict, report
BLOCKED with the exact `npm error` block — do **not** reach for `--force` or
`--legacy-peer-deps` in the committed state.

### 2.2 `package.json` scripts

- `web:check` → `node node_modules/typescript-7/bin/tsc -p crates/ajax-web/web/tsconfig.check.json --noEmit`
- add `web:lint` → `eslint --config crates/ajax-web/web/eslint.config.mjs crates/ajax-web/web/src crates/ajax-web/web/e2e`
- insert `npm run web:lint` into `verify`, immediately after `npm run web:check`.

### 2.3 `crates/ajax-web/web/eslint.config.mjs`

Flat config (ESLint 9 `export default [...]` / `tseslint.config(...)`). Required:

- `linterOptions.reportUnusedDisableDirectives: "error"`
- typescript-eslint recommended; `@typescript-eslint/no-explicit-any: "error"`
- `eslint-plugin-react-hooks` — `react-hooks/exhaustive-deps: "error"` and
  `react-hooks/rules-of-hooks: "error"` on `**/*.tsx`
- `eslint-plugin-import-x` — `import-x/no-cycle: "error"`
- `jsx-a11y` recommended on `**/*.tsx`
- `testing-library` rules scoped to `**/*.test.tsx`
- `@vitest/eslint-plugin` rules scoped to `**/*.test.{ts,tsx}`
- ignore `dist/`, `node_modules/`

**Do not** configure type-aware linting (`projectService` / `parserOptions.project`).
Syntactic rules only. Type-aware rules would need the parser to load the TS
program and are out of scope for this slice.

### 2.4 CI

In `.github/workflows/ci.yml`, in the `web` job, add a lint step **before** the
Playwright steps (so lint failures surface without waiting on browser install):

```yaml
      - name: Lint
        run: npm run web:lint
```

## Escalation rule — pre-existing violations

Enabling `jsx-a11y`, `testing-library`, `import-x`, and `vitest` rule sets on an
existing codebase will surface violations unrelated to this slice.

**Do not mass-fix them. Do not suppress them.**

If a rule reports pre-existing violations, set that specific rule to `"off"` in
the config with a `// slice 12 follow-up: <N> existing violations` comment, and
list the rule, the count, and three examples in `REMAINING_RISKS`.

These four must ship as **errors** and must pass:
`@typescript-eslint/no-explicit-any`, `react-hooks/rules-of-hooks`,
`react-hooks/exhaustive-deps`, `import-x/no-cycle`.

If any of those four cannot pass without touching application source, stop and
report BLOCKED — do not edit application source to satisfy them.

## Verification commands

Run all, record exact exit codes and output excerpts:

```bash
npm run web:test -- --run src/toolchain.test.ts
node node_modules/typescript-7/bin/tsc --version    # must print 7.0.2
npm run web:check
npm run web:lint
npm run web:test -- --run
npm run web:build:check
```

`npm run web:check` must pass with **zero** errors. If TypeScript 5 vs 7
differences appear, you have wired the wrong binary — fix the wiring, do not
fix the source.

## Stop conditions

- `web:check` cannot be made to run TypeScript 7.0.2.
- Any of the four mandatory error rules cannot pass without editing application
  source.
- npm cannot resolve the dependency set without `--force` / `--legacy-peer-deps`.
- The patch would exceed roughly 400 changed lines (excluding `package-lock.json`).
- You find yourself needing to edit `src/components/App.tsx`.
