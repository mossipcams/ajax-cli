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
// the recommended core sets). Slice 12 cleared the deferred backlog; tests use
// accessible queries or explicit data-testid hooks — no permanent rule exemptions.
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
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],
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
      // Backdrop dialog uses click-only dismiss; Escape is owned by Radix (see NewTaskSheet).
    },
  },
  {
    files: ["**/*.test.tsx"],
    plugins: {
      "testing-library": testingLibrary,
    },
    rules: {
      ...testingLibrary.configs["flat/react"].rules,
      "testing-library/prefer-presence-queries": "error",
      "testing-library/no-wait-for-multiple-assertions": "error",
    },
  },
  {
    files: ["**/*.test.{ts,tsx}"],
    plugins: {
      vitest,
    },
    rules: {
      ...vitest.configs.recommended.rules,
      "vitest/expect-expect": [
        "error",
        { assertFunctionNames: ["expect", "expectHeightBandPin"] },
      ],
      "vitest/no-conditional-expect": "error",
      "vitest/valid-expect": "error",
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
      "no-regex-spaces": "error",
      "prefer-const": "error",
      "no-empty-pattern": "error",
      "no-control-regex": "error",
    },
  },
);
