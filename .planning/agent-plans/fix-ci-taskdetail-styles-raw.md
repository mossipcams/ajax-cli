# Fix CI: TaskDetail.test.ts node types

## Failure

Web job `npm run web:check` fails:

- `Cannot find type definition file for 'node'`
- `Cannot find module 'node:fs'` / `node:path`

Cause: P0 test loads `styles.css` via `readFileSync` + `/// <reference types="node" />`, but web `tsconfig.json` only includes `vitest/globals` and `@testing-library/jest-dom` (no `@types/node`).

## Fix

Replace `node:fs`/`node:path` with `import stylesSource from "../styles.css?raw"` (same pattern as other `?raw` source contracts). Drop the node reference and `loadStylesSource` helper.

## Validation

```bash
npm run web:check
npm run web:test -- --run TaskDetail.test.ts
```

## Approval

User reported CI failed — treat as authorization to fix and push to the open PR.
