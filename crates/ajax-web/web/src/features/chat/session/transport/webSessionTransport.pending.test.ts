import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";

import { eventJson } from "./fixtures";
import { parseServerFrame } from "./parse";
import {
  connectWebSessionTransport,
  type WebSessionTransportCallbacks,
  type WebSessionTransportPlatform,
  type WebSessionSocket,
} from "./public";

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

function pendingSnapshotJson(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    type: "snapshot",
    protocolVersion: 2,
    cursor: 105,
    model: "auto",
    turnState: "idle",
    reset: true,
    agentPending: true,
    ...overrides,
  });
}

function nonPendingSnapshotJson(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    type: "snapshot",
    protocolVersion: 2,
    cursor: 105,
    model: "auto",
    turnState: "idle",
    reset: false,
    ...overrides,
  });
}

function tailEvents(): unknown[] {
  const tail = [];
  for (let cursor = 100; cursor < 105; cursor += 1) {
    tail.push(JSON.parse(eventJson(cursor, { type: "message", role: "agent", text: `tail-${cursor}` })));
  }
  return tail;
}

describe("connectWebSessionTransport early agentPending snapshots", () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("#1356 pending reset snapshot consumes the inline transcript without becoming ready", () => {
    const socket = fakeSocket();
    const onSnapshot = vi.fn();
    const onCursorAdvance = vi.fn();
    const onAgentPending = vi.fn();
    const cbs = { ...callbacks(), onSnapshot, onCursorAdvance, onAgentPending };
    const transport = connectWebSessionTransport("web/pending", cbs, platformFor(socket));

    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", {
      data: pendingSnapshotJson({ transcriptFrom: 100, transcript: tailEvents() }),
    } as MessageEvent);

    expect(onAgentPending).toHaveBeenCalledTimes(1);
    expect(cbs.onReady).not.toHaveBeenCalled();
    // No synthetic ready event while the harness is still spawning.
    expect(cbs.onEvent).not.toHaveBeenCalled();
    expect(onSnapshot).toHaveBeenCalledWith(
      expect.objectContaining({
        cursor: 105,
        agentPending: true,
        transcriptFrom: 100,
        transcript: expect.any(Array),
      }),
    );
    // Inline-transcript consumption keeps the existing cursor bookkeeping.
    expect(onCursorAdvance).toHaveBeenCalledTimes(1);
    expect(onCursorAdvance).toHaveBeenCalledWith(105);

    const id = transport.sendPrompt("Queued");
    expect(id).not.toBe("");
    const prompts = socket.sent.filter((frame) => JSON.parse(frame).type === "prompt");
    expect(prompts).toHaveLength(0);
    transport.dispose();
  });

  it("#1356 the following non-pending snapshot becomes ready once and dedupes inline transcript envelopes", () => {
    const socket = fakeSocket();
    const onCursorAdvance = vi.fn();
    const onAgentPending = vi.fn();
    const cbs = { ...callbacks(), onSnapshot: vi.fn(), onCursorAdvance, onAgentPending };
    const transport = connectWebSessionTransport("web/pending", cbs, platformFor(socket));

    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", {
      data: pendingSnapshotJson({ transcriptFrom: 100, transcript: tailEvents() }),
    } as MessageEvent);

    const id = transport.sendPrompt("Queued");
    expect(id).not.toBe("");
    expect(socket.sent.filter((frame) => JSON.parse(frame).type === "prompt")).toHaveLength(0);

    socket.emit("message", { data: nonPendingSnapshotJson() } as MessageEvent);

    expect(cbs.onReady).toHaveBeenCalledTimes(1);
    const prompts = socket.sent.filter((frame) => JSON.parse(frame).type === "prompt");
    expect(prompts).toHaveLength(1);
    expect(JSON.parse(prompts[0]).clientMessageId).toBe(id);

    // Envelopes whose cursor sits inside the early inline transcript dedupe.
    vi.mocked(cbs.onEvent).mockClear();
    for (let cursor = 100; cursor < 105; cursor += 1) {
      socket.emit("message", {
        data: eventJson(cursor, { type: "message", role: "agent", text: `replay-${cursor}` }),
      } as MessageEvent);
    }
    expect(cbs.onEvent).not.toHaveBeenCalled();

    socket.emit("message", {
      data: eventJson(105, { type: "message", role: "agent", text: "Live" }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledWith(expect.objectContaining({ type: "message", text: "Live" }));
    expect(onCursorAdvance).toHaveBeenLastCalledWith(106);
    transport.dispose();
  });

  it("#1356 pending reset:false snapshot without a transcript does not advance the cursor or become ready", () => {
    const socket = fakeSocket();
    const onSnapshot = vi.fn();
    const onCursorAdvance = vi.fn();
    const onAgentPending = vi.fn();
    const cbs = { ...callbacks(), onSnapshot, onCursorAdvance, onAgentPending };
    connectWebSessionTransport("web/pending", cbs, platformFor(socket));

    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: pendingSnapshotJson({ reset: false }) } as MessageEvent);

    expect(onAgentPending).toHaveBeenCalledTimes(1);
    expect(cbs.onReady).not.toHaveBeenCalled();
    expect(onCursorAdvance).not.toHaveBeenCalled();
    expect(cbs.onEvent).not.toHaveBeenCalled();

    // The normal snapshot completes the first-ready path.
    socket.emit("message", { data: nonPendingSnapshotJson() } as MessageEvent);
    expect(cbs.onReady).toHaveBeenCalledTimes(1);

    // A pending snapshot arriving when ready is already true must not call onReady again.
    socket.emit("message", { data: pendingSnapshotJson({ cursor: 106 }) } as MessageEvent);
    expect(cbs.onReady).toHaveBeenCalledTimes(1);
    expect(onAgentPending).toHaveBeenCalledTimes(2);
  });
});

describe("agentPending parsing in parseServerFrame", () => {
  const base = {
    type: "snapshot" as const,
    protocolVersion: 2,
    cursor: 5,
    model: "auto",
    turnState: "idle" as const,
    reset: false,
  };

  it("exposes agentPending only when it is boolean true", () => {
    const parsed = parseServerFrame(JSON.stringify({ ...base, agentPending: true }));
    expect(parsed).not.toBeNull();
    if (!parsed || parsed.kind !== "snapshot") throw new Error("expected snapshot frame");
    expect(parsed.snapshot.agentPending).toBe(true);
  });

  it("omits agentPending for false and non-boolean values", () => {
    for (const value of [false, "true", 1, null]) {
      const parsed = parseServerFrame(JSON.stringify({ ...base, agentPending: value }));
      expect(parsed).not.toBeNull();
      if (!parsed || parsed.kind !== "snapshot") throw new Error("expected snapshot frame");
      expect(parsed.snapshot.agentPending).toBeUndefined();
    }
  });

  it("omits agentPending for legacy snapshots without the field", () => {
    const parsed = parseServerFrame(JSON.stringify(base));
    expect(parsed).not.toBeNull();
    if (!parsed || parsed.kind !== "snapshot") throw new Error("expected snapshot frame");
    expect(parsed.snapshot.agentPending).toBeUndefined();
  });
});
