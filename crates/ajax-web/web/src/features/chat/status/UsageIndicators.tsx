import type { Usage } from "../session/public";

export function ContextUsageMeter({ usage }: { usage: Usage }) {
  const ratio = Math.min(1, usage.used / usage.size);
  return (
    <span
      className={`session-head-quiet session-usage${ratio >= 0.9 ? " is-tight" : ""}`}
      data-testid="session-usage"
    >
      Context {Math.round(ratio * 100)}% full
    </span>
  );
}
