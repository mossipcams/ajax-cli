import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  connectTaskTerminal,
  type TerminalConnection,
  type TerminalConnectionStatus,
} from "./terminalConnection";

// The socket renews the browser session once per disconnected episode. Stub it
// so tests drive the outcome instead of hitting fetch.
const renewBrowserSession = vi.fn<() => Promise<void>>();
vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return { ...actual, renewBrowserSession: () => renewBrowserSession() };
});

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
    renewBrowserSession.mockReset();
    renewBrowserSession.mockResolvedValue(undefined);
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

  async function fireCloseWithoutOpen() {
    latestSocket().fire("close");
    // Let the one-shot session renewal settle before running backoff timers.
    await vi.advanceTimersByTimeAsync(0);
    vi.runOnlyPendingTimers();
  }

  /** Spend the one-shot session-renewal redial that the first failed dial buys. */
  async function exhaustSessionRenewalRetry() {
    await fireCloseWithoutOpen();
  }

  it("gives up after repeated immediate failures", async () => {
    createConnection();
    expect(MockWebSocket.instances).toHaveLength(1);

    await exhaustSessionRenewalRetry();
    for (let i = 0; i < IMMEDIATE_FAILURE_LIMIT; i += 1) {
      await fireCloseWithoutOpen();
    }
    latestSocket().fire("close");

    expect(statuses.at(-1)).toBe("unavailable");

    const dialsBefore = MockWebSocket.instances.length;
    const statusesBefore = statuses.length;
    vi.advanceTimersByTime(60_000);
    expect(MockWebSocket.instances.length).toBe(dialsBefore);
    expect(statuses.slice(statusesBefore)).toEqual([]);
  });

  // A stale session cookie fails the WebSocket upgrade with a 401 the browser
  // never exposes. Without renewing, the socket burns its retry budget and
  // latches to "unavailable" for good.
  it("renews the browser session and redials when the first dial never opens", async () => {
    createConnection();
    expect(MockWebSocket.instances).toHaveLength(1);

    latestSocket().fire("close");
    await vi.advanceTimersByTimeAsync(0);

    expect(renewBrowserSession).toHaveBeenCalledTimes(1);
    // Redial is immediate — it must not wait out the backoff.
    expect(MockWebSocket.instances).toHaveLength(2);
    // Nothing was seeded on the failed dial, so the retry still asks for history.
    expect(MockWebSocket.instances[1].url).not.toContain("seed=0");
    expect(statuses.at(-1)).toBe("connecting");
  });

  it("renews at most once per disconnected episode", async () => {
    createConnection();

    await fireCloseWithoutOpen();
    await fireCloseWithoutOpen();
    await fireCloseWithoutOpen();

    expect(renewBrowserSession).toHaveBeenCalledTimes(1);
  });

  it("falls back to backoff when session renewal fails", async () => {
    renewBrowserSession.mockRejectedValue(new Error("offline"));
    createConnection();

    latestSocket().fire("close");
    await vi.advanceTimersByTimeAsync(0);

    expect(renewBrowserSession).toHaveBeenCalledTimes(1);
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(statuses.at(-1)).toBe("reconnecting");

    vi.runOnlyPendingTimers();
    expect(MockWebSocket.instances).toHaveLength(2);
  });

  // Mobile Safari drops the socket on background; that is not an auth failure
  // and must not cost a session renewal.
  it("does not renew when an established socket drops", async () => {
    createConnection();
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");

    socket.fire("close");
    await vi.advanceTimersByTimeAsync(0);

    expect(renewBrowserSession).not.toHaveBeenCalled();
  });

  // A renewed session after a reconnect episode must be available again later.
  it("re-arms the renewal after a successful open", async () => {
    createConnection();

    await fireCloseWithoutOpen();
    expect(renewBrowserSession).toHaveBeenCalledTimes(1);

    const reopened = latestSocket();
    reopened.readyState = MockWebSocket.OPEN;
    reopened.fire("open");

    reopened.fire("close");
    await vi.advanceTimersByTimeAsync(0);
    vi.runOnlyPendingTimers();
    latestSocket().fire("close");
    await vi.advanceTimersByTimeAsync(0);

    expect(renewBrowserSession).toHaveBeenCalledTimes(2);
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

  it("manual reconnectNow dials a full seed (seed)", async () => {
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

    await exhaustSessionRenewalRetry();
    for (let i = 0; i < IMMEDIATE_FAILURE_LIMIT; i += 1) {
      await fireCloseWithoutOpen();
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

  it("manual reconnectNow retries from unavailable", async () => {
    createConnection();

    await exhaustSessionRenewalRetry();
    for (let i = 0; i < IMMEDIATE_FAILURE_LIMIT; i += 1) {
      await fireCloseWithoutOpen();
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

  it("chunks large input into frames <=4096 bytes with multibyte char across chunk boundary", () => {
    const sent: Uint8Array[] = [];
    const socket = connectForOutput([]);
    socket.send = (data: unknown) => {
      expect(ArrayBuffer.isView(data) || data instanceof ArrayBuffer).toBe(true);
      const view = data as Uint8Array;
      const copy = new Uint8Array(view.byteLength);
      copy.set(new Uint8Array(view.buffer, view.byteOffset, view.byteLength));
      sent.push(copy);
    };

    // Build a payload exceeding 4096 encoded bytes with a multibyte character
    // whose encoded bytes straddle the 4096 chunk boundary.
    const filler = "x".repeat(4095);
    const boundary = "λ"; // U+03BB, 2 encoded bytes → bytes [4095, 4097) cross 4096
    const tail = "y".repeat(100);
    const payload = filler + boundary + tail;
    const expected = new TextEncoder().encode(payload);

    connection.sendInput(payload);

    expect(sent.length).toBeGreaterThan(1);

    // Every frame must be within the bridge limit.
    for (const frame of sent) {
      expect(frame.byteLength).toBeLessThanOrEqual(4096);
    }

    // Concatenated bytes must exactly equal the original encoding.
    const total = sent.reduce((sum, f) => sum + f.byteLength, 0);
    expect(total).toBe(expected.byteLength);
    const reassembled = new Uint8Array(total);
    let offset = 0;
    for (const frame of sent) {
      reassembled.set(frame, offset);
      offset += frame.byteLength;
    }
    expect(Array.from(reassembled)).toEqual(Array.from(expected));

    // Round-trip through a stream decoder must reproduce the original string.
    expect(new TextDecoder().decode(reassembled)).toBe(payload);
  });

  it("ordinary small input remains a single frame", () => {
    const sent: Uint8Array[] = [];
    const socket = connectForOutput([]);
    socket.send = (data: unknown) => {
      const view = data as Uint8Array;
      const copy = new Uint8Array(view.byteLength);
      copy.set(new Uint8Array(view.buffer, view.byteOffset, view.byteLength));
      sent.push(copy);
    };

    connection.sendInput("small");

    expect(sent).toHaveLength(1);
    expect(new TextDecoder().decode(sent[0])).toBe("small");
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
