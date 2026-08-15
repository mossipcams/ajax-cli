import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import {
  connectWebSessionTransport,
  type WebSessionTransportCallbacks,
  type WebSessionTransportPlatform,
  type WebSessionSocket,
} from "./webSessionTransport";

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
      data: JSON.stringify({ type: "ready", model: "composer-2.5", busy: true }),
    } as MessageEvent);

    expect(cbs.onReady).toHaveBeenCalledWith("composer-2.5");
    expect(cbs.onEvent).toHaveBeenCalledWith({
      type: "ready",
      model: "composer-2.5",
      busy: true,
    });
    transport.sendPrompt("Ship it");
    expect(socket.sent.map((payload) => JSON.parse(payload))).toContainEqual(
      expect.objectContaining({ type: "prompt", text: "Ship it" }),
    );

    socket.emit("message", {
      data: JSON.stringify({ type: "message", role: "agent", text: "On it" }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledWith({
      type: "message",
      role: "agent",
      text: "On it",
    });

    transport.setModel("gpt-5.6-sol-medium");
    expect(socket.sent).toContainEqual(
      JSON.stringify({ type: "set_model", model: "gpt-5.6-sol-medium" }),
    );

    transport.dispose();
    expect(socket.close).toHaveBeenCalled();
  });

  it("queues prompts until ready", () => {
    const socket = fakeSocket();
    const transport = connectWebSessionTransport("web/fix-login", callbacks(), platformFor(socket));
    transport.sendPrompt("First");
    expect(socket.sent).toHaveLength(0);
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: JSON.stringify({ type: "ready" }) } as MessageEvent);
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
    socket.emit("message", { data: JSON.stringify({ type: "ready" }) } as MessageEvent);
    transport.sendPrompt("Ship it");

    const prompt = JSON.parse(socket.sent.at(-1) ?? "{}") as Record<string, unknown>;
    expect(prompt).toMatchObject({ type: "prompt", text: "Ship it" });
    expect(typeof prompt.clientMessageId).toBe("string");

    socket.emit("message", {
      data: JSON.stringify({ type: "prompt_accepted", clientMessageId: prompt.clientMessageId }),
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
    firstSocket.emit("message", { data: JSON.stringify({ type: "ready" }) } as MessageEvent);
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
    secondSocket.emit("message", { data: JSON.stringify({ type: "ready" }) } as MessageEvent);
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
    socket.emit("message", { data: JSON.stringify({ type: "ready" }) } as MessageEvent);
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
    socket.emit("message", { data: JSON.stringify({ type: "ready" }) } as MessageEvent);
    expect(socket.sent).not.toContainEqual(JSON.stringify({ type: "prompt", text: "Queued" }));
    transport.dispose();
  });

  it("sendCancel(true) keeps pre-ready pending prompts for flush", () => {
    const socket = fakeSocket();
    const transport = connectWebSessionTransport("web/fix-login", callbacks(), platformFor(socket));
    transport.sendPrompt("Queued");
    transport.sendCancel(true);
    socket.readyState = OPEN_READY_STATE;
    socket.emit("message", { data: JSON.stringify({ type: "ready" }) } as MessageEvent);
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
    socket.emit("message", { data: JSON.stringify({ type: "ready" }) } as MessageEvent);
    expect(socket.sent).not.toContainEqual(JSON.stringify({ type: "prompt", text: "Queued" }));
    transport.dispose();
  });
});
