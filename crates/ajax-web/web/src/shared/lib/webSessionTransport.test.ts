import { describe, it, expect, vi, afterEach } from "vitest";
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
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("opens the task session websocket and handles ready + prompt", () => {
    const socket = fakeSocket();
    const cbs = callbacks();
    const transport = connectWebSessionTransport("web/fix-login", cbs, platformFor(socket));

    socket.readyState = OPEN_READY_STATE;
    socket.emit("open");
    socket.emit("message", { data: JSON.stringify({ type: "ready" }) } as MessageEvent);

    expect(cbs.onReady).toHaveBeenCalledOnce();
    transport.sendPrompt("Ship it");
    expect(socket.sent).toContainEqual(JSON.stringify({ type: "prompt", text: "Ship it" }));

    socket.emit("message", {
      data: JSON.stringify({ type: "message", role: "agent", text: "On it" }),
    } as MessageEvent);
    expect(cbs.onEvent).toHaveBeenCalledWith({
      type: "message",
      role: "agent",
      text: "On it",
    });

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
    expect(socket.sent).toContainEqual(JSON.stringify({ type: "prompt", text: "First" }));
    transport.dispose();
  });
});
