import { describe, it, expect, vi, afterEach } from "vitest";
import { join } from "node:path";
import { render, fireEvent, screen, act } from "@testing-library/react";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import ConnectionStatus, { ERROR_HOLD_MS } from "./ConnectionStatus";
import connectionStatusSource from "./ConnectionStatus.tsx?raw";

function loadStylesSource(): string {
  const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
  return readOrderedStylesSource(join(testDir, "../.."));
}

describe("ConnectionStatus", () => {
  it("shows the connection state label", () => {
    render(<ConnectionStatus state="connected" />);
    expect(screen.getByText("connected")).toBeInTheDocument();
    expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "connected");
  });

  it("appends a detail to the label when provided", () => {
    render(
      <ConnectionStatus state="backend unreachable" detail="HTTP 503" />,
    );
    expect(screen.getByText("backend unreachable: HTTP 503")).toBeInTheDocument();
  });

  it("offers a single sign-in action when the edge sign-in expired", () => {
    const onSignIn = vi.fn();
    render(<ConnectionStatus state="sign-in expired" onSignIn={onSignIn} />);
    fireEvent.click(screen.getByRole("button", { name: "Sign-in expired. Tap to sign in." }));
    expect(onSignIn).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
  });

  it("marks Retry as the sole primary connection action", () => {
    render(<ConnectionStatus state="disconnected" />);
    const retry = screen.getByRole("button", { name: "Retry" });
    expect(retry).toHaveTextContent("Retry");
    expect(retry).toHaveClass("is-primary");
    expect(screen.getByRole("button", { name: "Reload" })).not.toHaveClass("is-primary");
    expect(screen.getByRole("button", { name: "Copy Diagnostics" })).not.toHaveClass("is-primary");
    expect(connectionStatusSource).toMatch(/Retry[\s\S]*className="is-primary"/);
    expect(connectionStatusSource).toMatch(/Reload/);
    expect(connectionStatusSource).toMatch(/Copy Diagnostics/);
    expect(connectionStatusSource).toMatch(/Open Health URL/);
  });

  it("fires recovery callbacks", async () => {
    const onRetry = vi.fn();
    const onReload = vi.fn();
    const onCopyDiagnostics = vi.fn();
    render(
      <ConnectionStatus
        state="disconnected"
        onRetry={onRetry}
        onReload={onReload}
        onCopyDiagnostics={onCopyDiagnostics}
      />,
    );
    fireEvent.click(screen.getByText("Retry"));
    fireEvent.click(screen.getByText("Reload"));
    fireEvent.click(screen.getByText("Copy Diagnostics"));
    expect(onRetry).toHaveBeenCalledOnce();
    expect(onReload).toHaveBeenCalledOnce();
    expect(onCopyDiagnostics).toHaveBeenCalledOnce();
  });

  it("accepts a later Reload click after a failed recoverable reload callback", async () => {
    const onReload = vi.fn();
    render(<ConnectionStatus state="disconnected" onReload={onReload} />);
    const reload = screen.getByText("Reload");

    fireEvent.click(reload);
    expect(onReload).toHaveBeenCalledOnce();

    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
    fireEvent.click(reload);
    expect(onReload).toHaveBeenCalledTimes(2);
  });

  it("accepts a later Retry click after a failed attempt with the same state", async () => {
    const onRetry = vi.fn();
    const { rerender } = render(
      <ConnectionStatus state="backend unreachable" onRetry={onRetry} />,
    );

    fireEvent.click(screen.getByText("Retry"));
    expect(onRetry).toHaveBeenCalledOnce();

    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
    rerender(<ConnectionStatus state="backend unreachable" onRetry={onRetry} />);
    fireEvent.click(screen.getByText("Retry"));
    expect(onRetry).toHaveBeenCalledTimes(2);
  });

  it("latches Retry and Reload under same-turn multi-click", () => {
    const onRetry = vi.fn();
    const onReload = vi.fn();
    render(
      <ConnectionStatus state="disconnected" onRetry={onRetry} onReload={onReload} />,
    );
    const retry = screen.getByText("Retry");
    const reload = screen.getByText("Reload");
    retry.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    retry.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    reload.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    reload.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(onRetry).toHaveBeenCalledOnce();
    expect(onReload).toHaveBeenCalledOnce();
  });

  it("links to the health URL", () => {
    render(<ConnectionStatus state="checking" />);
    expect(screen.getByRole("link", { name: "Open Health URL" })).toHaveAttribute(
      "href",
      "/api/health",
    );
  });

  it("hides quiet connection states in CSS while error states stay visible", () => {
    const stylesSource = loadStylesSource();
    const quietStates = ["checking", "connected", "reconnecting"] as const;
    for (const state of quietStates) {
      expect(stylesSource).toMatch(
        new RegExp(`\\.connection-status\\[data-state="${state}"\\][\\s\\S]*?display:\\s*none`),
      );
    }
    const errorStates = ["backend unreachable", "disconnected", "stale session"] as const;
    for (const state of errorStates) {
      const rule =
        stylesSource.match(
          new RegExp(`\\.connection-status\\[data-state="${state}"\\]\\s*\\{([^}]*)\\}`),
        )?.[1] ?? "";
      expect(rule).not.toMatch(/display:\s*none/);
    }
  });

describe("ConnectionStatus #1612 error hold", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("#1612 holds a background error as reconnecting until the hold elapses", () => {
    vi.useFakeTimers();
    render(<ConnectionStatus state="backend unreachable" holdErrors />);
    const el = screen.getByTestId("connection-status");
    expect(el).toHaveAttribute("data-state", "reconnecting");
    act(() => {
      vi.advanceTimersByTime(ERROR_HOLD_MS - 1);
    });
    expect(el).toHaveAttribute("data-state", "reconnecting");
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(el).toHaveAttribute("data-state", "backend unreachable");
  });

  it("#1612 recovers before the hold ends without ever showing the error", () => {
    vi.useFakeTimers();
    const { rerender } = render(<ConnectionStatus state="backend unreachable" holdErrors />);
    expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "reconnecting");
    rerender(<ConnectionStatus state="connected" holdErrors />);
    act(() => {
      vi.advanceTimersByTime(ERROR_HOLD_MS + 100);
    });
    expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "connected");
  });

  it("#1612 shows sign-in expired immediately even when holding errors", () => {
    vi.useFakeTimers();
    render(<ConnectionStatus state="sign-in expired" holdErrors />);
    expect(screen.getByTestId("connection-status")).toHaveAttribute(
      "data-state",
      "sign-in expired",
    );
    expect(
      screen.getByRole("button", { name: "Sign-in expired. Tap to sign in." }),
    ).toBeInTheDocument();
  });

  it("#1612 shows the error immediately when not holding errors", () => {
    vi.useFakeTimers();
    render(<ConnectionStatus state="backend unreachable" />);
    expect(screen.getByTestId("connection-status")).toHaveAttribute(
      "data-state",
      "backend unreachable",
    );
  });
  it("#1612 hides a held error on the first render", () => {
    vi.useFakeTimers();
    render(<ConnectionStatus state="backend unreachable" holdErrors />);
    expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "reconnecting");
    expect(document.body.textContent).not.toContain("backend unreachable");
  });

  it("#1612 starts a fresh hold when the error recurs after release and recovery", () => {
    vi.useFakeTimers();
    const { rerender } = render(<ConnectionStatus state="backend unreachable" holdErrors />);
    act(() => {
      vi.advanceTimersByTime(ERROR_HOLD_MS);
    });
    expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "backend unreachable");
    rerender(<ConnectionStatus state="connected" holdErrors />);
    rerender(<ConnectionStatus state="backend unreachable" holdErrors />);
    expect(screen.getByTestId("connection-status")).toHaveAttribute("data-state", "reconnecting");
  });
});
});
