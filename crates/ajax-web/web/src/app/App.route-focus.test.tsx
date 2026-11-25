import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserCockpitView } from "@/shared/lib/types";

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

const focusCockpit: BrowserCockpitView = {
  ...cockpit,
  repos: { repos: [{ name: "web" }] },
  cards: [
    { id: "web/a", qualified_handle: "web/a", repo: "web", title: "Web A", status: "waiting", attention: "needs-you", last_activity_unix_secs: 0, actions: [] },
    { id: "web/b", qualified_handle: "web/b", repo: "web", title: "Web B", status: "running", attention: "active", last_activity_unix_secs: 0, actions: [] },
  ],
};

function webARow() {
  return screen.getByRole("button", { name: /Web A/ });
}

describe("App route focus", () => {
  beforeEach(() => {
    window.location.hash = "";
    sessionStorage.clear();
    localStorage.clear();
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
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(focusCockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    sessionStorage.clear();
  });

  it("moves focus to the task outlet after opening a row with Enter, and back to the row on Back", async () => {
    window.location.hash = "#/";
    render(<App />);
    const row = await screen.findByRole("button", { name: /Web A/ });

    row.focus();
    fireEvent.keyDown(row, { key: "Enter" });

    const taskOutlet = await screen.findByTestId("outlet-task");
    await waitFor(() => expect(taskOutlet).toHaveFocus());

    fireEvent.click(screen.getAllByRole("button", { name: /back/i })[0]);

    await waitFor(() => expect(webARow()).toHaveFocus());
  });

  it("does not move focus on initial load", async () => {
    window.location.hash = "#/";
    const focus = vi.spyOn(HTMLElement.prototype, "focus");
    render(<App />);
    await screen.findByRole("button", { name: /Web A/ });

    expect(focus).not.toHaveBeenCalledWith({ preventScroll: true });
    expect(document.body).toHaveFocus();
    focus.mockRestore();
  });
});
