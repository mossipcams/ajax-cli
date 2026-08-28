import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as webSessionTransport from "./transport/public";
import {
  SESSION_MODEL_STORAGE_KEY,
  writeSessionModel,
} from "@/features/task/public";
import { useChatSession } from "./useChatSession";

function mockTransport(
  callbacks: webSessionTransport.WebSessionTransportCallbacks[],
): webSessionTransport.WebSessionTransport {
  const transport: webSessionTransport.WebSessionTransport = {
    sendPrompt: vi.fn(() => "prompt-1"),
    sendCancel: vi.fn(),
    setModel: vi.fn(),
    setConfigOption: vi.fn(),
    respondPermission: vi.fn(),
    respondElicitation: vi.fn(),
    dispose: vi.fn(),
  };
  vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
    (_handle, nextCallbacks) => {
      callbacks.push(nextCallbacks);
      return transport;
    },
  );
  return transport;
}

describe("useChatSession", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    sessionStorage.clear();
    localStorage.clear();
  });

  it("clears the session outbox when the task mutates", () => {
    localStorage.setItem(
      "ajax.web.session.outbox.web%2Ffix-login",
      JSON.stringify([{ text: "queued", clientMessageId: "c1" }]),
    );
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    const transport = mockTransport(callbacks);
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, nextCallbacks) => {
        callbacks.push(nextCallbacks);
        nextCallbacks.onReady("auto");
        return transport;
      },
    );

    const onMutated = vi.fn();
    const { result, unmount } = renderHook(() =>
      useChatSession({
        handle: "web/fix-login",
        detail: null,
        onMutated,
      }),
    );

    act(() => result.current.onMutated());

    expect(localStorage.getItem("ajax.web.session.outbox.web%2Ffix-login")).toBeNull();
    expect(onMutated).toHaveBeenCalledOnce();
    unmount();
  });

  it("keeps pessimistic chips until the host snapshot confirms (#931)", () => {
    writeSessionModel("composer-2.5");
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    const transport = mockTransport(callbacks);

    const { result, unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null }),
    );

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "gpt-5.6-sol-medium",
        sessionConfigOptions: [
          {
            id: "model",
            category: "model",
            name: "Model",
            type: "select",
            currentValue: "gpt-5.6-sol-medium",
            choices: [
              { value: "gpt-5.6-sol-medium", name: "GPT 5.6" },
              { value: "claude-opus-5", name: "Opus" },
            ],
          },
        ],
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
      }),
    );
    expect(result.current.sessionModel).toBe("gpt-5.6-sol-medium");

    act(() => result.current.applyConfigOption("model", "claude-opus-5"));
    expect(result.current.sessionModel).toBe("gpt-5.6-sol-medium");
    expect(transport.setConfigOption).toHaveBeenCalledWith("model", "claude-opus-5");

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 1,
        model: "claude-opus-5",
        sessionConfigOptions: [
          {
            id: "model",
            category: "model",
            name: "Model",
            type: "select",
            currentValue: "claude-opus-5",
            choices: [
              { value: "gpt-5.6-sol-medium", name: "GPT 5.6" },
              { value: "claude-opus-5", name: "Opus" },
            ],
          },
        ],
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
      }),
    );
    expect(result.current.sessionModel).toBe("claude-opus-5");
    expect(localStorage.getItem(SESSION_MODEL_STORAGE_KEY)).toBe("claude-opus-5");
    unmount();
  });

  it("keeps the host model when a snapshot advertises only Fast", () => {
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    mockTransport(callbacks);
    const { result, unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null }),
    );

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "composer-2.5",
        sessionConfigOptions: [
          {
            id: "fast",
            category: "model_config",
            name: "Fast",
            type: "boolean",
            currentValue: false,
            choices: [],
          },
        ],
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
      }),
    );

    expect(result.current.sessionModel).toBe("composer-2.5");
    unmount();
  });

  it("surfaces config apply failures without moving chips", () => {
    const onConfigError = vi.fn();
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    mockTransport(callbacks);

    const { unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null, onConfigError }),
    );

    act(() =>
      callbacks[0]?.onEvent({
        type: "error",
        message: "config option model was refused — harness is running gpt-5.6-sol-medium",
      }),
    );
    expect(onConfigError).toHaveBeenCalled();
    unmount();
  });

  it("does not seed the in-session picker from localStorage (#931)", () => {
    writeSessionModel("composer-2.5");
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    mockTransport(callbacks);

    const { result, unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null }),
    );

    act(() => callbacks[0]?.onReady("gpt-5.6-sol-medium"));
    expect(result.current.sessionModel).toBe("gpt-5.6-sol-medium");
    expect(result.current.sessionModel).not.toBe("composer-2.5");
    unmount();
  });

  it("keeps pessimistic Cursor catalog picks until the host snapshot confirms", () => {
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    const transport = mockTransport(callbacks);

    const { result, unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null }),
    );

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "cursor-grok-4.6-high",
        sessionConfigOptions: [
          {
            id: "model",
            category: "model",
            name: "Model",
            type: "select",
            currentValue: "cursor-grok-4.6-high",
            choices: [{ value: "cursor-grok-4.6-high", name: "Grok 4.6 High" }],
          },
        ],
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
      }),
    );
    expect(result.current.sessionModel).toBe("cursor-grok-4.6-high");

    act(() => result.current.applyModel("cursor-grok-4.6-xhigh"));
    expect(result.current.sessionModel).toBe("cursor-grok-4.6-high");
    expect(transport.setModel).toHaveBeenCalledWith("cursor-grok-4.6-xhigh");

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 1,
        model: "cursor-grok-4.6-xhigh",
        sessionConfigOptions: [
          {
            id: "model",
            category: "model",
            name: "Model",
            type: "select",
            currentValue: "cursor-grok-4.6-xhigh",
            choices: [{ value: "cursor-grok-4.6-xhigh", name: "Grok 4.6 Extra High" }],
          },
        ],
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
      }),
    );
    expect(result.current.sessionModel).toBe("cursor-grok-4.6-xhigh");
    unmount();
  });

  it("clears the permission head immediately when the operator answers (#1018)", () => {
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    const transport = mockTransport(callbacks);

    const { result, unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null }),
    );

    act(() => callbacks[0]?.onReady("auto"));
    act(() =>
      callbacks[0]?.onEvent({
        type: "permission_request",
        requestId: "7",
        title: "Run tests?",
        detail: "cargo test",
      }),
    );
    expect(result.current.view.permission.decision).toEqual({
      requestId: "7",
      title: "Run tests?",
      detail: "cargo test",
    });

    act(() => result.current.respondPermission(true));
    expect(transport.respondPermission).toHaveBeenCalledWith("7", true);
    expect(result.current.view.permission.decision).toBeNull();
    expect(result.current.view.conversation[0]).toMatchObject({
      kind: "permission",
      requestId: "7",
      resolved: true,
    });
    unmount();
  });

  it("clears the elicitation head immediately when the operator answers", () => {
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    const transport = mockTransport(callbacks);
    const schema = {
      type: "object",
      properties: { target: { type: "string", title: "Target" } },
      required: ["target"],
    };

    const { result, unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null }),
    );

    act(() => callbacks[0]?.onReady("auto"));
    act(() =>
      callbacks[0]?.onEvent({
        type: "elicitation_request",
        requestId: "e7",
        message: "Pick a target",
        schema,
      }),
    );
    expect(result.current.view.elicitation.decision).toMatchObject({
      requestId: "e7",
      message: "Pick a target",
    });

    act(() => result.current.respondElicitation("accept", { target: "staging" }));
    expect(transport.respondElicitation).toHaveBeenCalledWith("e7", "accept", {
      target: "staging",
    });
    expect(result.current.view.elicitation.decision).toBeNull();
    expect(result.current.view.conversation[0]).toMatchObject({
      kind: "elicitation",
      requestId: "e7",
      resolved: true,
    });
    unmount();
  });

  it("keeps advertised config chips until the host snapshot confirms", () => {
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    const transport = mockTransport(callbacks);
    const liveOptions = [
      {
        id: "model",
        category: "model",
        name: "Model",
        type: "select",
        currentValue: "grok-4.6",
        choices: [
          { value: "grok-4.6", name: "Grok 4.6" },
          { value: "gpt-5.6-sol", name: "GPT-5.6-Sol" },
        ],
      },
    ];

    const { result, unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null }),
    );

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "grok-4.6",
        sessionConfigOptions: liveOptions,
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
      }),
    );
    expect(result.current.sessionModel).toBe("grok-4.6");
    expect(result.current.sessionConfigOptions?.[0]?.currentValue).toBe("grok-4.6");

    act(() => result.current.applyConfigOption("model", "gpt-5.6-sol"));
    expect(result.current.sessionModel).toBe("grok-4.6");
    expect(result.current.sessionConfigOptions?.[0]?.currentValue).toBe("grok-4.6");
    expect(transport.setConfigOption).toHaveBeenCalledWith("model", "gpt-5.6-sol");

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 1,
        model: "gpt-5.6-sol",
        sessionConfigOptions: [{ ...liveOptions[0]!, currentValue: "gpt-5.6-sol" }],
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
      }),
    );
    expect(result.current.sessionModel).toBe("gpt-5.6-sol");
    expect(result.current.sessionConfigOptions?.[0]?.currentValue).toBe("gpt-5.6-sol");
    unmount();
  });

  it("projects host context continuity into session view state", () => {
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    mockTransport(callbacks);

    const { result, unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null }),
    );

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState: "restored",
        contextEpoch: 3,
      }),
    );
    expect(result.current.view.context).toEqual({ state: "restored", epoch: 3 });

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 1,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState: "unavailable",
        contextEpoch: 3,
        contextError: "resume timed out",
      }),
    );
    expect(result.current.view.context).toEqual({
      state: "unavailable",
      epoch: 3,
      error: "resume timed out",
    });
    unmount();
  });

  it("projects transcriptError from host snapshots into session view", () => {
    const callbacks: webSessionTransport.WebSessionTransportCallbacks[] = [];
    mockTransport(callbacks);

    const { result, unmount } = renderHook(() =>
      useChatSession({ handle: "web/fix-login", detail: null }),
    );

    act(() =>
      callbacks[0]?.onSnapshot({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        contextState: "live",
        contextEpoch: 1,
        transcriptError: "forced append failure",
      }),
    );
    expect(result.current.view.context).toEqual({
      state: "live",
      epoch: 1,
      transcriptError: "forced append failure",
    });
    unmount();
  });
});
