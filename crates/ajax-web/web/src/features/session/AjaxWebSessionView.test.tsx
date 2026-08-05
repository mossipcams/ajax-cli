import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import AjaxWebSessionView from "./AjaxWebSessionView";
import { WEB_SESSION_PROTOCOL_VERSION } from "./types";
import { fetchTaskSymbols } from "@/shared/lib/api";
import type { WebSessionSymbolContext } from "./types";

vi.mock("@/shared/lib/api", () => ({
  fetchTaskSymbols: vi.fn(),
}));

const sampleSymbol: WebSessionSymbolContext = {
  id: "src/session.rs:2:start_session",
  name: "start_session",
  kind: "function",
  path: "src/session.rs",
  startLine: 2,
  endLine: 4,
  preview: "pub fn start_session() {",
  source: "pub fn start_session() {\n}\n",
};

type SocketHandler = (event?: Event | MessageEvent) => void;

class MockWebSocket {
  static OPEN = 1;
  static instances: MockWebSocket[] = [];

  readyState = 0;
  url: string;
  sent: string[] = [];
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

  removeEventListener(type: string, handler: SocketHandler) {
    const list = this.handlers.get(type);
    if (!list) return;
    const index = list.indexOf(handler);
    if (index >= 0) list.splice(index, 1);
  }

  close() {
    this.readyState = 3;
    this.fire("close");
  }

  send(data: string) {
    this.sent.push(data);
  }

  fire(type: string, event?: Event | MessageEvent) {
    if (type === "open") {
      this.readyState = MockWebSocket.OPEN;
    }
    for (const handler of this.handlers.get(type) ?? []) {
      handler(event);
    }
  }
}

function latestSocket(): MockWebSocket {
  const socket = MockWebSocket.instances.at(-1);
  if (!socket) throw new Error("expected a websocket dial");
  return socket;
}

function serverEvent(body: Record<string, unknown>) {
  return new MessageEvent("message", { data: JSON.stringify(body) });
}

describe("AjaxWebSessionView", () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.stubGlobal("WebSocket", MockWebSocket);
    vi.mocked(fetchTaskSymbols).mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("connects, shows user message on send, streams assistant reply, and settles", async () => {
    render(<AjaxWebSessionView handle="web/fix-login" />);
    expect(screen.getByTestId("ajax-web-session")).toBeInTheDocument();
    expect(latestSocket().url).toBe(
      `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/api/tasks/web%2Ffix-login/web-session`,
    );

    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");
    socket.fire(
      "message",
      serverEvent({
        type: "session.ready",
        version: WEB_SESSION_PROTOCOL_VERSION,
        sessionId: "sess-1",
      }),
    );
    socket.fire(
      "message",
      serverEvent({
        type: "session.status",
        version: WEB_SESSION_PROTOCOL_VERSION,
        state: "waiting",
      }),
    );

    const input = screen.getByTestId("ajax-web-session-input");
    fireEvent.change(input, { target: { value: "hello agent" } });
    fireEvent.click(screen.getByTestId("ajax-web-session-send"));

    expect(screen.getByText("hello agent")).toBeInTheDocument();
    expect(JSON.parse(socket.sent[0]!)).toEqual({
      type: "session.prompt",
      version: WEB_SESSION_PROTOCOL_VERSION,
      message: "hello agent",
    });

    await waitFor(() => {
      expect(screen.getByTestId("ajax-web-session-stop")).toBeInTheDocument();
    });

    socket.fire(
      "message",
      serverEvent({
        type: "session.status",
        version: WEB_SESSION_PROTOCOL_VERSION,
        state: "running",
      }),
    );

    socket.fire(
      "message",
      serverEvent({
        type: "session.assistant_delta",
        version: WEB_SESSION_PROTOCOL_VERSION,
        text: "Hello ",
      }),
    );
    socket.fire(
      "message",
      serverEvent({
        type: "session.assistant_delta",
        version: WEB_SESSION_PROTOCOL_VERSION,
        text: "back",
      }),
    );
    await waitFor(() => {
      expect(screen.getByText("Hello back")).toBeInTheDocument();
    });

    socket.fire(
      "message",
      serverEvent({
        type: "session.settled",
        version: WEB_SESSION_PROTOCOL_VERSION,
      }),
    );
    socket.fire(
      "message",
      serverEvent({
        type: "session.status",
        version: WEB_SESSION_PROTOCOL_VERSION,
        state: "waiting",
      }),
    );
    await waitFor(() => {
      expect(screen.getByTestId("ajax-web-session-send")).toBeInTheDocument();
    });
  });

  it("sends abort from the stop button while running", async () => {
    render(<AjaxWebSessionView handle="web/fix-login" />);
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");
    socket.fire(
      "message",
      serverEvent({
        type: "session.status",
        version: WEB_SESSION_PROTOCOL_VERSION,
        state: "running",
      }),
    );

    await waitFor(() => {
      expect(screen.getByTestId("ajax-web-session-stop")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId("ajax-web-session-stop"));

    expect(JSON.parse(socket.sent[0]!)).toEqual({
      type: "session.abort",
      version: WEB_SESSION_PROTOCOL_VERSION,
    });
  });

  it("shows error state when the socket fails", async () => {
    render(<AjaxWebSessionView handle="web/fix-login" />);
    const socket = latestSocket();
    socket.fire("error");

    await waitFor(() => {
      expect(screen.getByTestId("ajax-web-session-error")).toHaveTextContent(
        "Web session connection failed",
      );
    });
    expect(screen.getByTestId("ajax-web-session-status")).toHaveTextContent("Error");
  });

  it("attaches symbol context chips and includes source in the sent prompt", async () => {
    vi.mocked(fetchTaskSymbols).mockResolvedValue([sampleSymbol]);
    render(<AjaxWebSessionView handle="web/fix-login" />);
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");
    socket.fire(
      "message",
      serverEvent({
        type: "session.status",
        version: WEB_SESSION_PROTOCOL_VERSION,
        state: "waiting",
      }),
    );

    await waitFor(() => {
      expect(screen.getByTestId("ajax-web-session-add-context")).not.toBeDisabled();
    });

    fireEvent.click(screen.getByTestId("ajax-web-session-add-context"));
    fireEvent.change(screen.getByTestId("symbol-search-input"), {
      target: { value: "start" },
    });

    await waitFor(() => {
      expect(screen.getByTestId(`symbol-search-row-${sampleSymbol.id}`)).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId(`symbol-search-row-${sampleSymbol.id}`));
    fireEvent.click(screen.getByTestId("symbol-search-confirm"));

    expect(screen.getByTestId("ajax-web-session-context-chips")).toBeInTheDocument();
    expect(screen.getByText("start_session()")).toBeInTheDocument();

    fireEvent.change(screen.getByTestId("ajax-web-session-input"), {
      target: { value: "explain this" },
    });
    fireEvent.click(screen.getByTestId("ajax-web-session-send"));

    const sent = JSON.parse(socket.sent[0]!);
    expect(sent.type).toBe("session.prompt");
    expect(sent.message).toContain("## Attached context");
    expect(sent.message).toContain("start_session");
    expect(sent.message).toContain("pub fn start_session()");
    expect(sent.message).toContain("## Question");
    expect(sent.message).toContain("explain this");
    expect(screen.queryByTestId("ajax-web-session-context-chips")).not.toBeInTheDocument();
  });

  it("linkifies known symbols in assistant messages and attaches from detail sheet", async () => {
    vi.mocked(fetchTaskSymbols).mockResolvedValue([sampleSymbol]);
    render(<AjaxWebSessionView handle="web/fix-login" />);
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");
    socket.fire(
      "message",
      serverEvent({
        type: "session.status",
        version: WEB_SESSION_PROTOCOL_VERSION,
        state: "waiting",
      }),
    );

    await waitFor(() => {
      expect(screen.getByTestId("ajax-web-session-add-context")).not.toBeDisabled();
    });

    fireEvent.click(screen.getByTestId("ajax-web-session-add-context"));
    fireEvent.change(screen.getByTestId("symbol-search-input"), {
      target: { value: "start" },
    });
    await waitFor(() => {
      expect(screen.getByTestId(`symbol-search-row-${sampleSymbol.id}`)).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId(`symbol-search-row-${sampleSymbol.id}`));
    fireEvent.click(screen.getByTestId("symbol-search-confirm"));

    fireEvent.change(screen.getByTestId("ajax-web-session-input"), {
      target: { value: "seed symbol" },
    });
    fireEvent.click(screen.getByTestId("ajax-web-session-send"));

    socket.fire(
      "message",
      serverEvent({
        type: "session.assistant_delta",
        version: WEB_SESSION_PROTOCOL_VERSION,
        text: "See `start_session` for the entry point.",
      }),
    );
    socket.fire(
      "message",
      serverEvent({
        type: "session.settled",
        version: WEB_SESSION_PROTOCOL_VERSION,
      }),
    );

    await waitFor(() => {
      expect(
        screen.getByTestId(`ajax-web-session-symbol-ref-${sampleSymbol.id}`),
      ).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId(`ajax-web-session-symbol-ref-${sampleSymbol.id}`));
    expect(screen.getByTestId("symbol-detail-sheet")).toBeInTheDocument();
    expect(screen.getByTestId("symbol-detail-source")).toHaveTextContent(
      "pub fn start_session()",
    );

    fireEvent.click(screen.getByTestId("symbol-detail-attach"));
    expect(screen.getByTestId("ajax-web-session-context-chips")).toBeInTheDocument();
    expect(screen.getByText("start_session()")).toBeInTheDocument();
  });

  it("removes a context chip before send", async () => {
    vi.mocked(fetchTaskSymbols).mockResolvedValue([sampleSymbol]);
    render(<AjaxWebSessionView handle="web/fix-login" />);
    const socket = latestSocket();
    socket.readyState = MockWebSocket.OPEN;
    socket.fire("open");
    socket.fire(
      "message",
      serverEvent({
        type: "session.status",
        version: WEB_SESSION_PROTOCOL_VERSION,
        state: "waiting",
      }),
    );

    await waitFor(() => {
      expect(screen.getByTestId("ajax-web-session-add-context")).not.toBeDisabled();
    });

    fireEvent.click(screen.getByTestId("ajax-web-session-add-context"));
    fireEvent.change(screen.getByTestId("symbol-search-input"), {
      target: { value: "start" },
    });
    await waitFor(() => {
      expect(screen.getByTestId(`symbol-search-row-${sampleSymbol.id}`)).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId(`symbol-search-row-${sampleSymbol.id}`));
    fireEvent.click(screen.getByTestId("symbol-search-confirm"));

    fireEvent.click(screen.getByTestId(`ajax-web-session-context-chip-${sampleSymbol.id}`));
    expect(screen.queryByTestId("ajax-web-session-context-chips")).not.toBeInTheDocument();

    fireEvent.change(screen.getByTestId("ajax-web-session-input"), {
      target: { value: "plain question" },
    });
    fireEvent.click(screen.getByTestId("ajax-web-session-send"));
    expect(JSON.parse(socket.sent[0]!).message).toBe("plain question");
  });
});
