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
  /** Contiguous finalized text applied in sequence order (auto-insert destination). */
  finalTranscript: string;
  partialTranscript: string;
  pauseDeadlineMs?: number;
  pauseTimerToken?: number;
  errorMessage?: string;
  /** All received finals keyed by sequence (may include buffered future seqs). */
  finalSegments: Record<number, string>;
  /** Next sequence number that may be appended to finalTranscript. */
  nextExpectedSequence: number;
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
  | { type: "request_stop"; sessionId: string }
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
    nextExpectedSequence: 0,
    nextPauseTimerToken: 1,
  };
}

/** Strip trailing whitespace and terminal punctuation (ASCII + common Unicode). */
function normalizeStandaloneCommand(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/^[\s\u00a0]+|[\s\u00a0]+$/g, "")
    .replace(/[\s.!,?…。，、；：！？｡､]+$/u, "");
}

export function isStandalonePause(text: string): boolean {
  return normalizeStandaloneCommand(text) === "pause";
}

function buildFinalTranscript(
  finalSegments: Record<number, string>,
  throughExclusive: number,
): string {
  const parts: string[] = [];
  for (let sequence = 0; sequence < throughExclusive; sequence += 1) {
    const part = finalSegments[sequence];
    if (part !== undefined && part.length > 0) {
      parts.push(part);
    }
  }
  return parts.join(" ").trim();
}

/** Apply newly contiguous segments; returns next expected sequence. */
export function advanceContiguousFinals(
  finalSegments: Record<number, string>,
  nextExpectedSequence: number,
): { nextExpectedSequence: number; finalTranscript: string } {
  let next = nextExpectedSequence;
  while (finalSegments[next] !== undefined) {
    next += 1;
  }
  return {
    nextExpectedSequence: next,
    finalTranscript: buildFinalTranscript(finalSegments, next),
  };
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
        nextExpectedSequence: 0,
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
      // Ordinary dictation — including the words "start over" — is never a control.
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
      const advanced = advanceContiguousFinals(finalSegments, model.nextExpectedSequence);
      return {
        ...model,
        finalSegments,
        nextExpectedSequence: advanced.nextExpectedSequence,
        finalTranscript: advanced.finalTranscript,
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

    case "request_stop":
      if (
        !isActiveSession(model, action.sessionId) ||
        (model.state !== "listening" && model.state !== "pause_pending")
      ) {
        return model;
      }
      return {
        ...model,
        state: "finalizing",
        pauseDeadlineMs: undefined,
        pauseTimerToken: undefined,
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
