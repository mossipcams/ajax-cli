import { useMemo, type RefObject } from "react";
import type { Terminal } from "@xterm/xterm";
import type { TerminalConnection } from "@/shared/lib/terminalConnection";
import { undoPayload, type SpeechInsert } from "@/shared/lib/speechInsertLedger";
import { useSpeechInput } from "@/shared/hooks/useSpeechInput";

export type TaskTerminalSpeechDeps = {
  handle: string;
  termRef: RefObject<Terminal | undefined>;
  connectionRef: RefObject<TerminalConnection | undefined>;
  pasteThroughTerm: (text: string, ownedFocus?: boolean) => boolean;
};

export function useTaskTerminalSpeech(deps: TaskTerminalSpeechDeps): {
  speechModel: ReturnType<typeof useSpeechInput>["speechModel"];
  pauseCountdownSeconds: number | undefined;
  micAriaLabel: string;
  micArmed: boolean;
  toggleMic: () => void;
  cancelSpeechInput: () => void;
  cancelSpeechTransport: () => void;
} {
  const { handle, termRef, connectionRef, pasteThroughTerm } = deps;

  const adapter = useMemo(
    () => ({
      insertDelta: (delta: string) => {
        const bracketed = termRef.current?.modes.bracketedPasteMode ?? false;
        if (pasteThroughTerm(delta, false)) {
          return { ok: true, record: { text: delta, bracketed } satisfies SpeechInsert };
        }
        return { ok: false };
      },
      undoInserts: (records: readonly SpeechInsert[]) => {
        const payload = undoPayload(records);
        if (payload && connectionRef.current?.isOpen()) {
          connectionRef.current.sendInput(payload);
        }
      },
    }),
    [connectionRef, pasteThroughTerm, termRef],
  );

  return useSpeechInput(handle, adapter);
}
