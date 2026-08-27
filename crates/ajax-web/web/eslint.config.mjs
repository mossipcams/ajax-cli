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
        {
          assertFunctionNames: [
            "expect",
            "expectHeightBandPin",
            "expectThreadAtLiveEdge",
            "expectThreadAwayFromLiveEdge",
          ],
        },
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
    // Production layering: shared/ is the leaf and must not import app or features.
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
    ignores: ["**/*.test.{ts,tsx}"],
    files: ["**/src/app/routes/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/(chat|terminal|task|task-workspace|settings)/(?!public$).+",
              message:
                "app/routes import feature public modules only (@/features/<name>/public).",
            },
            {
              group: ["@/app/*", "**/app/*"],
              message: "app/routes must not import the app shell outside this folder.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}"],
    files: ["**/src/features/task-workspace/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/(chat|terminal|task)/(?!public$).+",
              message:
                "task-workspace imports peer features only through their public.ts modules.",
            },
            {
              regex: "^@/features/(settings|diff)/.*",
              message: "task-workspace must not import settings or diff internals.",
            },
            {
              group: ["@/app/*", "**/app/*"],
              message: "features must not import from the app shell.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}", "**/*.testHarness.tsx"],
    files: ["**/src/features/chat/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/task/(?!public$).+",
              message: "chat imports task only through @/features/task/public.",
            },
            {
              regex: "^@/features/(terminal|task-workspace|settings|diff)/.*",
              message: "chat must not import terminal, task-workspace, settings, or diff.",
            },
            {
              group: ["@/app/*", "**/app/*"],
              message: "features must not import from the app shell.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}", "**/*.testHarness.tsx"],
    files: ["**/src/features/chat/session/transport/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^\\.\\./(reducer|useChatSession|projectWireInput|model)(/|$)",
              message:
                "session/transport must not import session reducers or presentation-facing session modules.",
            },
            {
              regex: "^@/features/chat/(composer|conversation|activity|scrolling|permissions|model|status)/",
              message: "session/transport must not import Chat presentation capabilities.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}", "**/*.testHarness.tsx"],
    files: ["**/src/features/chat/session/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/chat/(composer|conversation|activity|scrolling|permissions|model|status)/",
              message: "session must not import Chat presentation capabilities.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}", "**/*.testHarness.tsx", "**/ChatSurface.tsx"],
    files: ["**/src/features/chat/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/chat/session/transport/(?!public$).+",
              message:
                "Raw transport modules stay inside session; import session/public or session/transport/public only.",
            },
            {
              regex: "^\\.\\./session/transport/(?!public$).+",
              message:
                "Raw transport modules stay inside session; import session/public or session/transport/public only.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}", "**/*.testHarness.tsx"],
    files: ["**/src/features/chat/composer/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/chat/(conversation|activity|scrolling|permissions|model|status)/",
              message: "composer must not import sibling Chat capabilities.",
            },
            {
              regex: "^\\.\\./(conversation|activity|scrolling|permissions|model|status)/",
              message: "composer must not import sibling Chat capabilities.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}", "**/*.testHarness.tsx"],
    files: ["**/src/features/chat/conversation/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/chat/(composer|scrolling|permissions|model|status)/",
              message:
                "conversation may import activity/public only; other capability imports are forbidden.",
            },
            {
              regex: "^\\.\\./(composer|scrolling|permissions|model|status)/",
              message:
                "conversation may import activity/public only; other capability imports are forbidden.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}", "**/*.testHarness.tsx"],
    files: [
      "**/src/features/chat/activity/**/*.{ts,tsx}",
      "**/src/features/chat/scrolling/**/*.{ts,tsx}",
      "**/src/features/chat/permissions/**/*.{ts,tsx}",
      "**/src/features/chat/model/**/*.{ts,tsx}",
      "**/src/features/chat/status/**/*.{ts,tsx}",
    ],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/chat/(composer|conversation|activity|scrolling|permissions|model|status)/",
              message: "Chat capabilities must not import sibling capability internals.",
            },
            {
              regex: "^\\.\\./(composer|conversation|activity|scrolling|permissions|model|status)/",
              message: "Chat capabilities must not import sibling capability internals.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}"],
    files: ["**/src/features/terminal/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/(chat|task|task-workspace|settings|diff)/.*",
              message: "terminal must not import chat, task, task-workspace, settings, or diff.",
            },
            {
              group: ["@/app/*", "**/app/*"],
              message: "features must not import from the app shell.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}"],
    files: ["**/src/features/task/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/(chat|terminal|task-workspace|settings|diff)/.*",
              message: "task must not import chat, terminal, task-workspace, settings, or diff.",
            },
            {
              group: ["@/app/*", "**/app/*"],
              message: "features must not import from the app shell.",
            },
          ],
        },
      ],
    },
  },
  {
    ignores: ["**/*.test.{ts,tsx}"],
    files: ["**/src/features/settings/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@/features/(chat|terminal|task|task-workspace|diff)/.*",
              message: "settings must not import other feature internals.",
            },
            {
              group: ["@/app/*", "**/app/*"],
              message: "features must not import from the app shell.",
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
