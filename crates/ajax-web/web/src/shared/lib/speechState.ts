// Pure speech-input state for continuous STT. No timers, browser APIs, or task truth.

export const DEFAULT_SPEECH_CONFIG = {
  pauseGracePeriodMs: 9000,
} as const;

export type SpeechInputState =
  | "idle"
  | "connecting"
  | "listening"
  | "pause_pending"
  | "finalizing"
  | "error";

/** @deprecated Use `SpeechInputState`. */
export type SpeechState = SpeechInputState;

export interface SpeechInputModel {
  state: SpeechInputState;
  sessionId?: string;
  pauseGracePeriodMs: number;
  finalTranscript: string;
  partialTranscript: string;
  pauseDeadlineMs?: number;
  pauseTimerToken?: number;
  errorMessage?: string;
  finalSegments: Record<number, string>;
  nextPauseTimerToken: number;
}

export type SpeechAction =
  | { type: "start"; sessionId: string }
  | { type: "provider_ready"; sessionId: string; pauseGracePeriodMs: number }
  | { type: "partial"; sessionId: string; sequence: number; text: string }
  | {
      type: "final";
      sessionId: string;
      sequence: number;
      text: string;
      nowMs: number;
    }
  | { type: "speech_started"; sessionId: string }
  | { type: "pause_elapsed"; sessionId: string; timerToken: number }
  | { type: "finalization_complete"; sessionId: string }
  | { type: "cancel"; sessionId: string }
  | { type: "error"; sessionId: string; message: string };

export function createSpeechInputModel(): SpeechInputModel {
  return {
    state: "idle",
    pauseGracePeriodMs: DEFAULT_SPEECH_CONFIG.pauseGracePeriodMs,
    finalTranscript: "",
    partialTranscript: "",
    finalSegments: {},
    nextPauseTimerToken: 1,
  };
}

export function isStandalonePause(text: string): boolean {
  const normalized = text.trim().toLowerCase().replace(/[\s.!?]+$/, "");
  return normalized === "pause";
}

export function isStandaloneStartOver(text: string): boolean {
  const normalized = text.trim().toLowerCase().replace(/[\s.!?]+$/, "");
  return normalized === "start over";
}

function buildFinalTranscript(finalSegments: Record<number, string>): string {
  const sequences = Object.keys(finalSegments)
    .map(Number)
    .sort((left, right) => left - right);
  return sequences.map((sequence) => finalSegments[sequence]).join(" ").trim();
}

function allowsNewStart(model: SpeechInputModel): boolean {
  return model.state === "idle" || model.state === "error";
}

function isActiveSession(model: SpeechInputModel, sessionId: string): boolean {
  return model.sessionId === sessionId;
}

export function speechReducer(
  model: SpeechInputModel,
  action: SpeechAction,
): SpeechInputModel {
  switch (action.type) {
    case "start":
      if (!allowsNewStart(model)) {
        return model;
      }
      return {
        ...model,
        state: "connecting",
        sessionId: action.sessionId,
        finalTranscript: "",
        partialTranscript: "",
        finalSegments: {},
        pauseDeadlineMs: undefined,
        pauseTimerToken: undefined,
        errorMessage: undefined,
      };

    case "provider_ready":
      if (model.state !== "connecting" || !isActiveSession(model, action.sessionId)) {
        return model;
      }
      return {
        ...model,
        state: "listening",
        pauseGracePeriodMs: action.pauseGracePeriodMs,
      };

    case "partial":
      if (model.state !== "listening" || !isActiveSession(model, action.sessionId)) {
        return model;
      }
      return { ...model, partialTranscript: action.text };

    case "final": {
      if (!isActiveSession(model, action.sessionId)) {
        return model;
      }

      const canAcceptControl =
        model.state === "listening" || model.state === "pause_pending";
      const canAcceptOrdinary = model.state === "listening";

      if (isStandalonePause(action.text)) {
        if (!canAcceptControl) {
          return model;
        }
        const timerToken = model.nextPauseTimerToken;
        return {
          ...model,
          state: "pause_pending",
          partialTranscript: "",
          pauseDeadlineMs: action.nowMs + model.pauseGracePeriodMs,
          pauseTimerToken: timerToken,
          nextPauseTimerToken: timerToken + 1,
        };
      }
      if (isStandaloneStartOver(action.text)) {
        if (!canAcceptControl) {
          return model;
        }
        return {
          ...model,
          finalSegments: {},
          finalTranscript: "",
          partialTranscript: "",
        };
      }
      if (!canAcceptOrdinary) {
        return model;
      }
      if (model.finalSegments[action.sequence] !== undefined) {
        return model;
      }
      const finalSegments = {
        ...model.finalSegments,
        [action.sequence]: action.text,
      };
      return {
        ...model,
        finalSegments,
        finalTranscript: buildFinalTranscript(finalSegments),
        partialTranscript: "",
      };
    }

    case "speech_started":
      if (model.state !== "pause_pending" || !isActiveSession(model, action.sessionId)) {
        return model;
      }
      return {
        ...model,
        state: "listening",
        pauseDeadlineMs: undefined,
      };

    case "pause_elapsed":
      if (
        model.state !== "pause_pending" ||
        !isActiveSession(model, action.sessionId) ||
        model.pauseTimerToken !== action.timerToken
      ) {
        return model;
      }
      return {
        ...model,
        state: "finalizing",
        pauseDeadlineMs: undefined,
      };

    case "finalization_complete":
      if (model.state !== "finalizing" || !isActiveSession(model, action.sessionId)) {
        return model;
      }
      return {
        ...model,
        state: "idle",
        sessionId: undefined,
        partialTranscript: "",
        pauseDeadlineMs: undefined,
        pauseTimerToken: undefined,
        errorMessage: undefined,
      };

    case "cancel":
      if (!isActiveSession(model, action.sessionId)) {
        return model;
      }
      return {
        ...model,
        state: "idle",
        sessionId: undefined,
        partialTranscript: "",
        pauseDeadlineMs: undefined,
        pauseTimerToken: undefined,
        errorMessage: undefined,
      };

    case "error":
      if (!isActiveSession(model, action.sessionId)) {
        return model;
      }
      return {
        ...model,
        state: "error",
        partialTranscript: "",
        pauseDeadlineMs: undefined,
        pauseTimerToken: undefined,
        errorMessage: action.message,
      };

    default:
      return model;
  }
}
