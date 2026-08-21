import type { PlanEntry } from "../session/public";

const PLAN_MARKS: Record<string, string> = {
  completed: "✓",
  in_progress: "▸",
  pending: "·",
};

export default function PlanChecklist({ entries }: { entries: PlanEntry[] }) {
  return (
    <ol className="session-plan" data-testid="session-plan">
      {entries.map((entry, index) => (
        <li
          key={`${index}-${entry.content}`}
          className="session-plan-step"
          data-status={entry.status}
        >
          <span className="session-plan-mark" aria-hidden="true">
            {PLAN_MARKS[entry.status] ?? "·"}
          </span>
          <span className="session-plan-text">{entry.content}</span>
        </li>
      ))}
    </ol>
  );
}
