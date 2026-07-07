import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  connectTaskTerminal,
  type TerminalConnection,
  type TerminalConnectionStatus,
} from "./terminalConnection";

type SocketHandler = (event?: Event | MessageEvent) => void;

class MockWebSocket {
  static OPEN = 1;
  static instances: MockWebSocket[] = [];

  readyState = 0;
  url: string;
  private handlers = new Map<string, SocketHandler[]>();

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  addEventListener(type: string, handler: SocketHandler) {
    const list = this.handlers.get(type) ?? [];
    list.push(handler);
    this.handlers.set(type, list);
  }

  removeEventListener() {}

  close() {}

  send() {}

  fire(type: string, event?: Event | MessageEvent) {
    for (const handler of this.handlers.get(type) ?? []) {
      handler(event);
    }
  }
}

describe("connectTaskTerminal", () => {
  let connection: TerminalConnection;
  let statuses: TerminalConnectionStatus[] = [];
  let serverErrors: string[] = [];

  const IMMEDIATE_FAILURE_LIMIT = 5;

  beforeEach(() => {
    vi.useFakeTimers();
    MockWebSocket.instances = [];
    statuses = [];
    serverErrors = [];
    vi.stubGlobal("WebSocket", MockWebSocket);
  });

  afterEach(() => {
    connection?.dispose();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  function createConnection() {
    connection = connectTaskTerminal("test-handle", {
      onOutput: () => {},
      onServerError: (message) => {
        serverErrors.push(message);
      },
      onStatus: (status) => {
        statuses.push(status);
      },
      onOpen: () => {},
    });
    return connection;
  }

  function latestSocket(): MockWebSocket {
    const socket = MockWebSocket.instances.at(-1);
    if (!socket) throw new Error("expected a websocket dial");
    return socket;
  }

  function fireCloseWithoutOpen() {
    latestSocket().fire("close");
    vi.runOnlyPendingTimers();
  }

  it("gives up after repeated immediate failures", () => {
    createConnection();
    expect(MockWebSocket.instances).toHaveLength(1);

    for (let i = 0; i < IMMEDIATE_FAILURE_LIMIT; i += 1) {
      fireCloseWithoutOpen();
    }
    latestSocket().fire("close");

    expect(statuses.at(-1)).toBe("unavailable");

    const dialsBefore = MockWebSocket.instances.length;
    const statusesBefore = statuses.length;
    vi.advanceTimersByTime(60_000);
    expect(MockWebSocket.instances.length).toBe(dialsBefore);
    expect(statuses.slice(statusesBefore)).toEqual([]);
  });

  it("keeps reconnecting after a socket has opened", () => {
    createConnection();
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");

    for (let i = 0; i < 20; i += 1) {
      socket.fire("close");
      vi.runOnlyPendingTimers();
      expect(statuses.at(-1)).toBe("reconnecting");
      expect(statuses).not.toContain("unavailable");
      const next = latestSocket();
      next.readyState = MockWebSocket.OPEN;
      next.fire("open");
    }
  });

  it("treats a server error frame as unavailable", async () => {
    createConnection();
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");
    socket.fire(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({ type: "error", error: "tmux attach failed" }),
      }),
    );
    await Promise.resolve();
    socket.fire("close");

    expect(statuses.at(-1)).toBe("unavailable");
    expect(serverErrors).toEqual(["tmux attach failed"]);
  });

  it("manual reconnectNow retries from unavailable", () => {
    createConnection();

    for (let i = 0; i < IMMEDIATE_FAILURE_LIMIT; i += 1) {
      fireCloseWithoutOpen();
    }
    latestSocket().fire("close");
    expect(statuses.at(-1)).toBe("unavailable");

    const dialsBefore = MockWebSocket.instances.length;
    connection.reconnectNow();

    expect(statuses.at(-1)).toBe("connecting");
    expect(MockWebSocket.instances.length).toBeGreaterThan(dialsBefore);
  });
});
