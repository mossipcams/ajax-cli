import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { SWIPE_PAGE_COMMIT_MS } from "@/shared/hooks/useSwipePageTransition";
import { taskHash } from "@/shared/lib/routes";

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

describe("App route entrance (#1600)", () => {
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

  it("keeps the directional entrance after its slide ends so route-fade-in does not restart (#1600)", async () => {
    render(<App />);
    fireEvent.click(await screen.findByText("Fix login"));

    const outlet = await screen.findByTestId("outlet-task");
    await waitFor(() => expect(outlet).toHaveClass("ajax-swipe-enter-left"));
    fireEvent.animationEnd(outlet);

    expect(outlet).toHaveClass("ajax-swipe-enter-left");
  });

  it("holds the none entrance on the diff outlet after the page cross-slide settles (#1600)", async () => {
    render(<App />);
    setHash(taskHash("web/fix-login"));
    const task = await screen.findByTestId("task-detail");
    Object.defineProperty(task, "clientWidth", { value: 390, configurable: true });

    vi.useFakeTimers();
    try {
      fireEvent.touchStart(task, { changedTouches: [{ clientX: 200, clientY: 80 }] });
      fireEvent.touchMove(task, { changedTouches: [{ clientX: 120, clientY: 82 }] });
      fireEvent.touchEnd(task, { changedTouches: [{ clientX: 120, clientY: 82 }] });
      await act(() => vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 1000));

      expect(screen.getByTestId("outlet-diff")).toHaveClass("ajax-swipe-enter-none");
      expect(screen.getByTestId("outlet-diff")).not.toHaveClass("ajax-swipe-enter-left");
      expect(screen.getByTestId("outlet-diff")).not.toHaveClass("ajax-swipe-enter-right");
    } finally {
      vi.useRealTimers();
    }
  });
});
