// The settled transcript: what the agent has already done. Nothing streams
// here — live output lives in the head — so this list can hold its scroll
// position while the agent works.

import { memo } from "react";
import Markdown from "./Markdown";
import { shortPath } from "./LiveHead";
import type { ThreadEntry, ToolStatus } from "./sessionThread";

const TOOL_MARKS: Record<ToolStatus, string> = {
  pending: "○",
  in_progress: "◐",
  completed: "●",
  failed: "×",
};

const TOOL_TONES: Record<ToolStatus, string> = {
  pending: "muted",
  in_progress: "running",
  completed: "done",
  failed: "error",
};

const PLAN_MARKS: Record<string, string> = {
  completed: "●",
  in_progress: "◐",
  pending: "○",
};

const Row = memo(function Row({ entry }: { entry: ThreadEntry }) {
  if (entry.kind === "prose" && entry.role === "user") {
    return (
      <article className="session-said" data-testid="session-message-user">
        {entry.text}
      </article>
    );
  }

  if (entry.kind === "prose") {
    return (
      <article className="session-reply" data-testid="session-message-agent">
        <Markdown source={entry.text} />
      </article>
    );
  }

  if (entry.kind === "tools") {
    // Settled work only. A call still running is the head's business —
    // showing it in both places reads as a duplicated row.
    const settled = entry.calls.filter(
      (call) => call.status === "completed" || call.status === "failed",
    );
    if (!settled.length) return null;
    return (
      <div className="session-tools" data-testid="session-tools">
        {settled.map((call) => (
          <div
            key={call.callId}
            className={`session-tool tone-${TOOL_TONES[call.status]}`}
            data-status={call.status}
          >
            <span className="session-tool-mark" aria-hidden="true">
              {TOOL_MARKS[call.status]}
            </span>
            <span className="session-tool-kind">{call.kind || "tool"}</span>
            <span className="session-tool-title">{call.title || call.callId}</span>
            {call.locations[0] ? (
              <span className="session-tool-path">{shortPath(call.locations[0])}</span>
            ) : null}
          </div>
        ))}
      </div>
    );
  }

  if (entry.kind === "plan") {
    return (
      <section className="session-plan" data-testid="session-plan">
        <h2 className="session-plan-label">Plan</h2>
        <ul>
          {entry.entries.map((item, index) => (
            <li key={`${entry.id}-${index}`} data-status={item.status}>
              <span className="session-plan-mark" aria-hidden="true">
                {PLAN_MARKS[item.status] ?? "○"}
              </span>
              {item.content}
            </li>
          ))}
        </ul>
      </section>
    );
  }

  return (
    <div
      className={`session-note tone-${entry.tone === "error" ? "error" : "muted"}`}
      data-testid={`session-note-${entry.tone}`}
    >
      <span className="session-note-text">{entry.text}</span>
      {entry.body ? <pre className="session-note-body">{entry.body}</pre> : null}
    </div>
  );
});

/** Only the tail entry changes while a turn streams; every settled row above it
 * is referentially stable, so `memo` keeps a long transcript off the hot path. */
export default function Transcript({ entries }: { entries: ThreadEntry[] }) {
  return (
    <>
      {entries.map((entry) => (
        <Row key={entry.id} entry={entry} />
      ))}
    </>
  );
}
