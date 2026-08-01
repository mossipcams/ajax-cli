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

  it("recognizes punctuated standalone pause and rejects sentence uses", () => {
    expect(isStandalonePause("pause")).toBe(true);
    expect(isStandalonePause("Pause.")).toBe(true);
    expect(isStandalonePause("Pause,")).toBe(true);
    expect(isStandalonePause("Pause!")).toBe(true);
    expect(isStandalonePause("Pause?")).toBe(true);
    expect(isStandalonePause("PAUSE")).toBe(true);
    expect(isStandalonePause("Add a pause between retries.")).toBe(false);
    expect(isStandalonePause("Pause the build after the tests.")).toBe(false);
    expect(isStandalonePause("The pause command is broken.")).toBe(false);
    expect(isStandalonePause("Create a pause command.")).toBe(false);

    const listening = startListening();
    const paused = speechReducer(listening, {
      type: "final",
      sessionId: "session-1",
      sequence: 0,
      text: "Pause,",
      nowMs: 1000,
    });

    expect(paused.state).toBe("pause_pending");
    expect(paused.finalTranscript).toBe("");
    expect(paused.partialTranscript).toBe("");
    expect(paused.pauseDeadlineMs).toBe(1000 + DEFAULT_SPEECH_CONFIG.pauseGracePeriodMs);
  });

  it("clears finals on standalone start over / start fresh and continues sequences", () => {
    expect(isStandaloneStartOver("start over")).toBe(true);
    expect(isStandaloneStartOver("Start over.")).toBe(true);
    expect(isStandaloneStartOver("start fresh")).toBe(true);
    expect(isStandaloneStartOver("Start fresh!")).toBe(true);
    expect(isStandaloneStartOver("please start over now")).toBe(false);
    expect(isStandalonePause("start over")).toBe(false);

    let model = startListening();
    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 0,
      text: "Hello",
      nowMs: 1000,
    });
    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "world",
      nowMs: 1100,
    });
    expect(model.finalTranscript).toBe("Hello world");

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 2,
      text: "Start over.",
      nowMs: 1200,
    });
    expect(model.state).toBe("listening");
    expect(model.finalTranscript).toBe("");
    expect(model.finalSegments).toEqual({});
    expect(model.nextExpectedSequence).toBe(3);

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 3,
      text: "Again",
      nowMs: 1300,
    });
    expect(model.finalTranscript).toBe("Again");
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
      sequence: 0,
      text: "pause",
      nowMs: 1000,
    });

    expect(paused.pauseDeadlineMs).toBe(5000);
  });

  it("ignores ordinary finals while pause_pending", () => {
    const paused = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 0,
      text: "pause",
      nowMs: 1000,
    });
    const withFinal = speechReducer(paused, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
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
      sequence: 0,
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
      sequence: 0,
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
        timerToken: paused.pauseTimerToken!,
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

    const paused = speechReducer(listening, {
      type: "final",
      sessionId: "session-1",
      sequence: 0,
      text: "pause",
      nowMs: 1000,
    });
    const fromPaused = speechReducer(paused, {
      type: "request_stop",
      sessionId: "session-1",
    });
    expect(fromPaused.state).toBe("finalizing");
  });

  it("ignores request_stop from other states or stale sessions", () => {
    const listening = startListening("session-2");
    expect(
      speechReducer(listening, { type: "request_stop", sessionId: "session-1" }),
    ).toBe(listening);
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
      sequence: 0,
      text: "pause",
      nowMs: 1000,
    });
    const finalizing = speechReducer(paused, {
      type: "pause_elapsed",
      sessionId: "session-1",
      timerToken: paused.pauseTimerToken!,
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
      sequence: 0,
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

  it("buffers out-of-order finals and applies them only in sequence order", () => {
    let model = startListening();
    model = speechReducer(model, {
      type: "partial",
      sessionId: "session-1",
      sequence: 0,
      text: "Hello wor",
    });
    model = speechReducer(model, {
      type: "partial",
      sessionId: "session-1",
      sequence: 0,
      text: "Hello world",
    });
    expect(model.partialTranscript).toBe("Hello world");

    // Future segment arrives first — must not appear in transcript yet.
    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "world.",
      nowMs: 1000,
    });
    expect(model.finalTranscript).toBe("");
    expect(model.nextExpectedSequence).toBe(0);

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 0,
      text: "Hello",
      nowMs: 1100,
    });
    expect(model.finalTranscript).toBe("Hello world.");
    expect(model.nextExpectedSequence).toBe(2);
    expect(model.partialTranscript).toBe("");

    const duplicate = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 0,
      text: "Hello",
      nowMs: 1200,
    });
    expect(duplicate).toBe(model);
  });

  it("ignores delayed missing-gap future segments until the gap fills", () => {
    let model = speechReducer(startListening(), {
      type: "final",
      sessionId: "session-1",
      sequence: 2,
      text: "three",
      nowMs: 1000,
    });
    expect(model.finalTranscript).toBe("");
    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 0,
      text: "one",
      nowMs: 1100,
    });
    expect(model.finalTranscript).toBe("one");
    expect(model.nextExpectedSequence).toBe(1);
    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "two",
      nowMs: 1200,
    });
    expect(model.finalTranscript).toBe("one two three");
    expect(model.nextExpectedSequence).toBe(3);
  });

  it("ignores stale session and timer events", () => {
    const model = startListening("session-2");

    expect(
      speechReducer(model, {
        type: "final",
        sessionId: "session-1",
        sequence: 0,
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

  it("walks the full dictation lifecycle into idle without submitting", () => {
    let model = speechReducer(createSpeechInputModel(), {
      type: "start",
      sessionId: "session-1",
    });
    expect(model.state).toBe("connecting");

    // Readiness gates recognition: partials before ready are ignored.
    model = speechReducer(model, {
      type: "partial",
      sessionId: "session-1",
      sequence: 0,
      text: "too early",
    });
    expect(model.partialTranscript).toBe("");

    model = speechReducer(model, {
      type: "provider_ready",
      sessionId: "session-1",
      pauseGracePeriodMs: 9_000,
    });
    expect(model.state).toBe("listening");

    model = speechReducer(model, {
      type: "partial",
      sessionId: "session-1",
      sequence: 0,
      text: "Hello wor",
    });
    expect(model.partialTranscript).toBe("Hello wor");
    expect(model.finalTranscript).toBe("");

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 0,
      text: "Hello world",
      nowMs: 1_000,
    });
    expect(model.finalTranscript).toBe("Hello world");
    expect(model.partialTranscript).toBe("");

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 1,
      text: "and more",
      nowMs: 1_100,
    });
    expect(model.finalTranscript).toBe("Hello world and more");
    expect(model.state).toBe("listening");

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 2,
      text: "Pause.",
      nowMs: 1_200,
    });
    expect(model.state).toBe("pause_pending");
    expect(model.finalTranscript).toBe("Hello world and more");
    expect(model.pauseDeadlineMs).toBe(1_200 + 9_000);

    // Speak-to-continue cancels the countdown.
    model = speechReducer(model, {
      type: "speech_started",
      sessionId: "session-1",
    });
    expect(model.state).toBe("listening");
    expect(model.pauseDeadlineMs).toBeUndefined();

    model = speechReducer(model, {
      type: "final",
      sessionId: "session-1",
      sequence: 3,
      text: "pause",
      nowMs: 2_000,
    });
    const pauseToken = model.pauseTimerToken;
    expect(model.state).toBe("pause_pending");
    expect(pauseToken).toBeDefined();

    model = speechReducer(model, {
      type: "pause_elapsed",
      sessionId: "session-1",
      timerToken: pauseToken!,
    });
    expect(model.state).toBe("finalizing");

    model = speechReducer(model, {
      type: "finalization_complete",
      sessionId: "session-1",
    });
    expect(model.state).toBe("idle");
    expect(model.sessionId).toBeUndefined();
    expect(model.finalTranscript).toBe("Hello world and more");
    expect(model.partialTranscript).toBe("");
    // Speech never auto-submits: no trailing Enter / newline from the lifecycle.
    expect(model.finalTranscript.includes("\n")).toBe(false);
    expect(model.finalTranscript.includes("\r")).toBe(false);
  });
});
