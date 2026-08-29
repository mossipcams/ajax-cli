import { useState, useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import ResultPanel from "@/shared/ui/ResultPanel";
import { Button } from "@/shared/ui/button";
import {
  attachmentsArePreparing,
  hasComposerPromptContent,
} from "@/shared/lib/promptContent";
import {
  attachComposerHotbarKeyboardRetention,
  preventComposerHotbarFocusSteal,
} from "./hotbarKeyboard";
import { autoGrow } from "./autoGrow";
import { useComposerContext } from "./useComposer";

export type ChatComposerProps = {
  notice?: ReactNode;
  modelControl?: ReactNode;
  contextUnavailable?: boolean;
  contextError?: string;
  transcriptError?: string;
  onRetryRestore?: () => void;
  onStartNewContext?: () => void;
};

const DEFAULT_CONTEXT_UNAVAILABLE_MESSAGE =
  "This session context could not be restored. Retry restore or start a new context to continue.";

export default function ChatComposer({
  notice = null,
  modelControl = null,
  contextUnavailable = false,
  contextError,
  transcriptError,
  onRetryRestore,
  onStartNewContext,
}: ChatComposerProps) {
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
    onComposerKeyDown,
    submitComposer,
    connected,
    promptingEnabled,
    everOpened,
    busy,
    slashMatches,
    slashMenuOpen,
    slashSelection,
    insertSlashMatch,
    attachments,
    removeAttachment,
    attachFiles,
    onComposerPaste,
    attachInputRef,
    attachAccept,
    canAttach,
    attachmentError,
    draftRestoreGeneration,
  } = useComposerContext();

  const hotbarRef = useRef<HTMLDivElement>(null);
  const [confirmStartNewContext, setConfirmStartNewContext] = useState(false);

  useEffect(() => {
    if (!contextUnavailable) setConfirmStartNewContext(false);
  }, [contextUnavailable]);

  useLayoutEffect(() => {
    if (draftRestoreGeneration === 0) return;
    const node = composerRef.current;
    if (!node) return;
    autoGrow(node, true);
  }, [composerRef, draftRestoreGeneration]);

  useEffect(() => {
    const hotbar = hotbarRef.current;
    if (!hotbar) return;
    return attachComposerHotbarKeyboardRetention(hotbar);
  }, []);

  const canPrompt = connected && promptingEnabled;
  const contextNotice = contextUnavailable
    ? (contextError?.trim() || DEFAULT_CONTEXT_UNAVAILABLE_MESSAGE)
    : null;
  const transcriptNotice = transcriptError?.trim() || null;

  return (
    <form
      className="session-composer"
      data-testid="session-composer"
      aria-label="Session composer"
      onSubmit={submitComposer}
    >
      {notice}
      {contextUnavailable && contextNotice ? (
        <div
          className="session-config-notice"
          data-testid="session-context-notice"
          role="alert"
        >
          <p>{contextNotice}</p>
          <div className="session-context-recovery-actions" data-testid="session-context-recovery">
            <Button type="button" variant="default" onClick={() => onRetryRestore?.()}>
              Retry restore
            </Button>
            <Button type="button" variant="secondary" onClick={() => setConfirmStartNewContext(true)}>
              Start new context
            </Button>
          </div>
        </div>
      ) : null}
      {!contextUnavailable && transcriptNotice ? (
        <div
          className="session-config-notice"
          data-testid="session-transcript-notice"
          role="alert"
        >
          <p>{transcriptNotice}</p>
        </div>
      ) : null}
      {confirmStartNewContext ? (
        <ResultPanel
          message="Start a new context? The visible transcript stays, but the agent will not remember prior turns."
          onConfirm={() => {
            onStartNewContext?.();
            setConfirmStartNewContext(false);
          }}
          onCancelConfirm={() => setConfirmStartNewContext(false)}
          onDismiss={() => setConfirmStartNewContext(false)}
        />
      ) : null}
      {attachmentError ? (
        <p className="session-composer-attachment-error" role="alert">
          {attachmentError}
        </p>
      ) : null}
      {attachments.length ? (
        <ul className="session-composer-attachments" data-testid="session-composer-attachments">
          {attachments.map((attachment) => (
            <li
              key={attachment.id}
              data-status={attachment.status}
              className={
                attachment.status === "error" ? "session-composer-attachment-error-chip" : undefined
              }
            >
              <span>
                {attachment.label}
                {attachment.status === "preparing" ? " (Preparing…)" : null}
                {attachment.status === "error" && attachment.error ? ` — ${attachment.error}` : null}
              </span>
              <button
                type="button"
                className="session-composer-attachment-remove"
                aria-label={`Remove ${attachment.label}`}
                onClick={() => removeAttachment(attachment.id)}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {slashMenuOpen ? (
        <ul
          className="session-composer-slash-menu"
          data-testid="session-composer-slash-menu"
          role="listbox"
          aria-label="Slash commands"
        >
          {slashMatches.map((command, index) => (
            <li key={command.name}>
              <button
                type="button"
                role="option"
                aria-selected={index === slashSelection}
                className={index === slashSelection ? "is-selected" : undefined}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => insertSlashMatch(command)}
              >
                <span className="session-composer-slash-name">/{command.name}</span>
                <span className="session-composer-slash-description">{command.description}</span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <div
        ref={hotbarRef}
        className="session-composer-hotbar"
        data-testid="session-composer-hotbar"
      >
        <input
          ref={attachInputRef}
          type="file"
          className="session-composer-attach-input"
          accept={attachAccept}
          multiple
          aria-hidden="true"
          tabIndex={-1}
          onChange={(event) => {
            const files = event.target.files;
            if (files?.length) void attachFiles(files);
            event.target.value = "";
          }}
        />
        {modelControl}
        <div className="session-composer-actions" data-testid="session-composer-actions">
        <button
          type="button"
          className="session-composer-button session-composer-attach"
          aria-label="Attach"
          disabled={!canPrompt || !canAttach}
          hidden={!canAttach}
          onMouseDown={preventComposerHotbarFocusSteal}
          onClick={() => attachInputRef.current?.click()}
        >
          <svg
            className="session-composer-icon"
            viewBox="0 0 24 24"
            width="20"
            height="20"
            aria-hidden="true"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-1.66-2.2A2 2 0 0 0 9.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
          </svg>
        </button>
        <button
          type="button"
          className={`session-composer-button session-composer-mic${micArmed ? " is-armed" : ""}${speechModel.state === "connecting" ? " is-connecting" : ""}`}
          aria-label={micArmed ? "Stop voice input" : micAriaLabel}
          title={micArmed ? "Stop voice input" : micAriaLabel}
          disabled={
            !canPrompt ||
            speechModel.state === "connecting" ||
            speechModel.state === "finalizing"
          }
          onMouseDown={preventComposerHotbarFocusSteal}
          onClick={toggleMic}
        >
          Mic
        </button>
        <button
          type="submit"
          className="session-composer-button session-composer-send"
          aria-label={submitLabel}
          disabled={
            !canPrompt ||
            attachmentsArePreparing(attachments) ||
            (queued === null && !hasComposerPromptContent(draft, attachments))
          }
          onMouseDown={preventComposerHotbarFocusSteal}
        >
          <svg
            className="session-composer-icon"
            viewBox="0 0 24 24"
            width="20"
            height="20"
            aria-hidden="true"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M12 19V5" />
            <path d="M5 12l7-7 7 7" />
          </svg>
        </button>
        </div>
      </div>
      <textarea
        rows={1}
        enterKeyHint="send"
        placeholder={
          !connected
            ? everOpened
              ? "Reconnecting…"
              : "Starting…"
            : !promptingEnabled
              ? transcriptNotice
                ? "Transcript unavailable…"
                : "Context unavailable…"
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
        onKeyDown={onComposerKeyDown}
        onPaste={onComposerPaste}
      />
      {speechModel.errorMessage || speechModel.state === "listening" ? (
        <p className="session-speech-status" role="status" aria-live="polite">
          {speechModel.errorMessage ?? "Listening…"}
        </p>
      ) : null}
    </form>
  );
}
