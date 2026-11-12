import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { join } from "node:path";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

function touch(type: string, clientY: number): Event {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, "touches", { value: [{ clientY }] });
  return event;
}

describe("App dashboard pull-to-refresh", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  function cockpitLoads() {
    return fetchMock.mock.calls.filter(([input]) => String(input) === "/api/cockpit").length;
  }

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
    fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    sessionStorage.clear();
  });

  it("keeps tracking every touchmove and refreshes on release (#1603)", async () => {
    render(<App />);
    await screen.findByText("Fix login");
    const outlet = screen.getByTestId("outlet-dashboard");
    const indicator = screen.getByTestId("pull-indicator");
    const loadsBeforePull = cockpitLoads();

    act(() => {
      outlet.dispatchEvent(touch("touchstart", 100));
    });

    const heights: string[] = [];
    const armed: boolean[] = [];
    for (const clientY of [130, 160, 190, 220, 250, 280, 310, 340]) {
      act(() => {
        outlet.dispatchEvent(touch("touchmove", clientY));
      });
      heights.push(indicator.style.height);
      armed.push(indicator.classList.contains("armed"));
    }

    expect(heights).toEqual(["15px", "30px", "45px", "60px", "75px", "90px", "96px", "96px"]);
    expect(armed).toEqual([false, false, false, false, true, true, true, true]);

    act(() => {
      outlet.dispatchEvent(new Event("touchend"));
    });

    await waitFor(() => expect(cockpitLoads()).toBe(loadsBeforePull + 1));
  });

  it("animates the pull indicator closed on release instead of snapping (#1609)", async () => {
    render(<App />);
    await screen.findByText("Fix login");
    const outlet = screen.getByTestId("outlet-dashboard");
    const indicator = screen.getByTestId("pull-indicator");

    act(() => {
      outlet.dispatchEvent(touch("touchstart", 100));
    });
    for (const clientY of [130, 160, 190]) {
      act(() => {
        outlet.dispatchEvent(touch("touchmove", clientY));
      });
    }

    expect(indicator.style.height).toBe("45px");
    expect(indicator.classList.contains("is-pulling")).toBe(true);

    act(() => {
      outlet.dispatchEvent(new Event("touchend"));
    });

    expect(indicator.style.height).toBe("0px");
    expect(indicator.classList.contains("is-pulling")).toBe(false);

    const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
    const css = readOrderedStylesSource(join(testDir, ".."));
    const base = css.match(/\.pull-indicator\s*\{([^}]*)\}/)?.[1] ?? "";
    const pulling = css.match(/\.pull-indicator\.is-pulling\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(base).toMatch(/transition:\s*height var\(--dur-ui\) var\(--ease-spring\);/);
    expect(pulling).toMatch(/transition:\s*none;/);
  });
});
