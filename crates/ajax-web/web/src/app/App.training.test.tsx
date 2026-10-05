import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";

// Hard file-scope stub: late microtasks must never reach jsdom's real WebSocket.
class StubWebSocket {
  readyState = 1;
  close() {}
  addEventListener() {}
  send() {}
}
globalThis.WebSocket = StubWebSocket as unknown as typeof WebSocket;

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

describe("App training modal route coupling", () => {
  beforeEach(() => {
    window.location.hash = "";
    document.title = "";
    Object.defineProperty(document, "hidden", { configurable: true, value: false });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    vi.stubGlobal(
      "WebSocket",
      class {
        readyState = 1;
        close() {}
        addEventListener() {}
        send() {}
      },
    );
    vi.stubGlobal(
      "ResizeObserver",
      class MockResizeObserver {
        observe = vi.fn();
        disconnect = vi.fn();
      },
    );
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("opens the training modal from the bottom-nav Train button", async () => {
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path === "/api/training/status") return Promise.resolve(jsonResponse({ state: "idle" }));
      if (path === "/api/training/models")
        return Promise.resolve(
          jsonResponse({ profiles: [], active_profile: null, running: false }),
        );
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);

    const trainingStatusCalls = () =>
      fetchMock.mock.calls.filter(([path]) => String(path) === "/api/training/status").length;

    // Closed modal: out of the DOM and no status polling.
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trainingStatusCalls()).toBe(0);

    fireEvent.click(screen.getByRole("button", { name: "Train" }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Training")).toBeInTheDocument();
    // Polling starts only after the modal opens.
    await vi.waitFor(() => expect(trainingStatusCalls()).toBeGreaterThan(0));

    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});
