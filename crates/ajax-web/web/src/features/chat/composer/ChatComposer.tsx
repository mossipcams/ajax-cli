import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import {
  attachComposerHotbarKeyboardRetention,
  preventComposerHotbarFocusSteal,
} from "./hotbarKeyboard";
import { autoGrow } from "./autoGrow";
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
    onComposerKeyDown,
    submitComposer,
    connected,
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

  return (
    <form
      className="session-composer"
      data-testid="session-composer"
      aria-label="Session composer"
      onSubmit={submitComposer}
    >
      {notice}
      {attachmentError ? (
        <p className="session-composer-attachment-error" role="alert">
          {attachmentError}
        </p>
      ) : null}
      {attachments.length ? (
        <ul className="session-composer-attachments" data-testid="session-composer-attachments">
          {attachments.map((attachment) => (
            <li key={attachment.id}>
              <span>{attachment.label}</span>
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
          disabled={!connected || !canAttach}
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
            !connected ||
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
          disabled={!connected || (!draft.trim() && queued === null)}
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
