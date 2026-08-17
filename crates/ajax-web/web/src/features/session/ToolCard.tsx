// A tool call in the conversation. The header is always readable at a glance —
// mark, kind, title, path, state — and the body holds what the call actually
// produced: the text a command printed, the diff an edit wrote.
//
// Default collapse follows status, not preference: a call that succeeded needs
// one line, a call that failed or is still running is why the operator opened
// this surface. Once toggled, the operator's choice wins for that call.

import { useState } from "react";
import type { ToolCall, ToolContent } from "./sessionThread";
import { diffLines, shortPath, toolMark, TOOL_TONES } from "./toolPresentation";

const STATUS_LABELS: Record<ToolCall["status"], string> = {
  pending: "queued",
  in_progress: "running",
  completed: "done",
  failed: "failed",
};

function DiffBlock({ path, oldText, newText }: { path: string; oldText: string; newText: string }) {
  const lines = diffLines(oldText, newText);
  const added = lines.filter((line) => line.sign === "+").length;
  const removed = lines.filter((line) => line.sign === "-").length;
  return (
    <figure className="session-diff" data-testid="session-tool-diff">
      <figcaption className="session-diff-head">
        <span className="session-diff-path" title={path}>
          {shortPath(path)}
        </span>
        <span className="session-diff-stat">
          <span className="session-diff-added">+{added}</span>{" "}
          <span className="session-diff-removed">−{removed}</span>
        </span>
      </figcaption>
      <pre className="session-diff-body">
        {lines.map((line, index) => (
          <span key={index} className={`session-diff-line sign-${signClass(line.sign)}`}>
            {line.sign}
            {line.text}
            {"\n"}
          </span>
        ))}
      </pre>
    </figure>
  );
}

function signClass(sign: " " | "-" | "+"): string {
  if (sign === "+") return "add";
  if (sign === "-") return "del";
  return "same";
}

function ContentBlock({ content }: { content: ToolContent }) {
  if (content.type === "diff") {
    return (
      <DiffBlock
        path={content.path}
        oldText={content.oldText ?? ""}
        newText={content.newText}
      />
    );
  }
  // Execute output arrives here as text: Ajax advertises no `terminal/*` client
  // capability, so there is never an embedded terminal to render instead.
  return (
    <pre className="session-tool-output" data-testid="session-tool-output">
      {content.text}
    </pre>
  );
}

export default function ToolCard({ call }: { call: ToolCall }) {
  const settled = call.status === "completed";
  const [open, setOpen] = useState<boolean | null>(null);
  const expanded = (open ?? !settled) && call.content.length > 0;
  const tone = TOOL_TONES[call.kind] ?? "muted";
  const location = call.locations[0];

  return (
    <section
      className={`session-toolcard tone-${tone}`}
      data-testid="session-tool-card"
      data-status={call.status}
      data-kind={call.kind || "other"}
    >
      <button
        type="button"
        className="session-toolcard-head"
        // Nothing to show, nothing to toggle — but the row still states what ran.
        disabled={call.content.length === 0}
        aria-expanded={expanded}
        onClick={() => setOpen(!expanded)}
      >
        <span className="session-tool-mark" aria-hidden="true">
          {toolMark(call.kind)}
        </span>
        <span className="session-tool-kind">{call.kind || "tool"}</span>
        <span className="session-tool-title">{call.title || call.callId}</span>
        {location ? (
          <span className="session-tool-path" title={location}>
            {shortPath(location)}
          </span>
        ) : null}
        <span className="session-toolcard-status">{STATUS_LABELS[call.status]}</span>
      </button>

      {expanded ? (
        <div className="session-toolcard-body">
          {call.content.map((content, index) => (
            <ContentBlock key={index} content={content} />
          ))}
        </div>
      ) : null}
    </section>
  );
}
