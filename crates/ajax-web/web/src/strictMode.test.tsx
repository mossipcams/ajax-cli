import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { StrictMode } from "react";
import { render, waitFor } from "@testing-library/react";
import mainSource from "@/app/main.tsx?raw";
import App from "@/app/App";
import cockpit from "@/fixtures/cockpit.json";

// StrictMode double-invokes effects in development. The Playwright suite runs
// against the Vite dev server, so socket cardinality is covered there
// (`e2e/terminal-behavior.test.ts`). These cases cover what Playwright cannot:
// that the shell itself stays single-flight, and that the wrapper is not
// quietly dropped from the entry point to make something else pass.

function jsonResponse(body: unknown) {
  return { ok: true, status: 200, text: () => Promise.resolve(JSON.stringify(body)) };
}

describe("StrictMode lifecycle safety", () => {
  beforeEach(() => {
    window.location.hash = "";
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
      class {
        observe = vi.fn();
        disconnect = vi.fn();
      },
    );
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("mounts the app inside StrictMode at the entry point", () => {
    expect(mainSource).toMatch(/<StrictMode>/);
    expect(mainSource).toMatch(/from "react"/);
  });

  it("does not double-fetch the cockpit on a StrictMode double mount", async () => {
    let cockpitCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") {
          cockpitCalls += 1;
          return Promise.resolve(jsonResponse(cockpit));
        }
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v1" }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );

    await waitFor(() => expect(cockpitCalls).toBeGreaterThan(0));
    // Let any second effect invocation settle before asserting.
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(cockpitCalls).toBe(1);
  });

  it("leaves no shell listeners behind when a StrictMode mount unmounts", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v1" }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );
    const addSpy = vi.spyOn(window, "addEventListener");
    const removeSpy = vi.spyOn(window, "removeEventListener");

    const { unmount } = render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    await waitFor(() => expect(addSpy).toHaveBeenCalled());
    unmount();

    const added = addSpy.mock.calls.filter(([type]) => type === "focus").length;
    const removed = removeSpy.mock.calls.filter(([type]) => type === "focus").length;
    expect(removed).toBe(added);
  });
});
