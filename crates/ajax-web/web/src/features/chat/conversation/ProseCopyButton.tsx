import { useEffect, useRef, useState } from "react";
import { copyProseSource } from "./copyProse";

const COPIED_MS = 1600;

export default function ProseCopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const resetTimer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (resetTimer.current !== null) window.clearTimeout(resetTimer.current);
    },
    [],
  );

  async function onCopy() {
    const ok = await copyProseSource(text);
    if (!ok) {
      setCopied(false);
      return;
    }
    setCopied(true);
    if (resetTimer.current !== null) window.clearTimeout(resetTimer.current);
    resetTimer.current = window.setTimeout(() => {
      setCopied(false);
      resetTimer.current = null;
    }, COPIED_MS);
  }

  const label = copied ? "Copied" : "Copy answer";

  return (
    <button
      type="button"
      className="pill session-reply-copy"
      aria-label={label}
      data-testid="session-reply-copy"
      data-copied={copied ? "true" : "false"}
      onClick={() => void onCopy()}
    >
      {copied ? "Copied" : "Copy"}
    </button>
  );
}
