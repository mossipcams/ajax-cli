import { useCallback, useEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import type { Terminal } from "@xterm/xterm";
import type { TerminalConnection } from "@/shared/lib/terminalConnection";
import {
  createSpeechInputModel,
  isStandaloneStartOver,
  speechReducer,
  type SpeechInputModel,
} from "@/shared/lib/speechState";
import {
  clearSpeechInserts,
  undoPayload,
  type SpeechInsert,
} from "@/shared/lib/speechInsertLedger";
import {
  createBrowserSpeechPlatform,
  createSpeechTransport,
  isMicrophonePermissionDenied,
  newSessionId,
  type SpeechTransport,
} from "@/shared/lib/speechTransport";

export type TaskTerminalSpeechDeps = {
  handle: string;
  termRef: RefObject<Terminal | undefined>;
  connectionRef: RefObject<TerminalConnection | undefined>;
  pasteThroughTerm: (text: string, ownedFocus?: boolean) => boolean;
};

export function useTaskTerminalSpeech(deps: TaskTerminalSpeechDeps): {
  speechModel: SpeechInputModel;
  pauseCountdownSeconds: number | undefined;
  micAriaLabel: string;
  micArmed: boolean;
  toggleMic: () => void;
  cancelSpeechInput: () => void;
  cancelSpeechTransport: () => void;
} {
  const { handle, termRef, connectionRef, pasteThroughTerm } = deps;

  const [speechModel, setSpeechModel] = useState<SpeechInputModel>(() =>
    createSpeechInputModel(),
  );
  const [pauseCountdownSeconds, setPauseCountdownSeconds] = useState<number | undefined>();
  const speechTransportRef = useRef<SpeechTransport | undefined>(undefined);
  const insertedSpeechRef = useRef<SpeechInsert[]>([]);
  const speechModelRef = useRef(speechModel);
  speechModelRef.current = speechModel;

  const undoInsertedSpeech = () => {
    const records = insertedSpeechRef.current;
    if (records.length === 0) return;
    const payload = undoPayload(records);
    clearSpeechInserts(records);
    // ponytail: assumes speech only appends to the current line; en-US UTF-16 .length DEL undo.
    if (payload && connectionRef.current?.isOpen()) {
      connectionRef.current.sendInput(payload);
    }
  };

  const dispatchSpeech = (action: Parameters<typeof speechReducer>[1]) => {
    setSpeechModel((previous) => speechReducer(previous, action));
  };

  const cancelSpeechInput = () => {
    const sessionId = speechModelRef.current.sessionId;
    speechTransportRef.current?.cancel();
    speechTransportRef.current = undefined;
    clearSpeechInserts(insertedSpeechRef.current);
    if (sessionId) {
      dispatchSpeech({ type: "cancel", sessionId });
    } else {
      setSpeechModel(createSpeechInputModel());
    }
    setPauseCountdownSeconds(undefined);
  };

  const finalizeMic = () => {
    const current = speechModelRef.current;
    if (
      current.state !== "listening" &&
      current.state !== "pause_pending"
    ) {
      return;
    }
    const sessionId = current.sessionId;
    if (!sessionId) {
      return;
    }

    setPauseCountdownSeconds(undefined);
    setSpeechModel((previous) => {
      const next = speechReducer(previous, {
        type: "request_stop",
        sessionId,
      });
      if (next.state === "finalizing" && previous.state !== "finalizing") {
        speechTransportRef.current?.stop();
      }
      return next;
    });
  };

  const activateMic = () => {
    if (
      !(
        speechModelRef.current.state === "idle" ||
        speechModelRef.current.state === "error"
      )
    ) {
      return;
    }

    if (speechTransportRef.current) {
      speechTransportRef.current.cancel();
      speechTransportRef.current = undefined;
    }

    const sessionId = newSessionId();
    clearSpeechInserts(insertedSpeechRef.current);
    dispatchSpeech({ type: "start", sessionId });

    const transport = createSpeechTransport(
      handle,
      {
        onReady: ({ pauseGracePeriodMs }) =>
          dispatchSpeech({ type: "provider_ready", sessionId, pauseGracePeriodMs }),
        onPartial: (sequence, text) =>
          dispatchSpeech({ type: "partial", sessionId, sequence, text }),
        onFinal: (sequence, text) => {
          // Paste contiguous transcript deltas / undo here, never inside a setState
          // updater: StrictMode may invoke an updater twice and double-write the PTY.
          const previous = speechModelRef.current;
          const action = {
            type: "final" as const,
            sessionId,
            sequence,
            text,
            nowMs: performance.now(),
          };
          const next = speechReducer(previous, action);
          if (next === previous) return;
          speechModelRef.current = next;
          setSpeechModel(next);

          if (isStandaloneStartOver(text)) {
            const canHandleControl =
              previous.state === "listening" || previous.state === "pause_pending";
            if (canHandleControl) {
              undoInsertedSpeech();
            }
            return;
          }

          if (
            next.finalTranscript !== previous.finalTranscript &&
            next.finalTranscript.startsWith(previous.finalTranscript)
          ) {
            const delta = next.finalTranscript.slice(previous.finalTranscript.length);
            if (delta) {
              const bracketed = termRef.current?.modes.bracketedPasteMode ?? false;
              if (pasteThroughTerm(delta, false)) {
                insertedSpeechRef.current.push({ text: delta, bracketed });
              }
            }
          }
        },
        onSpeechStarted: () => dispatchSpeech({ type: "speech_started", sessionId }),
        onSpeechEnded: () => {},
        onError: (message) => dispatchSpeech({ type: "error", sessionId, message }),
        onClosed: () => {
          const current = speechModelRef.current;
          if (current.state === "finalizing" && current.sessionId === sessionId) {
            dispatchSpeech({ type: "finalization_complete", sessionId });
          } else if (
            current.sessionId === sessionId &&
            current.state !== "finalizing" &&
            current.state !== "idle" &&
            current.state !== "error"
          ) {
            dispatchSpeech({
              type: "error",
              sessionId,
              message: "Speech connection closed",
            });
          }
          if (speechTransportRef.current) {
            speechTransportRef.current = undefined;
          }
          setPauseCountdownSeconds(undefined);
        },
        onBackpressureWarning: (message) => {
          // Surface via the existing status region without tearing down capture yet.
          setSpeechModel((previous) =>
            previous.sessionId === sessionId
              ? { ...previous, errorMessage: message }
              : previous,
          );
        },
      },
      createBrowserSpeechPlatform(),
      { sessionId },
    );
    speechTransportRef.current = transport;
    void transport.start().catch((error) => {
      if (isMicrophonePermissionDenied(error)) {
        cancelSpeechInput();
        return;
      }
      // Other errors surface through onError / reducer.
    });
  };

  const toggleMic = () => {
    const state = speechModelRef.current.state;
    if (state === "listening" || state === "pause_pending") {
      finalizeMic();
    } else {
      activateMic();
    }
  };

  useEffect(() => {
    if (speechModel.state !== "pause_pending" || speechModel.pauseDeadlineMs === undefined) {
      setPauseCountdownSeconds(undefined);
      return;
    }
    const sessionId = speechModel.sessionId;
    const timerToken = speechModel.pauseTimerToken;
    const deadlineMs = speechModel.pauseDeadlineMs;
    if (!sessionId || timerToken === undefined) return;

    const tick = () => {
      const remainingMs = deadlineMs - performance.now();
      if (remainingMs <= 0) {
        setPauseCountdownSeconds(0);
        setSpeechModel((previous) => {
          const next = speechReducer(previous, {
            type: "pause_elapsed",
            sessionId,
            timerToken,
          });
          if (next.state === "finalizing" && previous.state === "pause_pending") {
            speechTransportRef.current?.stop();
          }
          return next;
        });
        return;
      }
      setPauseCountdownSeconds(Math.max(1, Math.ceil(remainingMs / 1000)));
    };
    tick();
    const intervalId = window.setInterval(tick, 200);
    return () => window.clearInterval(intervalId);
  }, [
    speechModel.state,
    speechModel.pauseDeadlineMs,
    speechModel.pauseTimerToken,
    speechModel.sessionId,
  ]);

  const micAriaLabel = (() => {
    switch (speechModel.state) {
      case "connecting":
        return "Connecting voice input";
      case "listening":
        return "Voice input listening";
      case "pause_pending":
        return pauseCountdownSeconds !== undefined
          ? `Voice input pausing in ${pauseCountdownSeconds} seconds`
          : "Voice input pause pending";
      case "finalizing":
        return "Finalizing voice input";
      case "error":
        return speechModel.errorMessage
          ? `Voice input failed: ${speechModel.errorMessage}`
          : "Voice input failed";
      default:
        return "Start voice input";
    }
  })();

  const micArmed =
    speechModel.state === "listening" || speechModel.state === "pause_pending";

  useEffect(() => {
    return () => {
      speechTransportRef.current?.cancel();
      speechTransportRef.current = undefined;
    };
  }, []);

  const cancelSpeechTransport = useCallback(() => {
    speechTransportRef.current?.cancel();
    speechTransportRef.current = undefined;
  }, []);

  return {
    speechModel,
    pauseCountdownSeconds,
    micAriaLabel,
    micArmed,
    toggleMic,
    cancelSpeechInput,
    cancelSpeechTransport,
  };
}
