import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";

vi.mock("./contracts", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./contracts")>();
  return {
    ...actual,
    MAX_FRAME_BYTES: 8192,
  };
});

import { snapshotJson, eventJson } from "./fixtures";
import {
  connectWebSessionTransport,
  parseServerFrame,
  type WebSessionTransportCallbacks,
  type WebSessionTransportPlatform,
  type WebSessionSocket,
} from "./public";
import { sessionSocketUrl } from "./commands";

const OPEN_READY_STATE = 1;

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
      for (const listener of listeners.get(type) ?? []) listener(event);
    },
  };
  return socket;
}

function platformFor(socket: ReturnType<typeof fakeSocket>): WebSessionTransportPlatform {
  return { openSocket: vi.fn(() => socket) };
}

function callbacks(): WebSessionTransportCallbacks {
  return {
    onReady: vi.fn(),
    onEvent: vi.fn(),
    onClosed: vi.fn(),
  };
}

describe("connectWebSessionTransport tail snapshots", () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("#1301 tail snapshot discards every replay envelope below the snapshot cursor", () => {
    const socket = fakeSocket();
    const onSnapshot = vi.fn();
    const onCursorAdvance = vi.fn();
    const cbs = { ...callbacks(), onSnapshot, onCursorAdvance };
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));

    const tail = [];
    for (let cursor = 100; cursor < 105; cursor += 1) {
      tail.push(
        JSON.parse(eventJson(cursor, { type: "message", role: "agent", text: `tail-${cursor}` })),
      );
    }

    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", {
      data: JSON.stringify({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 105,
        model: "auto",
        turnState: "idle",
        reset: true,
        transcriptFrom: 100,
        transcript: tail,
      }),
    } as MessageEvent);

    expect(onSnapshot).toHaveBeenCalledWith(
      expect.objectContaining({ cursor: 105, transcriptFrom: 100 }),
    );
    expect(cbs.onEvent).not.toHaveBeenCalled();

    for (let cursor = 0; cursor < 105; cursor += 1) {
      socket.emit("message", {
        data: eventJson(cursor, { type: "message", role: "agent", text: `replay-${cursor}` }),
      } as MessageEvent);
    }
    expect(cbs.onEvent).not.toHaveBeenCalled();

    socket.emit("message", {
      data: eventJson(105, { type: "message", role: "agent", text: "Live", itemId: "live" }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledTimes(1);
    expect(cbs.onEvent).toHaveBeenLastCalledWith(
      expect.objectContaining({ type: "message", text: "Live" }),
    );
    expect(onCursorAdvance).toHaveBeenLastCalledWith(106);
    transport.dispose();
  });

  it("#1301 tail snapshot still delivers socket-local error frames at cursor 0", () => {
    const socket = fakeSocket();
    const onSnapshot = vi.fn();
    const cbs = { ...callbacks(), onSnapshot };
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));

    const tail = [];
    for (let cursor = 100; cursor < 105; cursor += 1) {
      tail.push(
        JSON.parse(eventJson(cursor, { type: "message", role: "agent", text: `tail-${cursor}` })),
      );
    }

    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", {
      data: JSON.stringify({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 105,
        model: "auto",
        turnState: "idle",
        reset: true,
        transcriptFrom: 100,
        transcript: tail,
      }),
    } as MessageEvent);

    const error = { type: "error" as const, message: "prompt queue is full" };
    socket.emit("message", { data: eventJson(0, error) } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledTimes(1);
    expect(cbs.onEvent).toHaveBeenLastCalledWith(error);
    transport.dispose();
  });

  it("#1301 acknowledges a prompt_accepted below the floor without delivering it", () => {
    const socket = fakeSocket();
    const onSnapshot = vi.fn();
    const cbs = { ...callbacks(), onSnapshot };
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));

    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: snapshotJson({ cursor: 5 }) } as MessageEvent);
    const id = transport.sendPrompt("Stale ack");
    expect(id).not.toBe("");

    const tail = [];
    for (let cursor = 100; cursor < 105; cursor += 1) {
      tail.push(
        JSON.parse(eventJson(cursor, { type: "message", role: "agent", text: `tail-${cursor}` })),
      );
    }
    socket.emit("message", {
      data: JSON.stringify({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 105,
        model: "auto",
        turnState: "idle",
        reset: true,
        transcriptFrom: 100,
        transcript: tail,
      }),
    } as MessageEvent);

    vi.mocked(cbs.onEvent).mockClear();
    socket.emit("message", {
      data: eventJson(3, { type: "prompt_accepted", clientMessageId: id }),
    } as MessageEvent);
    expect(cbs.onEvent).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: "prompt_accepted" }),
    );
    expect(sessionStorage.getItem("ajax.web.session.outbox.web%2Ffix-login")).toBeNull();

    socket.emit("message", {
      data: eventJson(105, { type: "message", role: "agent", text: "Live" }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledTimes(1);
    transport.dispose();
  });

  it("#1301 discards sub-floor replay envelopes but delivers live events at the snapshot cursor", () => {
    const socket = fakeSocket();
    const onSnapshot = vi.fn();
    const cbs = { ...callbacks(), onSnapshot };
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));

    const tail = [];
    for (let cursor = 100; cursor < 105; cursor += 1) {
      tail.push(
        JSON.parse(eventJson(cursor, { type: "message", role: "agent", text: `tail-${cursor}` })),
      );
    }

    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", {
      data: JSON.stringify({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 105,
        model: "auto",
        turnState: "idle",
        reset: true,
        transcriptFrom: 100,
        transcript: tail,
      }),
    } as MessageEvent);

    for (let cursor = 0; cursor < 99; cursor += 1) {
      socket.emit("message", {
        data: eventJson(cursor, { type: "message", role: "agent", text: `stale-${cursor}` }),
      } as MessageEvent);
    }
    expect(cbs.onEvent).not.toHaveBeenCalled();

    socket.emit("message", {
      data: eventJson(105, { type: "message", role: "agent", text: "Live" }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledTimes(1);
    expect(cbs.onEvent).toHaveBeenLastCalledWith(
      expect.objectContaining({ type: "message", text: "Live" }),
    );
    transport.dispose();
  });

  it("sends snapshotTail=1 when a snapshot consumer is registered", () => {
    const socket = fakeSocket();
    const openSocket = vi.fn(() => socket);
    connectWebSessionTransport(
      "web/fix-login",
      { ...callbacks(), onSnapshot: vi.fn() },
      { openSocket },
      "composer-2.5",
      4,
    );
    expect(openSocket).toHaveBeenCalledWith(
      expect.stringContaining("/api/tasks/web%2Ffix-login/session?model=composer-2.5&cursor=4&snapshotTail=1"),
    );
  });

  it("sends snapshotTail=1 with cursor only when a snapshot consumer is registered", () => {
    const socket = fakeSocket();
    const openSocket = vi.fn(() => socket);
    connectWebSessionTransport(
      "web/fix-login",
      { ...callbacks(), onSnapshot: vi.fn() },
      { openSocket },
      undefined,
      7,
    );
    expect(openSocket).toHaveBeenCalledWith(
      expect.stringContaining("cursor=7&snapshotTail=1"),
    );
    expect(openSocket).not.toHaveBeenCalledWith(expect.stringContaining("model="));
  });

  it("sends snapshotTail=1 with no other params when a snapshot consumer is registered", () => {
    const socket = fakeSocket();
    const openSocket = vi.fn(() => socket);
    connectWebSessionTransport(
      "web/fix-login",
      { ...callbacks(), onSnapshot: vi.fn() },
      { openSocket },
    );
    expect(openSocket).toHaveBeenCalledWith(
      expect.stringContaining("/api/tasks/web%2Ffix-login/session?snapshotTail=1"),
    );
  });

  it("omits snapshotTail when no snapshot consumer is registered", () => {
    const socket = fakeSocket();
    const openSocket = vi.fn(() => socket);
    connectWebSessionTransport(
      "web/fix-login",
      callbacks(),
      { openSocket },
      "composer-2.5",
      4,
    );
    expect(openSocket).toHaveBeenCalledWith(
      expect.stringContaining("model=composer-2.5&cursor=4"),
    );
    expect(openSocket).not.toHaveBeenCalledWith(expect.stringContaining("snapshotTail"));
  });
});

describe("sessionSocketUrl", () => {
  it("omits snapshotTail by default and with false", () => {
    const base = sessionSocketUrl("web/fix-login", "composer-2.5", 4);
    expect(base).toContain("model=composer-2.5&cursor=4");
    expect(base).not.toContain("snapshotTail");
    expect(sessionSocketUrl("web/fix-login", "composer-2.5", 4, false)).toBe(base);
  });

  it("appends snapshotTail=1 after model and cursor only when true", () => {
    expect(sessionSocketUrl("web/fix-login", "composer-2.5", 4, true)).toContain(
      "model=composer-2.5&cursor=4&snapshotTail=1",
    );
    expect(sessionSocketUrl("web/fix-login", undefined, undefined, true)).toContain(
      "?snapshotTail=1",
    );
  });
});

describe("transcriptFrom validation in parseServerFrame", () => {
  const base = {
    type: "snapshot" as const,
    protocolVersion: 2,
    cursor: 105,
    model: "auto",
    turnState: "idle" as const,
    reset: true,
    transcriptFrom: 100,
    transcript: [
      JSON.parse(eventJson(100, { type: "message", role: "user", text: "Question" })),
      JSON.parse(eventJson(104, { type: "message", role: "agent", text: "Answer" })),
    ],
  };

  it("accepts a valid tail snapshot with transcriptFrom and exposes it on the frame", () => {
    const parsed = parseServerFrame(JSON.stringify(base));
    expect(parsed).not.toBeNull();
    if (!parsed || parsed.kind !== "snapshot") throw new Error("expected snapshot frame");
    expect(parsed.snapshot).toEqual(
      expect.objectContaining({ cursor: 105, transcriptFrom: 100 }),
    );
  });

  it("rejects bad transcriptFrom values", () => {
    expect(parseServerFrame(JSON.stringify({ ...base, transcript: undefined }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ ...base, transcriptFrom: 101 }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ ...base, transcriptFrom: 106 }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ ...base, transcriptFrom: -1 }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ ...base, transcriptFrom: 2.5 }))).toBeNull();
  });

  it("accepts legacy tail snapshots without transcriptFrom", () => {
    const legacy = { ...base, transcriptFrom: undefined };
    const parsed = parseServerFrame(JSON.stringify(legacy));
    expect(parsed).not.toBeNull();
    if (!parsed || parsed.kind !== "snapshot") throw new Error("expected snapshot frame");
    expect(parsed.snapshot).not.toHaveProperty("transcriptFrom");
  });
});

describe("transcriptFloor validation in parseServerFrame", () => {
  const base = {
    type: "snapshot" as const,
    protocolVersion: 2,
    cursor: 105,
    model: "auto",
    turnState: "idle" as const,
    reset: true,
    transcriptFrom: 100,
    transcriptFloor: 50,
    transcript: [
      JSON.parse(eventJson(100, { type: "message", role: "user", text: "Question" })),
      JSON.parse(eventJson(104, { type: "message", role: "agent", text: "Answer" })),
    ],
  };

  it("accepts a valid transcriptFloor and exposes it on the snapshot", () => {
    const parsed = parseServerFrame(JSON.stringify(base));
    expect(parsed).not.toBeNull();
    if (!parsed || parsed.kind !== "snapshot") throw new Error("expected snapshot frame");
    expect(parsed.snapshot).toEqual(
      expect.objectContaining({ cursor: 105, transcriptFrom: 100, transcriptFloor: 50 }),
    );
  });

  it("rejects a transcriptFloor without a transcript", () => {
    expect(parseServerFrame(JSON.stringify({ ...base, transcript: undefined }))).toBeNull();
  });

  it("rejects a transcriptFloor above transcriptFrom", () => {
    expect(parseServerFrame(JSON.stringify({ ...base, transcriptFloor: 101 }))).toBeNull();
  });

  it("rejects negative, non-integer, and string transcriptFloor values", () => {
    expect(parseServerFrame(JSON.stringify({ ...base, transcriptFloor: -1 }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ ...base, transcriptFloor: 2.5 }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ ...base, transcriptFloor: "50" }))).toBeNull();
  });

  it("omits transcriptFloor for legacy snapshots without the field", () => {
    const parsed = parseServerFrame(JSON.stringify({ ...base, transcriptFloor: undefined }));
    expect(parsed).not.toBeNull();
    if (!parsed || parsed.kind !== "snapshot") throw new Error("expected snapshot frame");
    expect(parsed.snapshot).not.toHaveProperty("transcriptFloor");
  });
});
