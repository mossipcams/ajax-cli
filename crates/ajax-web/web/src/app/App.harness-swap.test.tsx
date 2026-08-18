import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen, waitFor } from "@testing-library/react";
import App from "./App";
import appSource from "./App.tsx?raw";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { writeOrchestrationChatEnabled } from "@/features/session/sessionMode";

class StubWebSocket {
  readyState = 1;
  close() {}
  addEventListener() {}
  send() {}
}
globalThis.WebSocket = StubWebSocket as unknown as typeof WebSocket;

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

describe("App harness swap", () => {
  beforeEach(() => {
    window.location.hash = "";
    document.title = "";
    localStorage.clear();
    writeOrchestrationChatEnabled(true);
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
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    window.location.hash = "";
    document.title = "";
    localStorage.clear();
  });

  it("wires harness swap on SessionChat with swap-only session outbox clearing", () => {
    const diffReviewBlock = appSource.match(/<DiffReview[\s\S]*?\/>/)?.[0] ?? "";
    const taskDetailBlock = appSource.match(/<TaskDetail[\s\S]*?\/>/)?.[0] ?? "";
    const sessionChatBlock = appSource.match(/<SessionChat[\s\S]*?\/>/)?.[0] ?? "";

    expect(diffReviewBlock).not.toMatch(/\bagent=/);
    expect(diffReviewBlock).not.toMatch(/onSwappedAgent=/);
    expect(taskDetailBlock).not.toMatch(/onSwappedAgent=/);
    expect(taskDetailBlock).not.toMatch(/HarnessSwap/);
    expect(sessionChatBlock).toMatch(
      /onSwappedAgent=\{\(\) => \{[\s\S]*?clearSessionOutbox\(route\.handle\)/,
    );
    expect(sessionChatBlock).toMatch(
      /onMutated=\{\(\) => route\.kind === "session" && route\.handle && reload\(\)\}/,
    );
  });

  it("clears the session outbox and reloads task detail after a harness swap in Ajax chat", async () => {
    let detailFetches = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/web%2Ffix-login")) {
          if (init?.method === "POST") return Promise.resolve(jsonResponse({ ok: true }));
          detailFetches += 1;
          return Promise.resolve(jsonResponse({ ...taskDetail, session_capable: true }));
        }
        if (path.startsWith("/api/session/models"))
          return Promise.resolve(jsonResponse({ models: [], default: "" }));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    sessionStorage.setItem(
      "ajax.web.session.outbox.web%2Ffix-login",
      JSON.stringify([{ text: "queued prompt", clientMessageId: "msg-1" }]),
    );

    render(<App />);
    setHash("#/session/web/fix-login");
    await screen.findByTestId("session-chat");
    fireEvent.click(screen.getByTestId("session-details"));
    await waitFor(() => expect(detailFetches).toBeGreaterThan(0));
    const beforeSwap = detailFetches;

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(screen.getByRole("radio", { name: "Cursor" }));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(detailFetches).toBe(beforeSwap + 1));
    expect(sessionStorage.getItem("ajax.web.session.outbox.web%2Ffix-login")).toBeNull();
  });
});
