import type { TurnUsage, Usage } from "../session/public";

/** Context pressure from ACP `usage_update`. Shown whenever the harness reports
 * a non-zero window; high pressure gets a warning tone at 90%+. */
export function ContextUsageMeter({ usage }: { usage: Usage }) {
  const ratio = Math.min(1, usage.used / usage.size);
  return (
    <p
      className={`session-head-quiet session-usage${ratio >= 0.9 ? " is-tight" : ""}`}
      data-testid="session-usage"
    >
      Context {Math.round(ratio * 100)}% full
    </p>
  );
}

const TURN_USAGE_FIELDS: { key: keyof TurnUsage; label: string }[] = [
  { key: "inputTokens", label: "input" },
  { key: "outputTokens", label: "output" },
  { key: "cacheReadTokens", label: "cache read" },
  { key: "cacheWriteTokens", label: "cache write" },
  { key: "totalTokens", label: "total" },
];

/** Per-turn token counts from ACP prompt results. Only present fields are
 * shown — missing counts are omitted, never rendered as zero. */
export function formatTurnUsage(turnUsage: TurnUsage): string | null {
  const parts = TURN_USAGE_FIELDS.flatMap(({ key, label }) => {
    const value = turnUsage[key];
    if (typeof value !== "number") return [];
    return [`${label} ${value.toLocaleString()}`];
  });
  if (parts.length === 0) return null;
  return `Turn tokens: ${parts.join(" · ")}`;
}
