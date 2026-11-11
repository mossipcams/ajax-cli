import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { join } from "node:path";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { writeOrchestrationChatEnabled } from "@/features/settings/public";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import { sessionHash, taskDiffHash } from "@/shared/lib/routes";

const sessionCockpit = {
  ...cockpit,
  cards: cockpit.cards.map((card) =>
    card.qualified_handle === "web/fix-login" ? { ...card, session_capable: true } : card,
  ),
};
const sessionDetail = { ...taskDetail, session_capable: true };

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

function stubFetch(sessionCapable: boolean) {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        return Promise.resolve(jsonResponse(sessionCapable ? sessionCockpit : cockpit));
      }
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path === "/api/session/models") {
        return Promise.resolve(jsonResponse({ models: [{ id: "auto", label: "Auto" }] }));
      }
      if (path.startsWith("/api/tasks/")) {
        return Promise.resolve(jsonResponse(sessionCapable ? sessionDetail : taskDetail));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    }),
  );
}

function cockpitChromeMounted() {
  return (
    screen.queryByRole("heading", { level: 1, name: "Ajax" }) !== null ||
    screen.queryByRole("navigation", { name: "Mobile navigation" }) !== null
  );
}

function watchCockpitChrome() {
  const watch = { everMounted: cockpitChromeMounted() };
  const observer = new MutationObserver(() => {
    if (cockpitChromeMounted()) watch.everMounted = true;
  });
  observer.observe(document.body, { childList: true, subtree: true });
  return { watch, stop: () => observer.disconnect() };
}

describe("App chat <-> diff chrome (#1604)", () => {
  beforeEach(() => {
    window.location.hash = "";
    sessionStorage.clear();
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
    stubFetch(true);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    sessionStorage.clear();
    localStorage.clear();
  });

  it("hides cockpit chrome and bottom nav on the session-capable Diff with Ajax chat on (#1604)", async () => {
    render(<App />);
    setHash(taskDiffHash("web/fix-login"));
    await screen.findByTestId("diff-review");

    await waitFor(() => expect(screen.getByTestId("app-shell")).toHaveClass("app-shell--session"));
    expect(cockpitChromeMounted()).toBe(false);
    expect(screen.queryByRole("navigation", { name: "Mobile navigation" })).not.toBeInTheDocument();
  });

  it("never mounts cockpit chrome or bottom nav through a Chat <-> Diff cross-slide (#1604)", async () => {
    render(<App />);
    setHash(sessionHash("web/fix-login"));
    await screen.findByTestId("session-chat");
    await waitFor(() => expect(screen.getByTestId("app-shell")).toHaveClass("app-shell--session"));

    const { watch, stop } = watchCockpitChrome();
    try {
      fireEvent.click(screen.getByTestId("session-details"));
      fireEvent.click(within(screen.getByTestId("task-details-sheet")).getByRole("button", { name: "Show diff" }));
      await screen.findByTestId("diff-review");
      await waitFor(() => expect(screen.queryByTestId("session-chat")).not.toBeInTheDocument());
      expect(screen.getByTestId("app-shell")).toHaveClass("app-shell--session");

      fireEvent.click(within(screen.getByTestId("diff-review-header")).getByRole("button", { name: "← Back" }));
      await screen.findByTestId("session-chat");
      await waitFor(() => expect(screen.queryByTestId("outlet-diff")).not.toBeInTheDocument());
      expect(screen.getByTestId("app-shell")).toHaveClass("app-shell--session");

      expect(watch.everMounted).toBe(false);
      expect(cockpitChromeMounted()).toBe(false);
    } finally {
      stop();
    }
  });

  it("keeps cockpit chrome on the Diff of a task that is not session-capable (#1604)", async () => {
    stubFetch(false);
    render(<App />);
    setHash(taskDiffHash("web/fix-login"));
    await screen.findByTestId("diff-review");

    expect(screen.getByRole("heading", { level: 1, name: "Ajax" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Mobile navigation" })).toBeInTheDocument();
    expect(screen.getByTestId("app-shell")).not.toHaveClass("app-shell--session");
  });

  it("keeps cockpit chrome on the Diff when Ajax chat is off (#1604)", async () => {
    writeOrchestrationChatEnabled(false);
    render(<App />);
    setHash(taskDiffHash("web/fix-login"));
    await screen.findByTestId("diff-review");

    expect(screen.getByRole("heading", { level: 1, name: "Ajax" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Mobile navigation" })).toBeInTheDocument();
    expect(screen.getByTestId("app-shell")).not.toHaveClass("app-shell--session");
  });

  it("never mounts cockpit chrome on a cold Diff while the task detail is pending (#1606)", async () => {
    let releaseDetail!: () => void;
    const detailReleased = new Promise<void>((resolve) => {
      releaseDetail = resolve;
    });
    const fetchWithCockpit = globalThis.fetch;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        if (String(input).startsWith("/api/tasks/")) {
          return detailReleased.then(() => jsonResponse(sessionDetail));
        }
        return fetchWithCockpit(input);
      }),
    );
    window.location.hash = taskDiffHash("web/fix-login");

    const { watch, stop } = watchCockpitChrome();
    try {
      render(<App />);
      expect(cockpitChromeMounted()).toBe(false);

      await screen.findByTestId("task-skeleton");
      expect(cockpitChromeMounted()).toBe(false);
      expect(screen.getByTestId("app-shell")).toHaveClass("app-shell--session");

      releaseDetail();
      await screen.findByTestId("diff-review");
      expect(cockpitChromeMounted()).toBe(false);
      expect(watch.everMounted).toBe(false);
    } finally {
      stop();
    }
  });

  it("holds the chat workspace safe-area padding without depending on the mounted session outlet (#1604)", () => {
    const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
    const css = readOrderedStylesSource(join(testDir, ".."));
    const block = css.match(/(?:^|\n)(\.app-shell--session \.route-scroll)\s*\{([^}]*)\}/);

    expect(block).not.toBeNull();
    const [, selector, declarations] = block!;
    expect(selector).not.toContain(":has(");
    expect(declarations).toMatch(
      /padding:\s*env\(safe-area-inset-top\)\s+env\(safe-area-inset-right\)\s+0\s+env\(safe-area-inset-left\)/,
    );
    expect(declarations).toMatch(/padding-bottom:\s*0/);
  });

  it("insets the chat-workspace Diff body and clears the home indicator (#1604)", () => {
    const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
    const css = readOrderedStylesSource(join(testDir, ".."));

    expect(css).toMatch(
      /\.app-shell--session \.diff-review\s*\{\s*padding-bottom:\s*calc\(20px \+ env\(safe-area-inset-bottom\)\);?\s*\}/,
    );
    expect(css).toMatch(
      /\.app-shell--session \.diff-review > :not\(\.detail-header\)\s*\{\s*margin-inline:\s*20px;?\s*\}/,
    );
  });
});
