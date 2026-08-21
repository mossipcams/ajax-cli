import { useState } from "react";
import { thoughtSnippet } from "../session/public";
import { ActivityRow } from "./ToolCard";

export default function Thought({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="session-thinking" data-testid="session-thinking">
      <ActivityRow
        className="session-thinking-toggle"
        mark="∴"
        tailChars={0}
        target={thoughtSnippet(text, 90)}
        aria-label={`Thinking — ${thoughtSnippet(text, 90)}`}
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      />
      {open ? (
        <p className="session-thinking-body" data-testid="session-thinking-body">
          {text}
        </p>
      ) : null}
    </div>
  );
}

export { Thought as ReasoningRow };
