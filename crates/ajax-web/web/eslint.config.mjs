// ESLint 9 flat config for the Ajax web frontend.
//
// Syntactic rules only — no type-aware linting (no projectService / parserOptions.project).
// Type-aware rules would require the parser to load a TS program; that is out
// of scope for this slice. The typechecker contract is owned by `web:check`,
// which runs TypeScript 7 via the `typescript-7` npm alias.
//
// Four rules must ship as `error` and pass clean against existing source:
//   @typescript-eslint/no-explicit-any, react-hooks/rules-of-hooks,
//   react-hooks/exhaustive-deps, import-x/no-cycle.
//
// Enabling the jsx-a11y / testing-library / import-x / vitest rule sets (plus
// the recommended core sets) on an existing codebase surfaces pre-existing
// violations unrelated to this slice. Per the slice's escalation rule, those
// specific rules are turned `off` here with a `// slice 12 follow-up: <N>
// existing violations` marker; the violations are catalogued in
// REMAINING_RISKS so slice 12 can fix them in bulk rather than in slice 2a.
import js from "@eslint/js";
import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import jsxA11y from "eslint-plugin-jsx-a11y";
import testingLibrary from "eslint-plugin-testing-library";
import vitest from "@vitest/eslint-plugin";
import importX from "eslint-plugin-import-x";

export default tseslint.config(
  {
    ignores: ["dist/", "node_modules/"],
  },
  eslint.configs.recommended,
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    linterOptions: {
      reportUnusedDisableDirectives: "error",
    },
  },
  {
    files: ["**/*.ts", "**/*.tsx"],
    rules: {
      "@typescript-eslint/no-explicit-any": "error",
      // slice 12 follow-up: 3 existing violations
      //   e2e/terminal-behavior.test.ts:195:9 'surface' is assigned a value but never used
      //   e2e/terminal-behavior.test.ts:1259:45 'el' is defined but never used
      //   src/components/App.test.tsx:391:40 'init' is defined but never used
      "@typescript-eslint/no-unused-vars": "off",
    },
  },
  {
    files: ["**/*.tsx"],
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
    },
  },
  {
    files: ["**/*.tsx"],
    plugins: {
      "jsx-a11y": jsxA11y,
    },
    rules: {
      ...jsxA11y.flatConfigs.recommended.rules,
      // slice 12 follow-up: 1 existing violation
      //   src/components/NewTaskSheet.tsx:126:7 Non-interactive elements should not be
      //     assigned mouse or keyboard event listeners
      "jsx-a11y/no-noninteractive-element-interactions": "off",
    },
  },
  {
    files: ["**/*.test.tsx"],
    plugins: {
      "testing-library": testingLibrary,
    },
    rules: {
      ...testingLibrary.configs["flat/react"].rules,
      // slice 12 follow-up: 109 existing violations
      //   src/components/ActionBar.test.tsx:33:22 Avoid direct Node access
      //   src/components/App.test.tsx:82:22 Avoid direct Node access
      //   src/react/useSheetDrag.test.tsx:28:28 Avoid direct Node access
      "testing-library/no-node-access": "off",
      // slice 12 follow-up: 90 existing violations
      //   src/components/ActionBar.test.tsx:32:12 Avoid destructuring queries from
      //     `render` result, use `screen.getByText` instead
      //   src/components/App.test.tsx:81:12 Avoid destructuring queries from
      //     `render` result, use `screen.getByRole` instead
      //   src/components/App.test.tsx:367:11 Avoid destructuring queries from
      //     `render` result, use `screen.findByTestId` instead
      "testing-library/prefer-screen-queries": "off",
      // slice 12 follow-up: 79 existing violations
      //   src/components/ActionBar.test.tsx:33:12 Avoid using container methods
      //   src/components/App.test.tsx:82:12 Avoid using container methods
      //   src/react/useSheetDrag.test.tsx:28:18 Avoid using container methods
      "testing-library/no-container": "off",
      // slice 12 follow-up: 44 existing violations
      //   src/components/ActionBar.test.tsx:39:11 `fireEvent.click` is sync and does
      //     not need `await` operator
      //   src/components/App.test.tsx:403:11 (same rule, same shape)
      //   src/components/TestInDevPanel.test.tsx:51:11 (same rule, same shape)
      "testing-library/no-await-sync-events": "off",
      // slice 12 follow-up: 1 existing violation
      //   src/components/App.test.tsx:477:12 Use `getBy*` queries rather than
      //     `queryBy*` for checking element is present
      "testing-library/prefer-presence-queries": "off",
      // slice 12 follow-up: 1 existing violation
      //   src/components/TestInDevPanel.test.tsx:119:7 Avoid using multiple
      //     assertions within `waitFor` callback
      "testing-library/no-wait-for-multiple-assertions": "off",
    },
  },
  {
    files: ["**/*.test.{ts,tsx}"],
    plugins: {
      vitest,
    },
    rules: {
      ...vitest.configs.recommended.rules,
      // slice 12 follow-up: 4 existing violations
      //   src/components/keyboardBandPin.test.ts:46:3 Test has no assertions
      //   src/components/keyboardBandPin.test.ts:58:3 Test has no assertions
      //   src/components/keyboardBandPin.test.ts:64:3 Test has no assertions
      "vitest/expect-expect": "off",
      // slice 12 follow-up: 3 existing violations
      //   src/components/keyboardBandPin.test.ts:70:3 Test has no assertions
      //     (counted in expect-expect; this rule flags conditional expect)
      //   src/fixtures.test.ts:100:7 Avoid calling `expect` inside conditional statements
      //   src/fixtures.test.ts:107:7 Avoid calling `expect` inside conditional statements
      "vitest/no-conditional-expect": "off",
      // slice 12 follow-up: 1 existing violation
      //   src/components/keyboardBandPin.test.ts:83:40 Expect takes at most 1 argument
      "vitest/valid-expect": "off",
    },
  },
  {
    files: ["**/*.{ts,tsx}"],
    plugins: {
      "import-x": importX,
    },
    rules: {
      "import-x/no-cycle": "error",
    },
  },
  {
    // Layering, added in slice 9 once app/features/shared existed. Direction is
    // one-way: app -> features -> shared. Enforced on the @/ alias because slice 9
    // round 1 made every cross-directory import use it; a relative escape hatch
    // would need to climb out of its own folder, which the patterns below also catch.
    //
    // shared/ is the leaf: it must not know about features or the app shell.
    // Tests are exempt from layering: these rules constrain *runtime* coupling, and
    // a test that reads another layer's source text with ?raw for a source-text
    // assertion is not a runtime dependency.
    ignores: ["**/*.test.{ts,tsx}"],
    files: ["**/src/shared/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@/features/*", "@/app/*", "**/features/*", "**/app/*"],
              message:
                "shared/ is the leaf layer: it must not import from features/ or app/. Move the shared piece down, or the consumer up.",
            },
          ],
        },
      ],
    },
  },
  {
    // features/ may use shared/, but not the app shell, and not each other —
    // cross-feature coupling is what feature folders exist to prevent.
    // Tests are exempt from layering: these rules constrain *runtime* coupling, and
    // a test that reads another layer's source text with ?raw for a source-text
    // assertion is not a runtime dependency.
    ignores: ["**/*.test.{ts,tsx}"],
    files: ["**/src/features/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@/app/*", "**/app/*"],
              message:
                "features/ must not import from the app shell. Lift the shared piece into shared/.",
            },
          ],
        },
      ],
    },
  },
  {
    // Tests are exempt from layering below: these rules constrain *runtime*
    // coupling, and a test that reads another layer's source text with ?raw for a
    // source-text assertion is not a runtime dependency.
    ignores: ["**/*.test.{ts,tsx}"],
    files: ["**/src/features/task/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@/app/*", "**/app/*", "@/features/settings/*"],
              message:
                "features/task must not import from app/ or another feature. Shared pieces belong in shared/.",
            },
          ],
        },
      ],
    },
  },
  {
    // Tests are exempt from layering: these rules constrain *runtime* coupling, and
    // a test that reads another layer's source text with ?raw for a source-text
    // assertion is not a runtime dependency.
    ignores: ["**/*.test.{ts,tsx}"],
    files: ["**/src/features/settings/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@/app/*", "**/app/*", "@/features/task/*"],
              message:
                "features/settings must not import from app/ or another feature. Shared pieces belong in shared/.",
            },
          ],
        },
      ],
    },
  },
  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      // slice 12 follow-up: 11 existing violations
      //   src/components/TaskTerminal.test.tsx:64:7 Spaces are hard to count. Use {4}
      //   src/components/TaskTerminal.test.tsx:78:9 Spaces are hard to count. Use {2}
      //   src/components/keyboardBandPin.test.ts:73:9 Spaces are hard to count. Use {2}
      "no-regex-spaces": "off",
      // slice 12 follow-up: 6 existing violations
      //   src/components/TaskTerminal.tsx:420:9 'fitAddon' is never reassigned. Use 'const'
      //   src/components/TaskTerminal.tsx:458:9 'refitController' never reassigned
      //   src/components/TaskTerminal.tsx:997:5 'selectionDisposable' never reassigned
      "prefer-const": "off",
      // slice 12 follow-up: 2 existing violations
      //   e2e/swipe-reveal.test.ts:74:18 Unexpected empty object pattern
      //   e2e/terminal-behavior.test.ts:368:18 Unexpected empty object pattern
      "no-empty-pattern": "off",
      // slice 12 follow-up: 1 existing violation
      //   src/components/TaskTerminal.tsx:208:20 Unexpected control character(s)
      //     in regular expression: \x1b
      "no-control-regex": "off",
    },
  },
  {
    // Skeleton is `aria-hidden="true"` and renders plain <div>s — it is
    // deliberately absent from the accessibility tree, so no accessible query
    // can address it. The only way to satisfy these rules here is
    // `getAllByRole("generic", { hidden: true })` filtered by class name, which
    // walks every div in the document and is strictly worse than the direct
    // container query it would replace. Not a follow-up: a permanent exemption
    // for a decorative component.
    //
    // ConnectionStatus.test.tsx keeps one container query for the same reason:
    // it asserts `data-state` on `<div class="connection-status">`, a structural
    // wrapper with no role, label, or testid. Every other query in that file was
    // converted to an accessible one. Giving the wrapper an ARIA role purely to
    // satisfy a lint rule would change production markup to serve the linter —
    // the exemption is the honest option.
    // TaskDetail.test.tsx keeps two container queries for the same reason: the
    // `data-mobile-chrome` markers and the `.task-detail` scroll-lock target are
    // layout-ownership hooks with no accessible equivalent, and the cases that
    // assert them are named for those hooks specifically. A round-3 conversion
    // retargeted them at a Back button and the terminal region, which passed
    // while no longer testing what the case names claim.
    // App.test.tsx keeps container queries for route/nav ownership markers
    // (`data-outlet`, `data-bottom-route`, `data-bottom-action`) and for
    // `.update-banner`, which renders with the `hidden` attribute — `getByRole`
    // excludes hidden elements, so a role query there would change the meaning
    // of `expect(banner.hidden).toBe(true)` rather than preserve it.
    files: [
      "**/components/Skeleton.test.tsx",
      "**/components/ConnectionStatus.test.tsx",
      "**/components/TaskDetail.test.tsx",
      "**/components/App.test.tsx",
    ],
    rules: {
      "testing-library/no-container": "off",
      "testing-library/no-node-access": "off",
    },
  },
);