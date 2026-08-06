import { describe, expect, it, vi } from "vitest";
import {
  connectWebSession,
  composeWebSessionPrompt,
  clarifyWebSessionError,
  webSessionSocketUrl,
  type WebSessionTransportCallbacks,
  type WebSessionTransportPlatform,
  type WebSessionSocket,
} from "./webSessionTransport";
import { WEB_SESSION_PROTOCOL_VERSION } from "./types";
import type { WebSessionSymbolContext } from "./types";

type Listener = (event: Event | MessageEvent) => void;

function fakeSocket(): WebSessionSocket & {
  sent: string[];
  emit(type: string, event?: Event | MessageEvent): void;
} {
  const listeners = new Map<string, Set<Listener>>();
  const socket = {
    readyState: 0,
    sent: [] as string[],
    send(data: string) {
      this.sent.push(data);
    },
    close: vi.fn(),
    addEventListener(type: string, listener: Listener) {
      const set = listeners.get(type) ?? new Set<Listener>();
      set.add(listener);
      listeners.set(type, set);
    },
    removeEventListener(type: string, listener: Listener) {
      listeners.get(type)?.delete(listener);
    },
    emit(type: string, event: Event | MessageEvent = new Event(type)) {
      if (type === "open") {
        this.readyState = 1;
      }
      for (const listener of listeners.get(type) ?? []) listener(event);
    },
  };
  return socket;
}

function callbacks(): WebSessionTransportCallbacks & {
  [K in keyof WebSessionTransportCallbacks]: ReturnType<typeof vi.fn>;
} {
  return {
    onConnectionStatus: vi.fn(),
    onSessionReady: vi.fn(),
    onRunStatus: vi.fn(),
    onAssistantDelta: vi.fn(),
    onProgress: vi.fn(),
    onSettled: vi.fn(),
    onError: vi.fn(),
    onClosed: vi.fn(),
  };
}

function platformFor(socket: ReturnType<typeof fakeSocket>): WebSessionTransportPlatform {
  return {
    openSocket: vi.fn(() => socket),
  };
}

const symbol: WebSessionSymbolContext = {
  id: "src/session.rs:4:start_session",
  name: "start_session",
  kind: "method",
  path: "src/session.rs",
  startLine: 4,
  endLine: 6,
  preview: "pub fn start_session(&self) -> bool {",
  source: "pub fn start_session(&self) -> bool {\n    true\n}",
};

describe("webSessionSocketUrl", () => {
  it("builds same-origin task-scoped web-session URL", () => {
    expect(webSessionSocketUrl("web/fix-login")).toBe(
      `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/api/tasks/web%2Ffix-login/web-session`,
    );
  });
});

describe("composeWebSessionPrompt", () => {
  it("returns the user message when no symbols are attached", () => {
    expect(composeWebSessionPrompt("hello agent", [])).toBe("hello agent");
  });

  it("prepends attached symbol context before the question", () => {
    const prompt = composeWebSessionPrompt("fix this", [symbol]);
    expect(prompt).toContain("## Attached context");
    expect(prompt).toContain("src/session.rs — method `start_session`");
    expect(prompt).toContain("pub fn start_session(&self) -> bool {");
    expect(prompt).toContain("## Question");
    expect(prompt).toContain("fix this");
  });
});

describe("connectWebSession", () => {
  it("opens socket, sends prompt, streams deltas, and settles", () => {
    const socket = fakeSocket();
    const events = callbacks();
    const transport = connectWebSession("web/fix-login", events, platformFor(socket));

    socket.emit("open");
    expect(events.onConnectionStatus).toHaveBeenCalledWith("connected");

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "session.ready",
          version: WEB_SESSION_PROTOCOL_VERSION,
          sessionId: "sess-1",
        }),
      }),
    );
    expect(events.onSessionReady).toHaveBeenCalledWith("sess-1");

    transport.sendPrompt("hello");
    expect(socket.sent).toHaveLength(1);
    expect(JSON.parse(socket.sent[0]!)).toEqual({
      type: "session.prompt",
      version: WEB_SESSION_PROTOCOL_VERSION,
      message: "hello",
    });

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "session.status",
          version: WEB_SESSION_PROTOCOL_VERSION,
          state: "running",
        }),
      }),
    );
    expect(events.onRunStatus).toHaveBeenCalledWith("running");

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "session.assistant_delta",
          version: WEB_SESSION_PROTOCOL_VERSION,
          text: "Hi ",
        }),
      }),
    );
    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "session.assistant_delta",
          version: WEB_SESSION_PROTOCOL_VERSION,
          text: "there",
        }),
      }),
    );
    expect(events.onAssistantDelta).toHaveBeenNthCalledWith(1, "Hi ");
    expect(events.onAssistantDelta).toHaveBeenNthCalledWith(2, "there");

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "session.progress",
          version: WEB_SESSION_PROTOCOL_VERSION,
          kind: "tool",
          toolName: "cargo test",
          status: "running",
          summary: "Running focused tests",
          path: "crates/ajax-core",
        }),
      }),
    );
    expect(events.onProgress).toHaveBeenCalledWith({
      kind: "tool",
      toolName: "cargo test",
      status: "running",
      summary: "Running focused tests",
      path: "crates/ajax-core",
    });

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "session.settled",
          version: WEB_SESSION_PROTOCOL_VERSION,
        }),
      }),
    );
    expect(events.onSettled).toHaveBeenCalled();

    transport.dispose();
  });

  it("sends abort when stop is requested", () => {
    const socket = fakeSocket();
    const events = callbacks();
    const transport = connectWebSession("web/fix-login", events, platformFor(socket));
    socket.emit("open");

    transport.sendAbort();
    expect(socket.sent).toHaveLength(1);
    expect(JSON.parse(socket.sent[0]!)).toEqual({
      type: "session.abort",
      version: WEB_SESSION_PROTOCOL_VERSION,
    });

    transport.dispose();
  });

  it("surfaces server errors and reconnects after close", () => {
    vi.useFakeTimers();
    const sockets: ReturnType<typeof fakeSocket>[] = [];
    const platform: WebSessionTransportPlatform = {
      openSocket: vi.fn(() => {
        const next = fakeSocket();
        sockets.push(next);
        return next;
      }),
    };
    const events = callbacks();
    const transport = connectWebSession("web/fix-login", events, platform);
    const socket = sockets[0]!;
    socket.emit("open");

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "session.error",
          version: WEB_SESSION_PROTOCOL_VERSION,
          code: "provider_error",
          message: "boom",
        }),
      }),
    );
    expect(events.onRunStatus).toHaveBeenCalledWith("waiting");
    expect(events.onError).toHaveBeenCalledWith("boom");

    // Drop after a stable open → reconnecting, then redial.
    vi.setSystemTime(Date.now() + 2000);
    socket.emit("close");
    expect(events.onConnectionStatus).toHaveBeenCalledWith("reconnecting");
    vi.runOnlyPendingTimers();
    expect(platform.openSocket).toHaveBeenCalledTimes(2);
    sockets[1]!.emit("open");
    expect(events.onConnectionStatus).toHaveBeenCalledWith("connected");

    transport.dispose();
    vi.useRealTimers();
  });

  it("fails fatally after immediate never-open dials are exhausted", () => {
    vi.useFakeTimers();
    const sockets: ReturnType<typeof fakeSocket>[] = [];
    const platform: WebSessionTransportPlatform = {
      openSocket: vi.fn(() => {
        const next = fakeSocket();
        sockets.push(next);
        return next;
      }),
    };
    const events = callbacks();
    const transport = connectWebSession("web/fix-login", events, platform);

    for (let i = 0; i < 6; i += 1) {
      sockets.at(-1)!.emit("close");
      vi.runOnlyPendingTimers();
    }

    expect(events.onConnectionStatus).toHaveBeenCalledWith("error");
    expect(events.onError).toHaveBeenCalledWith(
      expect.stringContaining("Web session connection failed"),
    );

    transport.dispose();
    vi.useRealTimers();
  });

  it("clarifies auth and spawn errors for operators", () => {
    expect(clarifyWebSessionError("authenticate cursor_login failed")).toMatch(/Cursor login/);
    expect(clarifyWebSessionError("failed to spawn agent")).toMatch(/binary not available/);
  });

  it("forwards attention.required and attention.respond", () => {
    const socket = fakeSocket();
    const events = callbacks();
    events.onAttentionRequired = vi.fn();
    events.onAttentionCleared = vi.fn();
    const transport = connectWebSession("web/fix-login", events, platformFor(socket));
    socket.emit("open");

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "attention.required",
          version: WEB_SESSION_PROTOCOL_VERSION,
          handle: "web/other",
          requestId: "7",
          kind: "permission",
          title: "Permission needed",
          summary: "Permission: Run tests",
          options: ["allow-once", "reject"],
        }),
      }),
    );
    expect(events.onAttentionRequired).toHaveBeenCalledWith({
      handle: "web/other",
      requestId: "7",
      kind: "permission",
      title: "Permission needed",
      summary: "Permission: Run tests",
      options: ["allow-once", "reject"],
    });

    transport.respondAttention("web/other", "7", {
      type: "permission",
      outcome: "allow-once",
    });
    expect(JSON.parse(socket.sent[0]!)).toEqual({
      type: "attention.respond",
      version: WEB_SESSION_PROTOCOL_VERSION,
      targetHandle: "web/other",
      requestId: "7",
      response: { type: "permission", outcome: "allow-once" },
    });

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "attention.cleared",
          version: WEB_SESSION_PROTOCOL_VERSION,
          handle: "web/other",
          requestId: "7",
        }),
      }),
    );
    expect(events.onAttentionCleared).toHaveBeenCalledWith("web/other", "7");

    transport.dispose();
  });
});
