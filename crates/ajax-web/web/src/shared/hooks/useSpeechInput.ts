import { useCallback, useEffect, useRef, useState } from "react";
import {
  createSpeechInputModel,
  isStandaloneStartOver,
  speechReducer,
  type SpeechInputModel,
} from "@/shared/lib/speechState";
import {
  clearSpeechInserts,
  type SpeechInsert,
} from "@/shared/lib/speechInsertLedger";
import {
  createBrowserSpeechPlatform,
  createSpeechTransport,
  isMicrophonePermissionDenied,
  newSessionId,
  type SpeechTransport,
} from "@/shared/lib/speechTransport";

export type SpeechInputAdapter = {
  insertDelta: (delta: string) => { ok: boolean; record?: SpeechInsert };
  undoInserts: (records: readonly SpeechInsert[]) => void;
};

export function useSpeechInput(
  handle: string,
  adapter: SpeechInputAdapter,
): {
  speechModel: SpeechInputModel;
  pauseCountdownSeconds: number | undefined;
  micAriaLabel: string;
  micArmed: boolean;
  toggleMic: () => void;
  cancelSpeechInput: () => void;
  cancelSpeechTransport: () => void;
} {
  const adapterRef = useRef(adapter);
  adapterRef.current = adapter;

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
    adapterRef.current.undoInserts(records);
    clearSpeechInserts(records);
  };

  const dispatchSpeech = (action: Parameters<typeof speechReducer>[1]) => {
    const previous = speechModelRef.current;
    const next = speechReducer(previous, action);
    if (next === previous) return;
    speechModelRef.current = next;
    setSpeechModel(next);
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
              const result = adapterRef.current.insertDelta(delta);
              if (result.ok && result.record) {
                insertedSpeechRef.current.push(result.record);
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
