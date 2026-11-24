import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

describe("App dashboard cockpit load failure", () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let cockpitMode: "pending" | "fail" | "throw" | "ok";

  function cockpitLoads() {
    return fetchMock.mock.calls.filter(([input]) => String(input) === "/api/cockpit").length;
  }

  function headerRetry() {
    return within(screen.getByTestId("connection-status")).getByRole("button", { name: "Retry" });
  }

  beforeEach(() => {
    window.location.hash = "";
    sessionStorage.clear();
    cockpitMode = "fail";
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
    fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        if (cockpitMode === "pending") return new Promise(() => {});
        if (cockpitMode === "throw") return Promise.reject(new TypeError("Failed to fetch"));
        if (cockpitMode === "fail") return Promise.resolve(jsonResponse({ error: "boom" }, 500));
        return Promise.resolve(jsonResponse(cockpit));
      }
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    sessionStorage.clear();
  });

  it("replaces skeletons with the error message when the first load returns 500", async () => {
    render(<App />);
    const dashboard = screen.getByTestId("outlet-dashboard");
    await within(dashboard).findByText("Could not load tasks — HTTP 500");

    expect(screen.queryByTestId("dashboard-skeleton")).not.toBeInTheDocument();
    expect(screen.queryByText("— loading")).not.toBeInTheDocument();
    expect(screen.getByText("— unavailable")).toBeInTheDocument();
  });

  it("replaces skeletons with the error message when the first load throws", async () => {
    cockpitMode = "throw";
    render(<App />);
    const dashboard = screen.getByTestId("outlet-dashboard");
    await within(dashboard).findByText("Could not load tasks — Failed to fetch");

    expect(screen.queryByTestId("dashboard-skeleton")).not.toBeInTheDocument();
    expect(screen.queryByText("— loading")).not.toBeInTheDocument();
    expect(screen.getByText("— unavailable")).toBeInTheDocument();
  });

  it("leaves the header ConnectionStatus Retry as the only Retry button on the page", async () => {
    render(<App />);
    await within(screen.getByTestId("outlet-dashboard")).findByText(/Could not load tasks/);

    expect(screen.getAllByRole("button", { name: "Retry" })).toHaveLength(1);
    expect(headerRetry()).toBeInTheDocument();
  });

  it("re-runs the cockpit load from the header Retry and renders the task list when it succeeds", async () => {
    render(<App />);
    const dashboard = screen.getByTestId("outlet-dashboard");
    await within(dashboard).findByText(/Could not load tasks/);
    const loadsBeforeRetry = cockpitLoads();
    cockpitMode = "ok";

    fireEvent.click(headerRetry());

    await waitFor(() => expect(cockpitLoads()).toBeGreaterThan(loadsBeforeRetry));
    expect(await within(dashboard).findByText("Fix login")).toBeInTheDocument();
    expect(within(dashboard).queryByText(/Could not load tasks/)).not.toBeInTheDocument();
  });

  it("keeps skeletons while the first cockpit load is still in flight", () => {
    cockpitMode = "pending";
    render(<App />);

    const dashboard = screen.getByTestId("outlet-dashboard");
    expect(screen.getByTestId("dashboard-skeleton")).toBeInTheDocument();
    expect(within(dashboard).queryByText(/Could not load tasks/)).not.toBeInTheDocument();
    expect(screen.queryByText("— unavailable")).not.toBeInTheDocument();
  });
});
