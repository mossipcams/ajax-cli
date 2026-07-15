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

  it("first connect dials without a seed opt-out (seed)", () => {
    createConnection();
    expect(MockWebSocket.instances[0].url).not.toContain("seed=0");
  });

  it("automatic backoff reconnect dials with seed=0 (seed)", () => {
    const openCalls: Array<{ isReconnect: boolean; seeded: boolean }> = [];
    connection = connectTaskTerminal("test-handle", {
      onOutput: () => {},
      onServerError: () => {},
      onStatus: (status) => {
        statuses.push(status);
      },
      onOpen: (isReconnect, seeded) => {
        openCalls.push({ isReconnect, seeded });
      },
    });

    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");

    socket.fire("close");
    vi.advanceTimersByTime(1000);

    expect(MockWebSocket.instances[1].url).toContain("seed=0");
    const second = MockWebSocket.instances[1];
    second.readyState = MockWebSocket.OPEN;
    second.fire("open");

    expect(openCalls).toHaveLength(2);
    expect(openCalls[1]).toEqual({ isReconnect: true, seeded: false });
  });

  it("foreground visibility reconnect dials with seed=0 (seed)", () => {
    const openCalls: Array<{ isReconnect: boolean; seeded: boolean }> = [];
    connection = connectTaskTerminal("test-handle", {
      onOutput: () => {},
      onServerError: () => {},
      onStatus: (status) => {
        statuses.push(status);
      },
      onOpen: (isReconnect, seeded) => {
        openCalls.push({ isReconnect, seeded });
      },
    });

    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");

    socket.fire("close");

    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    document.dispatchEvent(new Event("visibilitychange"));

    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2);
    expect(MockWebSocket.instances[1].url).toContain("seed=0");

    const second = MockWebSocket.instances[1];
    second.readyState = MockWebSocket.OPEN;
    second.fire("open");

    expect(openCalls[1]).toEqual({ isReconnect: true, seeded: false });
  });

  it("manual reconnectNow dials a full seed (seed)", () => {
    const openCalls: Array<{ isReconnect: boolean; seeded: boolean }> = [];
    connection = connectTaskTerminal("test-handle", {
      onOutput: () => {},
      onServerError: () => {},
      onStatus: (status) => {
        statuses.push(status);
      },
      onOpen: (isReconnect, seeded) => {
        openCalls.push({ isReconnect, seeded });
      },
    });

    for (let i = 0; i < IMMEDIATE_FAILURE_LIMIT; i += 1) {
      fireCloseWithoutOpen();
    }
    latestSocket().fire("close");
    expect(statuses.at(-1)).toBe("unavailable");

    connection.reconnectNow();

    const newSocket = latestSocket();
    expect(newSocket.url).not.toContain("seed=0");

    newSocket.readyState = MockWebSocket.OPEN;
    newSocket.fire("open");

    expect(openCalls.at(-1)?.seeded).toBe(true);
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

  it("decodes binary ArrayBuffer output via TextDecoder into onOutput", async () => {
    const outputs: string[] = [];
    connection = connectTaskTerminal("test-handle", {
      onOutput: (text) => outputs.push(text),
      onServerError: () => {},
      onStatus: () => {},
      onOpen: () => {},
    });
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");

    const bytes = new TextEncoder().encode("λ binary");
    socket.fire(
      "message",
      new MessageEvent("message", { data: bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) }),
    );
    await Promise.resolve();
    await Promise.resolve();

    expect(outputs).toEqual(["λ binary"]);
  });

  it("decodes binary Blob output via TextDecoder into onOutput", async () => {
    const outputs: string[] = [];
    connection = connectTaskTerminal("test-handle", {
      onOutput: (text) => outputs.push(text),
      onServerError: () => {},
      onStatus: () => {},
      onOpen: () => {},
    });
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");

    socket.fire(
      "message",
      new MessageEvent("message", {
        data: new Blob([new TextEncoder().encode("blob pty")]),
      }),
    );

    await vi.waitFor(() => {
      expect(outputs).toEqual(["blob pty"]);
    });
  });

  it("still accepts legacy JSON base64 output frames", async () => {
    const outputs: string[] = [];
    connection = connectTaskTerminal("test-handle", {
      onOutput: (text) => outputs.push(text),
      onServerError: () => {},
      onStatus: () => {},
      onOpen: () => {},
    });
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");

    socket.fire(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({ type: "output", data: btoa("legacy ok") }),
      }),
    );
    await Promise.resolve();

    expect(outputs).toEqual(["legacy ok"]);
  });

  it("JSON error frames still call onServerError", async () => {
    createConnection();
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");
    socket.fire(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({ type: "error", error: "attach boom" }),
      }),
    );
    await Promise.resolve();

    expect(serverErrors).toEqual(["attach boom"]);
  });

  it("resize still sends JSON text", () => {
    createConnection();
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");

    const sent: unknown[] = [];
    socket.send = (data: unknown) => {
      sent.push(data);
    };

    connection.sendResize(120, 40);

    expect(sent).toHaveLength(1);
    expect(typeof sent[0]).toBe("string");
    expect(JSON.parse(String(sent[0]))).toEqual({ type: "resize", cols: 120, rows: 40 });
  });

  function connectForOutput(outputs: string[]) {
    connection = connectTaskTerminal("test-handle", {
      onOutput: (text) => outputs.push(text),
      onServerError: () => {},
      onStatus: () => {},
      onOpen: () => {},
    });
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");
    return socket;
  }

  function fireBinaryFrame(socket: MockWebSocket, bytes: Uint8Array) {
    socket.fire(
      "message",
      new MessageEvent("message", {
        data: bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
      }),
    );
  }

  it("reassembles split UTF-8 emoji bytes across consecutive binary frames", async () => {
    const outputs: string[] = [];
    const socket = connectForOutput(outputs);
    const emoji = "👋";
    const bytes = new TextEncoder().encode(emoji);
    fireBinaryFrame(socket, bytes.subarray(0, 2));
    fireBinaryFrame(socket, bytes.subarray(2));

    await vi.waitFor(() => {
      expect(outputs.join("")).toBe(emoji);
    });
    expect(outputs.join("")).toBe(emoji);
  });

  it("preserves unicode and control corpus order through binary frames", async () => {
    const outputs: string[] = [];
    const socket = connectForOutput(outputs);
    const corpus =
      "ASCII" +
      "😀" +
      "e\u0301" +
      "漢" +
      "\x1b[31mRED\x1b[0m" +
      "\x1b[2K" +
      "carriage\rreturn" +
      "line\nbreak" +
      "crlf\r\nend";

    for (const segment of [
      "ASCII",
      "😀",
      "e\u0301",
      "漢",
      "\x1b[31mRED\x1b[0m\x1b[2K",
      "carriage\rreturn",
      "line\nbreak",
      "crlf\r\nend",
    ]) {
      fireBinaryFrame(socket, new TextEncoder().encode(segment));
    }

    await vi.waitFor(() => {
      expect(outputs.join("")).toBe(corpus);
    });
    expect(outputs.join("")).toBe(corpus);
  });

  it("preserves rapid burst and large payload order without loss or duplication", async () => {
    const outputs: string[] = [];
    const socket = connectForOutput(outputs);
    const burst = Array.from({ length: 100 }, (_, index) => `chunk-${String(index).padStart(3, "0")}`);
    const large = "L".repeat(128 * 1024);
    const expected = burst.join("") + large;

    for (const chunk of burst) {
      fireBinaryFrame(socket, new TextEncoder().encode(chunk));
    }
    fireBinaryFrame(socket, new TextEncoder().encode(large));

    await vi.waitFor(() => {
      expect(outputs.join("")).toBe(expected);
    });
    expect(outputs.join("")).toBe(expected);
    expect(outputs.join("").length).toBe(expected.length);
  });
});
