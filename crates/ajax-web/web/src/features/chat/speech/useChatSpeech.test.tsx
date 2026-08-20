import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import type { RefObject } from "react";
import { useChatSpeech } from "./useChatSpeech";

const speechTransports: Array<{
  sessionId: string;
  cancel: ReturnType<typeof vi.fn>;
  callbacks: {
    onFinal?: (sequence: number, text: string) => void;
  };
}> = [];
let sessionCounter = 0;

vi.mock("@/shared/lib/speechTransport", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/shared/lib/speechTransport")>();
  return {
    ...actual,
    createBrowserSpeechPlatform: () => ({}),
    newSessionId: () => {
      sessionCounter += 1;
      return `speech-session-${sessionCounter}`;
    },
    createSpeechTransport: (
      _handle: string,
      callbacks: {
        onReady?: (payload: { pauseGracePeriodMs: number }) => void;
        onFinal?: (sequence: number, text: string) => void;
      },
      _platform: unknown,
      options?: { sessionId?: string },
    ) => {
      const sessionId = options?.sessionId ?? `speech-session-${sessionCounter}`;
      const cancel = vi.fn();
      speechTransports.push({ sessionId, cancel, callbacks });
      return {
        start: vi.fn(async () => {}),
        stop: vi.fn(),
        cancel,
        sessionId: () => sessionId,
      };
    },
  };
});

function mountSpeech(initialDraft = "") {
  const draftRef = { current: initialDraft };
  const setDraft = vi.fn((value: string) => {
    draftRef.current = value;
  });
  const view = renderHook(() =>
    useChatSpeech({
      handle: "web/fix-login",
      draftRef: draftRef as RefObject<string>,
      setDraft,
    }),
  );
  return { view, draftRef, setDraft };
}

describe("useChatSpeech", () => {
  beforeEach(() => {
    speechTransports.length = 0;
    sessionCounter = 0;
  });

  afterEach(() => vi.clearAllMocks());

  it("ignores a second same-turn mic toggle so transport session matches model", async () => {
    const { view } = mountSpeech();

    await act(async () => {
      view.result.current.toggleMic();
      view.result.current.toggleMic();
    });

    expect(speechTransports).toHaveLength(1);
    expect(view.result.current.speechModel.sessionId).toBe(speechTransports[0]?.sessionId);
    expect(view.result.current.speechModel.state).toBe("connecting");
  });

  it("inserts finalized transcript deltas into the draft with spacing", async () => {
    const { view, draftRef, setDraft } = mountSpeech("hello");

    await act(async () => {
      view.result.current.toggleMic();
    });

    await act(async () => {
      speechTransports[0]?.callbacks.onReady?.({ pauseGracePeriodMs: 9000 });
      speechTransports[0]?.callbacks.onFinal?.(0, "world");
    });

    expect(draftRef.current).toBe("hello world");
    expect(setDraft).toHaveBeenCalledWith("hello world");
  });

  it("does not auto-submit finalized speech into the session outbox", async () => {
    const { view, draftRef } = mountSpeech();

    await act(async () => {
      view.result.current.toggleMic();
    });

    await act(async () => {
      speechTransports[0]?.callbacks.onReady?.({ pauseGracePeriodMs: 9000 });
      speechTransports[0]?.callbacks.onFinal?.(0, "please fix the test");
    });

    expect(draftRef.current).toBe("please fix the test");
  });
});
