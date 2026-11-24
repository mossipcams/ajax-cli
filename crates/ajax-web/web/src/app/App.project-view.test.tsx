import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import type { BrowserCockpitView } from "@/shared/lib/types";

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

const twoRepoCockpit: BrowserCockpitView = {
  ...cockpit,
  repos: { repos: [{ name: "web" }, { name: "api" }] },
  cards: [
    { id: "web/a", qualified_handle: "web/a", repo: "web", title: "Web A", status: "waiting", attention: "needs-you", last_activity_unix_secs: 0, actions: [] },
    { id: "api/b", qualified_handle: "api/b", repo: "api", title: "API B", status: "waiting", attention: "needs-you", last_activity_unix_secs: 0, actions: [] },
    { id: "api/c", qualified_handle: "api/c", repo: "api", title: "API C", status: "running", attention: "active", last_activity_unix_secs: 0, actions: [] },
  ],
};

describe("App project view", () => {
  beforeEach(() => {
    window.location.hash = "";
    sessionStorage.clear();
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
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(twoRepoCockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    sessionStorage.clear();
  });

  it("counts only the selected project's tasks in the header", async () => {
    window.location.hash = "#/p/web";
    render(<App />);
    expect(await screen.findByText("1 needs you")).toBeInTheDocument();
    expect(screen.queryByText("2 need you · 1 running")).not.toBeInTheDocument();
  });

  it("shows the project name as the header title on a project route", async () => {
    window.location.hash = "#/p/web";
    render(<App />);
    expect(await screen.findByText("1 needs you")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/^web$/);
  });

  it("keeps the global counts and Ajax title on the dashboard", async () => {
    window.location.hash = "#/";
    render(<App />);
    expect(await screen.findByText("2 need you · 1 running")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/^Ajax$/);
  });
});
