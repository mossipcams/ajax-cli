import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import type { RefObject } from "react";
import type { Terminal } from "@xterm/xterm";
import { useTaskTerminalSpeech } from "./useTaskTerminalSpeech";
import type { TerminalConnection } from "@/shared/lib/terminalConnection";

const speechTransports: Array<{
  sessionId: string;
  cancel: ReturnType<typeof vi.fn>;
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
      _callbacks: unknown,
      _platform: unknown,
      options?: { sessionId?: string },
    ) => {
      const sessionId = options?.sessionId ?? `speech-session-${sessionCounter}`;
      const cancel = vi.fn();
      speechTransports.push({ sessionId, cancel });
      return {
        start: vi.fn(async () => {}),
        stop: vi.fn(),
        cancel,
        sessionId: () => sessionId,
      };
    },
  };
});

vi.mock("@/shared/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/shared/lib/api")>();
  return { ...actual, renewBrowserSession: vi.fn(async () => {}) };
});

function deps() {
  return {
    termRef: { current: null } as RefObject<Terminal | null>,
    connectionRef: { current: null } as RefObject<TerminalConnection | null>,
    pasteThroughTerm: () => false,
  };
}

describe("useTaskTerminalSpeech", () => {
  beforeEach(() => {
    speechTransports.length = 0;
    sessionCounter = 0;
  });
  afterEach(() => vi.clearAllMocks());

  it("ignores a second same-turn mic toggle so transport session matches model", async () => {
    const { result } = renderHook(() =>
      useTaskTerminalSpeech({ handle: "web/fix-login", ...deps() }),
    );

    await act(async () => {
      result.current.toggleMic();
      result.current.toggleMic();
    });

    expect(speechTransports).toHaveLength(1);
    expect(result.current.speechModel.sessionId).toBe(speechTransports[0]?.sessionId);
    expect(result.current.speechModel.state).toBe("connecting");
  });
});
