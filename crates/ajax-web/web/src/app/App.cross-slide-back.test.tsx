import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, act, within } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { taskDiffHash, taskHash } from "@/shared/lib/routes";

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

function stubFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    }),
  );
}

describe("App cross-slide back transition", () => {
  beforeEach(() => {
    window.location.hash = "";
    sessionStorage.clear();
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
    stubFetch();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    sessionStorage.clear();
    localStorage.clear();
  });

  it("plays only the page cross-slide when Back is pressed in the diff header (#1601)", async () => {
    render(<App />);
    setHash(taskHash("web/fix-login"));
    await screen.findByTestId("outlet-task");
    setHash(taskDiffHash("web/fix-login"));
    await screen.findByTestId("diff-review");

    vi.useFakeTimers();
    fireEvent.click(within(screen.getByTestId("diff-review-header")).getByRole("button", { name: "← Back" }));
    await act(() => vi.advanceTimersByTimeAsync(1000));

    const outlet = screen.getByTestId("outlet-task");
    expect(outlet).not.toHaveClass("ajax-swipe-enter-left");
    expect(outlet).not.toHaveClass("ajax-swipe-enter-right");
  });
});
