import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { TASK_TERMINAL_PREFERENCE_STORAGE_KEY } from "@/features/task-workspace/public";
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

describe("App back navigation transition", () => {
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
    vi.unstubAllGlobals();
    sessionStorage.clear();
  });

  it("slides the dashboard in from the right when going back from a task", async () => {
    render(<App />);
    setHash(taskHash("web/fix-login"));
    await screen.findByTestId("outlet-task");

    fireEvent.click(within(screen.getByTestId("mobile-chrome-header")).getByRole("button", { name: "← Back" }));

    const outlet = await screen.findByTestId("outlet-dashboard");
    await waitFor(() => expect(outlet).toHaveClass("ajax-swipe-enter-right"));
    expect(outlet).not.toHaveClass("ajax-swipe-enter-left");
  });

  it("slides the dashboard in from the right when going back from the terminal view", async () => {
    localStorage.setItem(TASK_TERMINAL_PREFERENCE_STORAGE_KEY, JSON.stringify(["web/fix-login"]));
    render(<App />);
    setHash(taskHash("web/fix-login"));
    await screen.findByTestId("outlet-task");

    const back = within(screen.getByTestId("task-detail")).getByRole("button", { name: "← Back" });
    fireEvent.click(back);

    const outlet = await screen.findByTestId("outlet-dashboard");
    expect(outlet).toHaveClass("ajax-swipe-enter-right");
    expect(outlet).not.toHaveClass("ajax-swipe-enter-left");
  });

  it("slides the task in from the left when opening it from the dashboard", async () => {
    render(<App />);
    fireEvent.click(await screen.findByText("Fix login"));

    const outlet = await screen.findByTestId("outlet-task");
    expect(outlet).toHaveClass("ajax-swipe-enter-left");
    expect(outlet).not.toHaveClass("ajax-swipe-enter-right");
  });
});
