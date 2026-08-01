import { Button } from "@/shared/ui/button";
import type { SpeechInputState } from "@/shared/lib/speechState";

export interface TerminalComposerProps {
  value: string;
  partialText: string;
  state: SpeechInputState;
  onChange: (value: string) => void;
  onInsert: (value: string) => void;
  pauseCountdownSeconds?: number;
  errorMessage?: string;
}

export default function TerminalComposer({
  value,
  partialText,
  state,
  onChange,
  onInsert,
  pauseCountdownSeconds,
  errorMessage,
}: TerminalComposerProps) {
  const insertDisabled = state === "finalizing" || state === "connecting";

  return (
    <div className="terminal-composer">
      <textarea
        aria-label="Terminal composer"
        className="terminal-composer-input"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      {partialText ? (
        <div
          data-testid="terminal-composer-partial"
          className="terminal-composer-partial"
          aria-live="polite">
          {partialText}
        </div>
      ) : (
        <div data-testid="terminal-composer-partial" className="terminal-composer-partial" />
      )}
      <div role="status" className="terminal-composer-status">
        {state === "connecting" ? <span>Connecting…</span> : null}
        {state === "listening" ? <span>Listening</span> : null}
        {state === "finalizing" ? <span>Finalizing…</span> : null}
        {state === "pause_pending" && pauseCountdownSeconds !== undefined ? (
          <>
            <span>Pausing in {pauseCountdownSeconds}…</span>
            <span>Speak to continue</span>
          </>
        ) : null}
        {state === "error" && errorMessage ? <span>{errorMessage}</span> : null}
      </div>
      <Button
        type="button"
        variant="secondary"
        disabled={insertDisabled}
        onClick={() => onInsert(value)}>
        Insert transcript
      </Button>
    </div>
  );
}
