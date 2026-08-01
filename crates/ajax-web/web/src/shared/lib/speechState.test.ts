import { describe, expect, it } from "vitest";
import {
  DEFAULT_SPEECH_CONFIG,
  createSpeechInputModel,
  isStandalonePause,
  isStandaloneStartOver,
  speechReducer,
} from "./speechState";

const startListening = (sessionId = "session-1") => {
  const connecting = speechReducer(createSpeechInputModel(), {
    type: "start",
    sessionId,
  });
  return speechReducer(connecting, {
    type: "provider_ready",
    sessionId,
    pauseGracePeriodMs: DEFAULT_SPEECH_CONFIG.pauseGracePeriodMs,
  });
};

describe("speech state", () => {
  it("allows one active session and rejects duplicate starts", () => {
    const connecting = speechReducer(createSpeechInputModel(), {
      type: "start",
      sessionId: "session-1",
    });

    expect(connecting.state).toBe("connecting");
    expect(
      speechReducer(connecting, { type: "start", sessionId: "session-2" }),
    ).toBe(connecting);
  });

  it("recognizes only a normalized standalone pause utterance", () => {
    expect(isStandalonePause("pause")).toBe(true);
    expect(isStandalonePause("Pause.")).toBe(true);
    expect(isStandalonePause("Pause!")).toBe(true);
    expect(isStandalonePause("PAUSE")).toBe(true);
    expect(isStandalonePause("Add a pause between retries.")).toBe(false);

    const listening = startListening();
    const paused = speechReducer(listening, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "Pause.",
      nowMs: 1000,
    });

    expect(paused.state).toBe("pause_pending");
    expect(paused.finalTranscript).toBe("");
    expect(paused.partialTranscript).toBe("");
    expect(paused.pauseDeadlineMs).toBe(1000 + DEFAULT_SPEECH_CONFIG.pauseGracePeriodMs);
    expect(paused.pauseTimerToken).toBe(1);
  });

  it("uses server-provided pause grace period for pause deadline", () => {
    const connecting = speechReducer(createSpeechInputModel(), {
      type: "start",
      sessionId: "session-1",
    });
    const listening = speechReducer(connecting, {
      type: "provider_ready",
      sessionId: "session-1",
      pauseGracePeriodMs: 4000,
    });
    const paused = speechReducer(listening, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "pause",
      nowMs: 1000,
    });

    expect(paused.pauseDeadlineMs).toBe(5000);
    expect(paused.pauseDeadlineMs).not.toBe(1000 + DEFAULT_SPEECH_CONFIG.pauseGracePeriodMs);
  });

  it("recognizes only a normalized standalone start over utterance", () => {
    expect(isStandaloneStartOver("start over")).toBe(true);
    expect(isStandaloneStartOver("Start over.")).toBe(true);
    expect(isStandaloneStartOver("START OVER!")).toBe(true);
    expect(isStandaloneStartOver("We should start over tomorrow.")).toBe(false);

    let model = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "Hello world.",
      nowMs: 1000,
    });
    expect(model.finalTranscript).toBe("Hello world.");

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 2,
      text: "Start over.",
      nowMs: 1100,
    });

    expect(model.state).toBe("listening");
    expect(model.finalTranscript).toBe("");
    expect(model.partialTranscript).toBe("");
    expect(model.finalSegments).toEqual({});
  });

  it("clears segments on start over during pause_pending without changing state", () => {
    let model = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "Hello world.",
      nowMs: 1000,
    });
    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 2,
      text: "pause",
      nowMs: 1100,
    });
    expect(model.state).toBe("pause_pending");

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 3,
      text: "Start over.",
      nowMs: 1200,
    });

    expect(model.state).toBe("pause_pending");
    expect(model.finalTranscript).toBe("");
    expect(model.partialTranscript).toBe("");
    expect(model.finalSegments).toEqual({});
    expect(model.pauseDeadlineMs).toBe(1100 + DEFAULT_SPEECH_CONFIG.pauseGracePeriodMs);
    expect(model.pauseTimerToken).toBe(1);
  });

  it("accepts a repeated sequence after start over clears segments", () => {
    let model = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "Hello.",
      nowMs: 1000,
    });
    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "start over",
      nowMs: 1100,
    });
    expect(model.finalSegments).toEqual({});

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "Hello again.",
      nowMs: 1200,
    });

    expect(model.finalTranscript).toBe("Hello again.");
    expect(model.finalSegments).toEqual({ 1: "Hello again." });
  });

  it("ignores ordinary finals while pause_pending", () => {
    const paused = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "pause",
      nowMs: 1000,
    });
    const withFinal = speechReducer(paused, {
      type: "final",
      sessionId: "session-1",
      sequence: 2,
      text: "Late phrase.",
      nowMs: 1100,
    });

    expect(withFinal).toBe(paused);
    expect(withFinal.finalTranscript).toBe("");
    expect(withFinal.finalSegments).toEqual({});
  });

  it("keeps sentence uses of pause as transcript content", () => {
    const result = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "Add a pause between retries.",
      nowMs: 1000,
    });

    expect(result.state).toBe("listening");
    expect(result.finalTranscript).toBe("Add a pause between retries.");
  });

  it("cancels a pause timer immediately on speech activity", () => {
    const paused = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "pause",
      nowMs: 1000,
    });
    const resumed = speechReducer(paused, {
      type: "speech_started",
      sessionId: "session-1",
    });

    expect(resumed.state).toBe("listening");
    expect(resumed.pauseDeadlineMs).toBeUndefined();
    expect(
      speechReducer(resumed, {
        type: "pause_elapsed",
        sessionId: "session-1",
        timerToken: paused.pauseTimerToken,
      }),
    ).toBe(resumed);
  });

  it("finalizes on request_stop from listening or pause_pending", () => {
    const listening = startListening();
    const fromListening = speechReducer(listening, {
      type: "request_stop",
      sessionId: "session-1",
    });
    expect(fromListening.state).toBe("finalizing");
    expect(fromListening.pauseDeadlineMs).toBeUndefined();
    expect(fromListening.pauseTimerToken).toBeUndefined();

    const paused = speechReducer(listening, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "pause",
      nowMs: 1000,
    });
    const fromPaused = speechReducer(paused, {
      type: "request_stop",
      sessionId: "session-1",
    });
    expect(fromPaused.state).toBe("finalizing");
    expect(fromPaused.pauseDeadlineMs).toBeUndefined();
    expect(fromPaused.pauseTimerToken).toBeUndefined();
  });

  it("ignores request_stop from other states or stale sessions", () => {
    const listening = startListening("session-2");
    expect(
      speechReducer(listening, { type: "request_stop", sessionId: "session-1" }),
    ).toBe(listening);
    expect(
      speechReducer(createSpeechInputModel(), {
        type: "request_stop",
        sessionId: "session-1",
      }),
    ).toEqual(createSpeechInputModel());
    const connecting = speechReducer(createSpeechInputModel(), {
      type: "start",
      sessionId: "session-1",
    });
    expect(
      speechReducer(connecting, { type: "request_stop", sessionId: "session-1" }),
    ).toBe(connecting);
  });

  it("finalizes only after the active pause timer and preserves finals on cancel", () => {
    const paused = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "pause",
      nowMs: 1000,
    });
    const finalizing = speechReducer(paused, {
      type: "pause_elapsed",
      sessionId: "session-1",
      timerToken: paused.pauseTimerToken,
    });

    expect(finalizing.state).toBe("finalizing");
    expect(
      speechReducer(finalizing, {
        type: "finalization_complete",
        sessionId: "session-1",
      }).state,
    ).toBe("idle");

    const withFinal = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "Keep this.",
      nowMs: 1000,
    });
    const cancelled = speechReducer(withFinal, {
      type: "cancel",
      sessionId: "session-1",
    });
    expect(cancelled.state).toBe("idle");
    expect(cancelled.finalTranscript).toBe("Keep this.");
  });

  it("replaces partials and orders duplicate final segments by sequence", () => {
    let model = startListening();
    model = speechReducer(model, {
      type: "partial",
      sessionId: "session-1",
      sequence: 2,
      text: "Hello wor",
    });
    model = speechReducer(model, {
      type: "partial",
      sessionId: "session-1",
      sequence: 2,
      text: "Hello world",
    });
    expect(model.partialTranscript).toBe("Hello world");

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 2,
      text: "world.",
      nowMs: 1000,
    });
    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "Hello",
      nowMs: 1100,
    });
    const duplicate = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "Hello",
      nowMs: 1200,
    });

    expect(model.finalTranscript).toBe("Hello world.");
    expect(model.partialTranscript).toBe("");
    expect(duplicate).toBe(model);
  });

  it("ignores stale session and timer events", () => {
    const model = startListening("session-2");

    expect(
      speechReducer(model, {
        type: "final",
        sessionId: "session-1",
        sequence: 1,
        text: "stale",
        nowMs: 1000,
      }),
    ).toBe(model);
    expect(
      speechReducer(model, {
        type: "pause_elapsed",
        sessionId: "session-2",
        timerToken: 99,
      }),
    ).toBe(model);
  });

  it("enters an explicit recoverable error state for the active session", () => {
    const errored = speechReducer(startListening(), {
      type: "error",
      sessionId: "session-1",
      message: "Microphone permission denied.",
    });

    expect(errored.state).toBe("error");
    expect(errored.errorMessage).toBe("Microphone permission denied.");
    expect(errored.finalTranscript).toBe("");
  });

  it("allows a recoverable error to start a fresh session", () => {
    const errored = speechReducer(startListening(), {
      type: "error",
      sessionId: "session-1",
      message: "Provider unavailable.",
    });
    const retry = speechReducer(errored, { type: "start", sessionId: "session-2" });

    expect(retry.state).toBe("connecting");
    expect(retry.sessionId).toBe("session-2");
    expect(retry.errorMessage).toBeUndefined();
  });
});
