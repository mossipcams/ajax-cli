import { defineConfig, enforceTdd, forbidCommandPattern } from "@nizos/probity";

export default defineConfig({
  rules: [
    forbidCommandPattern({
      match: /git\s+reset\s+--hard/,
      reason: "Do not reset away local work; inspect status and preserve user changes.",
    }),
    forbidCommandPattern({
      match: /git\s+checkout\s+--/,
      reason: "Do not discard local file changes without explicit user approval.",
    }),
    forbidCommandPattern({
      match: /rm\s+-rf/,
      reason: "Avoid broad recursive removal; remove specific generated paths only after inspection.",
    }),
    {
      files: ["crates/**/*.rs", "crates/**/tests/**/*.rs", "scripts/**/*.sh"],
      rules: [
        enforceTdd({
          instructions: (defaults) => `${defaults}

Ajax project rules:
- Read architecture.md before architectural analysis, planning, or code changes.
- Write a failing behavior test before changing code.
- For each task, run the focused test and show the failure before implementation.
- Implement the smallest change needed to pass the failing behavior test.
- Never modify files in tests/ unless the approved plan names those test files.
- Never delete or weaken test assertions.
- After each task, ask exactly: "Task N done. Continue?" unless the user approved finishing all tasks.`,
          maxEvents: 16,
        }),
      ],
    },
  ],
});
