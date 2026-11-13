import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, act, within, waitFor } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { projectHash, sessionHash, taskDiffHash, taskHash } from "@/shared/lib/routes";

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

function touch(
  type: string,
  clientX: number,
  clientY = 40,
  target?: EventTarget,
  timeStamp?: number,
): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "touches", { value: [{ clientX, clientY }] });
  Object.defineProperty(event, "changedTouches", { value: [{ clientX, clientY }] });
  if (target) Object.defineProperty(event, "target", { value: target });
  if (timeStamp !== undefined) Object.defineProperty(event, "timeStamp", { value: timeStamp });
  return event;
}

function setScrollTop(node: HTMLElement, value: number) {
  Object.defineProperty(node, "scrollTop", { configurable: true, writable: true, value });
}

describe("App page-stack below layer", () => {
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

  it("mounts the inert below layer on an edge touchstart and removes it on a no-move release", async () => {
    render(<App />);
    setHash(taskHash("web/fix-login"));
    const detail = await screen.findByTestId("task-detail");

    expect(screen.queryByTestId("page-stack-below")).toBeNull();
    await act(async () => {
      detail.dispatchEvent(touch("touchstart", 4, 40));
    });

    const below = screen.getByTestId("page-stack-below");
    expect(below.hasAttribute("inert")).toBe(true);
    expect(below).toHaveAttribute("aria-hidden", "true");
    expect(within(below).getByTestId(/^(outlet-dashboard|dashboard-skeleton)$/)).toBeTruthy();
    expect(within(screen.getByTestId("route-scroll")).getByTestId("task-detail")).toBe(detail);

    await act(async () => {
      detail.dispatchEvent(touch("touchend", 4, 40));
    });
    expect(screen.queryByTestId("page-stack-below")).toBeNull();
  });

  it("renders the project list as the below copy when a task was opened from a project", async () => {
    render(<App />);
    setHash(projectHash("api"));
    await screen.findByTestId("outlet-project");
    setHash(taskHash("web/fix-login"));
    const detail = await screen.findByTestId("task-detail");
    await act(async () => {
      detail.dispatchEvent(touch("touchstart", 4, 40));
    });

    const below = within(screen.getByTestId("page-stack-below"));
    expect(below.getByTestId("outlet-project")).toBeTruthy();
    expect(below.queryByTestId("outlet-dashboard")).toBeNull();
  });

  it("renders the below layer at the saved destination offset", async () => {
    render(<App />);
    const scroll = screen.getByTestId("route-scroll");
    setScrollTop(scroll, 300);
    act(() => {
      scroll.dispatchEvent(new Event("scroll"));
    });

    setHash(taskHash("web/fix-login"));
    const detail = await screen.findByTestId("task-detail");
    await act(async () => {
      detail.dispatchEvent(touch("touchstart", 4, 40));
    });

    // eslint-disable-next-line testing-library/no-node-access -- the wrapper has no test id; its transform is the contract
    const belowScroll = screen.getByTestId("page-stack-below").querySelector(
      ".page-stack-below-scroll",
    ) as HTMLElement;
    expect(belowScroll).toHaveStyle({ transform: "translate3d(0, -300px, 0)" });
  });

  it("never pulls the list from a drag on the inert below copy", async () => {
    render(<App />);
    setHash(taskHash("web/fix-login"));
    const detail = await screen.findByTestId("task-detail");
    await act(async () => {
      detail.dispatchEvent(touch("touchstart", 4, 40));
    });

    const copy = within(screen.getByTestId("page-stack-below")).getByTestId("outlet-dashboard");
    await act(async () => {
      copy.dispatchEvent(touch("touchstart", 200, 60));
      copy.dispatchEvent(touch("touchmove", 200, 140));
      copy.dispatchEvent(touch("touchend", 200, 140));
    });

    const indicators = screen.getAllByTestId("pull-indicator");
    expect(indicators.length).toBeGreaterThan(0);
    for (const indicator of indicators) expect(indicator).not.toHaveClass("is-pulling");
  });

  it("keeps the leaving workspace in route-scroll until the pop cross-slide settles", async () => {
    render(<App />);
    setHash(taskHash("web/fix-login"));
    const detail = await screen.findByTestId("task-detail");

    vi.useFakeTimers();
    const halfWidth = Math.floor(window.innerWidth / 2);
    await act(async () => {
      detail.dispatchEvent(touch("touchstart", 4, 40, undefined, 1000));
      detail.dispatchEvent(touch("touchmove", 30, 42, undefined, 1150));
      detail.dispatchEvent(touch("touchmove", halfWidth + 60, 42, undefined, 1250));
      detail.dispatchEvent(touch("touchend", halfWidth + 60, 42, undefined, 1300));
    });

    expect(within(screen.getByTestId("route-scroll")).getByTestId("task-detail")).toBe(detail);
    expect(screen.getByTestId("page-stack-below")).toBeTruthy();

    await act(() => vi.advanceTimersByTimeAsync(3000));

    expect(within(screen.getByTestId("route-scroll")).getByTestId("outlet-dashboard")).toBeTruthy();
    expect(screen.queryByTestId("task-detail")).toBeNull();
    expect(screen.queryByTestId("page-stack-below")).toBeNull();
  });

  it("never mounts the below layer from an edge touchstart on the diff page", async () => {
    render(<App />);
    setHash(taskHash("web/fix-login"));
    await screen.findByTestId("task-detail");
    setHash(taskDiffHash("web/fix-login"));
    const diff = await screen.findByTestId("diff-review");

    act(() => {
      diff.dispatchEvent(touch("touchstart", 4, 40));
      diff.dispatchEvent(touch("touchend", 4, 40));
    });
    expect(screen.queryByTestId("page-stack-below")).toBeNull();
  });

  function gestureTarget() {
    return screen.getByTestId("session-chat");
  }

  it("carries the dashboard chrome and nav in the below copy on a chat workspace", async () => {
    render(<App />);
    setHash(sessionHash("web/fix-login"));
    await waitFor(() => expect(gestureTarget()).toBeTruthy());
    await act(async () => {
      gestureTarget().dispatchEvent(touch("touchstart", 4, 40));
    });

    const below = screen.getByTestId("page-stack-below");
    // eslint-disable-next-line testing-library/no-node-access -- asserting the copy carries both elements
    expect(below.querySelectorAll(".cockpit-chrome").length).toBe(1);
    // eslint-disable-next-line testing-library/no-node-access -- asserting the copy carries both elements
    expect(below.querySelectorAll(".bottom-nav").length).toBe(1);
    // eslint-disable-next-line testing-library/no-node-access -- every chrome element must be inside the inert copy
    for (const chromeEl of Array.from(document.querySelectorAll(".cockpit-chrome"))) {
      expect(below.contains(chromeEl)).toBe(true);
    }
  });

  it("carries neither chrome nor nav in the below copy on a terminal task page", async () => {
    render(<App />);
    setHash(taskHash("web/fix-login"));
    const detail = await screen.findByTestId("task-detail");
    await act(async () => {
      detail.dispatchEvent(touch("touchstart", 4, 40));
    });

    const below = screen.getByTestId("page-stack-below");
    // eslint-disable-next-line testing-library/no-node-access -- asserting the copy carries neither element
    expect(below.querySelectorAll(".cockpit-chrome").length).toBe(0);
    // eslint-disable-next-line testing-library/no-node-access -- asserting the copy carries neither element
    expect(below.querySelectorAll(".bottom-nav").length).toBe(0);
  });

  it("keeps chrome in the below copy through a committed back swipe from a chat workspace", async () => {
    render(<App />);
    setHash(sessionHash("web/fix-login"));
    await waitFor(() => expect(gestureTarget()).toBeTruthy());

    vi.useFakeTimers();
    const halfWidth = Math.floor(window.innerWidth / 2);
    await act(async () => {
      gestureTarget().dispatchEvent(touch("touchstart", 4, 40, undefined, 1000));
      gestureTarget().dispatchEvent(touch("touchmove", 30, 42, undefined, 1150));
      gestureTarget().dispatchEvent(touch("touchmove", halfWidth + 60, 42, undefined, 1250));
      gestureTarget().dispatchEvent(touch("touchend", halfWidth + 60, 42, undefined, 1300));
    });

    const below = screen.getByTestId("page-stack-below");
    expect(below).toBeTruthy();
    // eslint-disable-next-line testing-library/no-node-access -- every chrome element must be inside the inert copy
    const chromeDuringSlide = Array.from(document.querySelectorAll(".cockpit-chrome"));
    expect(chromeDuringSlide.length).toBeGreaterThan(0);
    for (const chromeEl of chromeDuringSlide) {
      expect(below.contains(chromeEl)).toBe(true);
    }

    await act(() => vi.advanceTimersByTimeAsync(3000));

    expect(screen.queryByTestId("page-stack-below")).toBeNull();
    // eslint-disable-next-line testing-library/no-node-access -- the settled chrome belongs to the main shell
    const mainChrome = Array.from(document.querySelectorAll(".cockpit-chrome"));
    expect(mainChrome.length).toBeGreaterThan(0);
  });
});
