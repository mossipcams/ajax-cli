import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen, act } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

describe("App status and results", () => {
  beforeEach(() => {
    window.location.hash = "";
    document.title = "";
    Object.defineProperty(document, "hidden", { configurable: true, value: false });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    vi.stubGlobal("WebSocket", class {
      readyState = 1;
      close() {}
      addEventListener() {}
      send() {}
    });
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

  it("reports a single needs-you card as '1 needs you'", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);

    expect(await screen.findByText("1 needs you")).toBeInTheDocument();
  });

  it("restarts the dismiss timer when the same result is shown again", async () => {
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path === "/api/operations") return Promise.reject(new Error("offline"));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
    const operationCalls = () =>
      fetchMock.mock.calls.filter(([path]) => String(path) === "/api/operations").length;

    render(<App />);
    const ship = () => screen.getByRole("button", { name: "Ship" });
    await screen.findByRole("button", { name: "Ship" });

    vi.useFakeTimers();
    fireEvent.click(ship());
    await vi.waitFor(() => expect(screen.getByTestId("result-panel")).toBeInTheDocument());

    await act(() => vi.advanceTimersByTimeAsync(10000));
    fireEvent.click(ship());
    await vi.waitFor(() => expect(operationCalls()).toBe(2));
    await act(() => vi.advanceTimersByTimeAsync(0));

    await act(() => vi.advanceTimersByTimeAsync(3000));
    expect(screen.getByTestId("result-panel")).toBeInTheDocument();
  });
});
