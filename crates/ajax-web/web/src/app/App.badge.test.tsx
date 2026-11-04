import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

function stubCockpit(cards: unknown[]) {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse({ ...cockpit, cards }));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    }),
  );
}

const needsYouCard = cockpit.cards.find((card) => card.attention === "needs-you");
const idleCard = { ...cockpit.cards[0], attention: "idle" };
const reviewCard = { ...cockpit.cards[0], attention: "review" };

describe("App Home Screen badge", () => {
  let setAppBadge: ReturnType<typeof vi.fn>;
  let clearAppBadge: ReturnType<typeof vi.fn>;

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
    setAppBadge = vi.fn(() => Promise.resolve());
    clearAppBadge = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "setAppBadge", {
      configurable: true,
      value: setAppBadge,
    });
    Object.defineProperty(navigator, "clearAppBadge", {
      configurable: true,
      value: clearAppBadge,
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    delete (navigator as Partial<Navigator>).setAppBadge;
    delete (navigator as Partial<Navigator>).clearAppBadge;
  });

  it("sets the badge to the needs-you count, counting review cards too", async () => {
    stubCockpit([needsYouCard, reviewCard, idleCard]);

    render(<App />);

    expect(await screen.findByText("2 need you")).toBeInTheDocument();
    await vi.waitFor(() => expect(setAppBadge).toHaveBeenCalledWith(2));
    expect(clearAppBadge).not.toHaveBeenCalled();
  });

  it("clears the badge when nothing needs attention", async () => {
    stubCockpit([idleCard]);

    render(<App />);

    await vi.waitFor(() => expect(clearAppBadge).toHaveBeenCalledTimes(1));
    expect(setAppBadge).not.toHaveBeenCalled();
  });

  it("does nothing when the badge API is absent", async () => {
    delete (navigator as Partial<Navigator>).setAppBadge;
    delete (navigator as Partial<Navigator>).clearAppBadge;
    stubCockpit([needsYouCard]);

    render(<App />);

    expect(await screen.findByText("1 needs you")).toBeInTheDocument();
    expect(setAppBadge).not.toHaveBeenCalled();
    expect(clearAppBadge).not.toHaveBeenCalled();
  });

  it("swallows a synchronous throw from setAppBadge", async () => {
    setAppBadge.mockImplementation(() => {
      throw new Error("sync failure");
    });
    stubCockpit([needsYouCard]);

    render(<App />);

    expect(await screen.findByText("1 needs you")).toBeInTheDocument();
    await vi.waitFor(() => expect(setAppBadge).toHaveBeenCalledWith(1));
  });

  it("swallows a rejected clearAppBadge promise", async () => {
    clearAppBadge.mockImplementation(() => Promise.reject(new Error("rejected")));
    stubCockpit([idleCard]);

    render(<App />);

    await vi.waitFor(() => expect(clearAppBadge).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("1 task")).toBeInTheDocument();
  });
});
