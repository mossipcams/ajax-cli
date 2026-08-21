import type { ReactNode } from "react";
import { useComposerContext } from "./useComposer";

export type ChatComposerProps = {
  notice?: ReactNode;
  modelControl?: ReactNode;
};

export default function ChatComposer({ notice = null, modelControl = null }: ChatComposerProps) {
  const {
    draft,
    composerRef,
    queued,
    submitLabel,
    speechModel,
    micAriaLabel,
    micArmed,
    toggleMic,
    onDraftChange,
    onKeyDown,
    submitComposer,
    connected,
    everOpened,
    busy,
  } = useComposerContext();

  return (
    <form
      className="session-composer"
      data-testid="session-composer"
      aria-label="Session composer"
      onSubmit={submitComposer}
    >
      {notice}
      <div className="session-composer-row">
        <textarea
          rows={1}
          enterKeyHint="send"
          placeholder={
            !connected
              ? everOpened
                ? "Reconnecting…"
                : "Starting…"
              : queued !== null
                ? "Enter stops this turn and sends…"
                : busy
                  ? "Queues after this turn…"
                  : "Message…"
          }
          aria-label="Message"
          ref={composerRef}
          value={draft}
          onChange={(e) => {
            const next = e.target.value;
            onDraftChange(next, next.length < draft.length);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              onKeyDown(e.key, e.shiftKey);
            }
          }}
        />
        <div className="session-composer-actions">
          {modelControl}
          <button
            type="button"
            className={`session-composer-button session-composer-mic${micArmed ? " is-armed" : ""}${speechModel.state === "connecting" ? " is-connecting" : ""}`}
            aria-label={micArmed ? "Stop voice input" : micAriaLabel}
            title={micArmed ? "Stop voice input" : micAriaLabel}
            disabled={
              !connected ||
              speechModel.state === "connecting" ||
              speechModel.state === "finalizing"
            }
            onClick={toggleMic}
          >
            Mic
          </button>
          <button
            type="submit"
            className="session-composer-button session-composer-send"
            aria-label={submitLabel}
            disabled={!connected || (!draft.trim() && queued === null)}
          >
            {submitLabel}
          </button>
        </div>
      </div>
      {speechModel.errorMessage || speechModel.state === "listening" ? (
        <p className="session-speech-status" role="status" aria-live="polite">
          {speechModel.errorMessage ?? "Listening…"}
        </p>
      ) : null}
    </form>
  );
}
