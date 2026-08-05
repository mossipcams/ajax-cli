import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import App from "./App";
import * as telemetry from "@/shared/lib/telemetry";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";

// Hard file-scope stub: late microtasks must never reach jsdom's real WebSocket.
class StubWebSocket {
  readyState = 1;
  close() {}
  addEventListener() {}
  send() {}
}
globalThis.WebSocket = StubWebSocket as unknown as typeof WebSocket;

function setHash(hash: string) {
  window.location.hash = hash;
  window.dispatchEvent(new HashChangeEvent("hashchange"));
}

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

describe("App drop shell confirm", () => {
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

  it("keeps shell confirm visible after navigating away from the task", async () => {
    const completeSpy = vi.spyOn(telemetry, "endTapToOperationComplete");
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    fireEvent.click(await screen.findByText("Drop"));
    expect(await screen.findByTestId("result-panel-confirm")).toBeInTheDocument();

    setHash("#/");
    expect(await screen.findByTestId("outlet-dashboard")).toBeInTheDocument();
    expect(screen.getByTestId("result-panel-confirm")).toBeInTheDocument();
    expect(completeSpy).not.toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ error_kind: "unmount" }),
    );

    fireEvent.click(screen.getByText("Cancel"));
    expect(completeSpy).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ ok: false, op: "drop", error_kind: "undo" }),
    );
    expect(screen.queryByTestId("result-panel-confirm")).not.toBeInTheDocument();
  });
});
