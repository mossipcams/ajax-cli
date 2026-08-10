interface CopyFallbackProps {
  open: boolean;
  text: string;
  onDone: () => void;
}

interface PasteFallbackProps {
  open: boolean;
  notice: string;
  text: string;
  onTextChange: (text: string) => void;
  onSend: () => void;
  onCancel: () => void;
}

/** Clipboard unavailable sheets for copy/paste fallbacks. */
export function TaskTerminalClipboardSheets({
  copy,
  paste,
}: {
  copy: CopyFallbackProps;
  paste: PasteFallbackProps;
}) {
  return (
    <>
      {copy.open ? (
        <div className="terminal-paste-fallback">
          <p className="terminal-paste-notice" role="status">
            Clipboard unavailable — copy below.
          </p>
          <textarea
            className="terminal-paste-input"
            readOnly
            aria-label="Copy text"
            value={copy.text}></textarea>
          <div className="terminal-paste-actions">
            <button type="button" className="terminal-key" onClick={() => copy.onDone()}>
              Done
            </button>
          </div>
        </div>
      ) : null}
      {paste.open ? (
        <div className="terminal-paste-fallback">
          <p className="terminal-paste-notice" role="status">
            {paste.notice}
          </p>
          <textarea
            className="terminal-paste-input"
            aria-label="Paste text"
            value={paste.text}
            onChange={(event) => paste.onTextChange(event.target.value)}></textarea>
          <div className="terminal-paste-actions">
            <button type="button" className="terminal-key" onClick={() => paste.onSend()}>
              Send
            </button>
            <button type="button" className="terminal-key" onClick={() => paste.onCancel()}>
              Cancel
            </button>
          </div>
        </div>
      ) : null}
    </>
  );
}
