import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { snapshotJson, eventJson } from "./fixtures";
import {
  connectWebSessionTransport,
  parseServerEvent,
  parseServerFrame,
  PROMPT_TOO_LONG,
  MAX_FRAME_BYTES,
  readSessionCursor,
  writeSessionCursor,
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

describe("connectWebSessionTransport", () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("opens the task session websocket and handles ready + prompt", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    const openSocket = vi.fn(() => socket);
    const transport = connectWebSessionTransport(
      "web/fix-login",
      cbs,
      { openSocket },
      "composer-2.5",
    );

    expect(openSocket).toHaveBeenCalledWith(
      expect.stringContaining("/api/tasks/web%2Ffix-login/session?model=composer-2.5"),
    );

    socket.readyState = OPEN_READY_STATE;
    socket.emit("open");
    socket.emit("message", {
      data: snapshotJson({ model: "composer-2.5", turnState: "busy" }),
    } as MessageEvent);

    expect(cbs.onReady).toHaveBeenCalledWith("composer-2.5");
    expect(cbs.onEvent).toHaveBeenCalledTimes(1);
    expect(cbs.onEvent).toHaveBeenCalledWith({
      type: "ready",
      model: "composer-2.5",
      busy: true,
      reset: false,
    });
    transport.sendPrompt("Ship it");
    expect(socket.sent.map((payload) => JSON.parse(payload))).toContainEqual(
      expect.objectContaining({ type: "prompt", text: "Ship it" }),
    );

    socket.emit("message", {
      data: eventJson(0, { type: "message", role: "agent", text: "On it", itemId: "i1" }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledWith({
      type: "message",
      role: "agent",
      text: "On it",
      itemId: "i1",
    });

    transport.setModel("gpt-5.6-sol-medium");
    expect(socket.sent).toContainEqual(
      JSON.stringify({ type: "set_model", model: "gpt-5.6-sol-medium" }),
    );

    transport.setConfigOption("model", "composer-2.5");
    expect(socket.sent).toContainEqual(
      JSON.stringify({ type: "set_config_option", configId: "model", value: "composer-2.5" }),
    );

    transport.dispose();
    expect(socket.close).toHaveBeenCalled();
  });

  it("omits ?model= from the reconnect URL unless a model is pinned", () => {
    const socket = fakeSocket();
    const openSocket = vi.fn(() => socket);
    connectWebSessionTransport("web/fix-login", callbacks(), { openSocket });
    expect(openSocket).toHaveBeenCalledWith(
      expect.stringMatching(/\/api\/tasks\/web%2Ffix-login\/session$/),
    );
    expect(openSocket).not.toHaveBeenCalledWith(expect.stringContaining("?model="));
  });

  it("queues prompts until ready", () => {
    const socket = fakeSocket();
    const transport = connectWebSessionTransport("web/fix-login", callbacks(), platformFor(socket));
    transport.sendPrompt("First");
    expect(socket.sent).toHaveLength(0);
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: snapshotJson() } as MessageEvent);
    expect(socket.sent.map((payload) => JSON.parse(payload))).toContainEqual(
      expect.objectContaining({ type: "prompt", text: "First" }),
    );
    transport.dispose();
  });

  it("assigns prompt ids and handles host acceptance acknowledgements", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    const transport = connectWebSessionTransport(
      "web/fix-login",
      cbs,
      platformFor(socket),
      "auto",
    );

    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: snapshotJson() } as MessageEvent);
    transport.sendPrompt("Ship it");

    const prompt = JSON.parse(socket.sent.at(-1) ?? "{}") as Record<string, unknown>;
    expect(prompt).toMatchObject({ type: "prompt", text: "Ship it" });
    expect(typeof prompt.clientMessageId).toBe("string");

    socket.emit("message", {
      data: eventJson(1, {
        type: "prompt_accepted",
        clientMessageId: prompt.clientMessageId as string,
      }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledWith({
      type: "prompt_accepted",
      clientMessageId: prompt.clientMessageId,
    });
    transport.dispose();
  });

  it("retries an unacknowledged prompt on a new transport", () => {
    const firstSocket = fakeSocket();
    const first = connectWebSessionTransport(
      "web/fix-login",
      callbacks(),
      platformFor(firstSocket),
    );
    firstSocket.readyState = OPEN_READY_STATE;
    firstSocket.emit("message", { data: snapshotJson() } as MessageEvent);
    first.sendPrompt("Retry me");
    const firstPrompt = JSON.parse(firstSocket.sent.at(-1) ?? "{}") as Record<string, string>;
    first.dispose();

    const secondSocket = fakeSocket();
    const second = connectWebSessionTransport(
      "web/fix-login",
      callbacks(),
      platformFor(secondSocket),
    );
    secondSocket.readyState = OPEN_READY_STATE;
    secondSocket.emit("message", { data: snapshotJson() } as MessageEvent);
    expect(JSON.parse(secondSocket.sent.at(-1) ?? "{}")).toEqual({
      type: "prompt",
      text: "Retry me",
      clientMessageId: firstPrompt.clientMessageId,
    });
    second.dispose();
  });

  it("sendCancel(true) sends keepQueue on the wire", () => {
    const socket = fakeSocket();
    socket.readyState = OPEN_READY_STATE;
    const transport = connectWebSessionTransport("web/fix-login", callbacks(), platformFor(socket));
    socket.emit("message", { data: snapshotJson() } as MessageEvent);
    transport.sendCancel(true);
    expect(socket.sent).toContainEqual(JSON.stringify({ type: "cancel", keepQueue: true }));
    transport.sendCancel();
    expect(socket.sent).toContainEqual(JSON.stringify({ type: "cancel" }));
    transport.dispose();
  });

  it("calls onClosed once when error is followed by close", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));
    socket.emit("error");
    socket.emit("close");
    expect(cbs.onClosed).toHaveBeenCalledOnce();
  });

  it("sendCancel() clears pre-ready pending prompts before flush", () => {
    const socket = fakeSocket();
    const transport = connectWebSessionTransport("web/fix-login", callbacks(), platformFor(socket));
    transport.sendPrompt("Queued");
    transport.sendCancel();
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: snapshotJson() } as MessageEvent);
    expect(socket.sent).not.toContainEqual(JSON.stringify({ type: "prompt", text: "Queued" }));
    transport.dispose();
  });

  it("sendCancel(true) keeps pre-ready pending prompts for flush", () => {
    const socket = fakeSocket();
    const transport = connectWebSessionTransport("web/fix-login", callbacks(), platformFor(socket));
    transport.sendPrompt("Queued");
    transport.sendCancel(true);
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: snapshotJson() } as MessageEvent);
    expect(socket.sent.map((payload) => JSON.parse(payload))).toContainEqual(
      expect.objectContaining({ type: "prompt", text: "Queued" }),
    );
    transport.dispose();
  });

  it("clears pre-ready pending prompts when the socket fails to open", async () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));
    transport.sendPrompt("Queued");
    socket.emit("error");
    await Promise.resolve();
    expect(cbs.onEvent).toHaveBeenCalledWith({
      type: "error",
      message: "Session WebSocket failed to open",
    });
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: snapshotJson() } as MessageEvent);
    expect(socket.sent).not.toContainEqual(JSON.stringify({ type: "prompt", text: "Queued" }));
    transport.dispose();
  });

  // #929: ordinary pastes above the old 4096-byte ceiling must send.
  it("accepts a prompt larger than the former 4096-byte frame limit", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));

    const id = transport.sendPrompt("x".repeat(5000));

    expect(id).not.toBe("");
    expect(cbs.onEvent).not.toHaveBeenCalledWith({ type: "error", message: PROMPT_TOO_LONG });
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: snapshotJson() } as MessageEvent);
    expect(socket.sent.map((payload) => JSON.parse(payload))).toContainEqual(
      expect.objectContaining({ type: "prompt", text: "x".repeat(5000) }),
    );
    transport.dispose();
  });

  // The host rejects a frame over its ceiling before it can read the frame's
  // clientMessageId, so the prompt is never acknowledged. Queued, it was resent
  // on every reconnect and rejected every time — one long paste poisoned the
  // session permanently, surviving reloads in sessionStorage.
  it("refuses a prompt too large for the host frame limit instead of queueing it", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));

    const id = transport.sendPrompt("x".repeat(MAX_FRAME_BYTES));

    expect(id).toBe("");
    expect(cbs.onEvent).toHaveBeenCalledWith({ type: "error", message: PROMPT_TOO_LONG });
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: snapshotJson() } as MessageEvent);
    expect(socket.sent.filter((payload) => payload.includes('"type":"prompt"'))).toEqual([]);
    expect(sessionStorage.getItem("ajax.web.session.outbox.web%2Ffix-login")).toBeNull();
    transport.dispose();
  });

  it("discards an already-poisoned oversized prompt from a stored outbox", () => {
    sessionStorage.setItem(
      "ajax.web.session.outbox.web%2Ffix-login",
      JSON.stringify([
        { text: "x".repeat(MAX_FRAME_BYTES), clientMessageId: "poison" },
        { text: "fine", clientMessageId: "keep" },
      ]),
    );
    const socket = fakeSocket();
    const transport = connectWebSessionTransport("web/fix-login", callbacks(), platformFor(socket));

    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: snapshotJson() } as MessageEvent);

    const prompts = socket.sent.map((payload) => JSON.parse(payload));
    expect(prompts).toContainEqual(expect.objectContaining({ clientMessageId: "keep" }));
    expect(prompts).not.toContainEqual(expect.objectContaining({ clientMessageId: "poison" }));
    transport.dispose();
  });

  it("advances next-to-read cursor via callback, not sessionStorage", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    let nextToRead: number | undefined;
    const transport = connectWebSessionTransport(
      "web/fix-login",
      { ...cbs, onCursorAdvance: (cursor) => { nextToRead = cursor; } },
      platformFor(socket),
    );
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", {
      data: snapshotJson({ cursor: 5 }),
    } as MessageEvent);
    expect(nextToRead).toBeUndefined();
    expect(readSessionCursor("web/fix-login")).toBeUndefined();
    socket.emit("message", {
      data: eventJson(3, { type: "message", role: "agent", text: "tail", itemId: "i3" }),
    } as MessageEvent);
    expect(nextToRead).toBe(4);
    expect(readSessionCursor("web/fix-login")).toBeUndefined();
    transport.dispose();
  });

  it("cold attach omits cursor even when sessionStorage holds a legacy value", () => {
    writeSessionCursor("web/fix-login", 2);
    const socket = fakeSocket();
    const openSocket = vi.fn(() => socket);
    connectWebSessionTransport("web/fix-login", callbacks(), { openSocket });
    expect(openSocket).toHaveBeenCalledWith(
      expect.not.stringContaining("cursor="),
    );
    expect(readSessionCursor("web/fix-login")).toBeUndefined();
  });

  it("in-page reconnect supplies resume cursor as next-to-read", () => {
    const socket = fakeSocket();
    const openSocket = vi.fn(() => socket);
    connectWebSessionTransport("web/fix-login", callbacks(), { openSocket }, undefined, 2);
    expect(openSocket).toHaveBeenCalledWith(expect.stringContaining("cursor=2"));
  });

  it("#994 reapplies idle snapshot state after the replay tail", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));
    socket.readyState = OPEN_READY_STATE;

    socket.emit("message", {
      data: snapshotJson({ cursor: 3, model: "gpt-5.6-sol", turnState: "idle" }),
    } as MessageEvent);
    socket.emit("message", {
      data: eventJson(1, { type: "prompt_accepted", clientMessageId: "historical" }),
    } as MessageEvent);
    socket.emit("message", {
      data: eventJson(2, {
        type: "message",
        role: "agent",
        text: "Historical tail",
        itemId: "i-history",
      }),
    } as MessageEvent);

    expect(cbs.onEvent).toHaveBeenLastCalledWith({
      type: "ready",
      model: "gpt-5.6-sol",
      busy: false,
      reset: false,
    });
  });

  it("#994 does not reapply the snapshot to live events at its cursor", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));
    socket.readyState = OPEN_READY_STATE;

    socket.emit("message", {
      data: snapshotJson({ cursor: 3, turnState: "idle" }),
    } as MessageEvent);
    socket.emit("message", {
      data: eventJson(3, {
        type: "message",
        role: "agent",
        text: "Live update",
        itemId: "i-live",
      }),
    } as MessageEvent);

    expect(cbs.onEvent).toHaveBeenCalledTimes(2);
    expect(cbs.onEvent).toHaveBeenLastCalledWith({
      type: "message",
      role: "agent",
      text: "Live update",
      itemId: "i-live",
    });
  });

  it("applies pendingPermission from snapshot", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", {
      data: snapshotJson({
        pendingPermission: { requestId: "p1", title: "Run tests?" },
      }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledWith(
      expect.objectContaining({ type: "ready", reset: false }),
    );
    expect(cbs.onEvent).toHaveBeenCalledWith({
      type: "permission_request",
      requestId: "p1",
      title: "Run tests?",
    });
    transport.dispose();
  });

  it("applies pendingElicitation from snapshot", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", {
      data: snapshotJson({
        pendingElicitation: {
          requestId: "e1",
          message: "Pick env",
          schema: { type: "object", properties: {} },
        },
      }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledWith({
      type: "elicitation_request",
      requestId: "e1",
      message: "Pick env",
      schema: { type: "object", properties: {} },
    });
    transport.dispose();
  });

  it("sends elicitation accept with content", () => {
    const socket = fakeSocket();
    const transport = connectWebSessionTransport("web/fix-login", callbacks(), platformFor(socket));
    socket.readyState = OPEN_READY_STATE;
    transport.respondElicitation("e1", "accept", { target: "staging" });
    expect(JSON.parse(String(socket.sent[0]))).toEqual({
      type: "elicitation",
      requestId: "e1",
      action: "accept",
      content: { target: "staging" },
    });
    transport.dispose();
  });
});

describe("parseServerEvent", () => {
  it("maps snapshot reset onto ready", () => {
    expect(parseServerEvent(snapshotJson({ reset: true }))).toEqual({
      type: "ready",
      model: "auto",
      busy: false,
      reset: true,
    });
  });
});

describe("parseServerFrame", () => {
  it("accepts well-formed variants", () => {
    expect(parseServerFrame(snapshotJson({ model: "auto" }))).toEqual({
      kind: "snapshot",
      snapshot: expect.objectContaining({ model: "auto", turnState: "idle" }),
    });
    expect(
      parseServerFrame(
        JSON.stringify({
          type: "snapshot",
          protocolVersion: 2,
          cursor: 1,
          model: "grok-4.6",
          turnState: "idle",
          reset: false,
          sessionConfigOptions: [
            {
              id: "model",
              category: "model",
              name: "Model",
              type: "select",
              currentValue: "grok-4.6",
              choices: [{ value: "grok-4.6", name: "Grok 4.6" }],
            },
          ],
        }),
      ),
    ).toEqual({
      kind: "snapshot",
      snapshot: expect.objectContaining({
        model: "grok-4.6",
        sessionConfigOptions: [
          expect.objectContaining({ id: "model", currentValue: "grok-4.6" }),
        ],
      }),
    });
    expect(
      parseServerFrame(
        eventJson(0, {
          type: "message",
          role: "agent",
          text: "Hi",
          itemId: "i1",
          messageId: "m1",
        }),
      ),
    ).toEqual({
      kind: "event",
      cursor: 0,
      event: { type: "message", role: "agent", text: "Hi", itemId: "i1", messageId: "m1" },
    });
    expect(
      parseServerFrame(
        eventJson(0, { type: "prompt_accepted", clientMessageId: "c1" }),
      ),
    ).toEqual({
      kind: "event",
      cursor: 0,
      event: { type: "prompt_accepted", clientMessageId: "c1" },
    });
    expect(
      parseServerFrame(
        eventJson(0, {
          type: "tool_call",
          callId: "t1",
          title: "Read",
          kind: "read",
          status: "completed",
        }),
      ),
    ).toMatchObject({
      kind: "event",
      event: { type: "tool_call", callId: "t1" },
    });
    expect(
      parseServerFrame(eventJson(0, { type: "permission_request", requestId: "p1" })),
    ).toEqual({
      kind: "event",
      cursor: 0,
      event: { type: "permission_request", requestId: "p1" },
    });
    expect(
      parseServerFrame(
        eventJson(0, {
          type: "elicitation_request",
          requestId: "e1",
          message: "Pick env",
          schema: { type: "object", properties: {} },
        }),
      ),
    ).toEqual({
      kind: "event",
      cursor: 0,
      event: {
        type: "elicitation_request",
        requestId: "e1",
        message: "Pick env",
        schema: { type: "object", properties: {} },
      },
    });
    expect(parseServerFrame(eventJson(0, { type: "error", message: "nope" }))).toEqual({
      kind: "event",
      cursor: 0,
      event: { type: "error", message: "nope" },
    });
    expect(
      parseServerFrame(
        eventJson(0, {
          type: "turn_usage",
          inputTokens: 12,
          totalTokens: 12,
        }),
      ),
    ).toEqual({
      kind: "event",
      cursor: 0,
      event: { type: "turn_usage", inputTokens: 12, totalTokens: 12 },
    });
    expect(
      parseServerFrame(
        eventJson(0, {
          type: "turn_usage",
          requestId: "req-1",
        }),
      ),
    ).toEqual({
      kind: "event",
      cursor: 0,
      event: { type: "turn_usage", requestId: "req-1" },
    });
    expect(
      parseServerFrame(eventJson(0, { type: "turn_usage" })),
    ).toBeNull();
  });

  it("drops invalid JSON and variants missing required fields", () => {
    expect(parseServerFrame("not json")).toBeNull();
    expect(parseServerFrame(JSON.stringify({ type: "message", role: "agent" }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ type: "prompt_accepted" }))).toBeNull();
    expect(
      parseServerFrame(JSON.stringify({ type: "tool_call", callId: "t1", title: "x" })),
    ).toBeNull();
    expect(parseServerFrame(JSON.stringify({ type: "permission_request" }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ type: "error" }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ type: "unknown" }))).toBeNull();
    expect(parseServerFrame(JSON.stringify({ type: "snapshot", protocolVersion: 1 }))).toBeNull();
  });

  it("drops malformed event frames without dispatching them", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", {
      data: JSON.stringify({ type: "event", protocolVersion: 2, cursor: 0, payload: { type: "message", role: "agent" } }),
    } as MessageEvent);
    expect(cbs.onEvent).not.toHaveBeenCalled();
    socket.emit("message", { data: snapshotJson() } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledTimes(1);
    transport.dispose();
  });
});
