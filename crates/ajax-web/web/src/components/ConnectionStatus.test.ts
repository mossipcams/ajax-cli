import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ConnectionStatus from "./ConnectionStatus.svelte";
import connectionStatusSource from "./ConnectionStatus.svelte?raw";

describe("ConnectionStatus", () => {
  it("shows the connection state label", () => {
    const { getByText, container } = render(ConnectionStatus, {
      props: { state: "connected" },
    });
    expect(getByText("connected")).toBeInTheDocument();
    expect(container.querySelector(".connection-status")?.getAttribute("data-state")).toBe(
      "connected",
    );
  });

  it("appends a detail to the label when provided", () => {
    const { getByText } = render(ConnectionStatus, {
      props: { state: "backend unreachable", detail: "HTTP 503" },
    });
    expect(getByText("backend unreachable: HTTP 503")).toBeInTheDocument();
  });

  it("marks Retry as the sole primary connection action", () => {
    const { container } = render(ConnectionStatus, {
      props: { state: "disconnected" },
    });
    const retry = container.querySelector(".connection-actions button.is-primary");
    expect(retry).not.toBeNull();
    expect(retry).toHaveTextContent("Retry");
    const primaries = container.querySelectorAll(".connection-actions .is-primary");
    expect(primaries).toHaveLength(1);
    expect(connectionStatusSource).toMatch(/Retry[\s\S]*class="is-primary"/);
    expect(connectionStatusSource).toMatch(/Reload/);
    expect(connectionStatusSource).toMatch(/Copy Diagnostics/);
    expect(connectionStatusSource).toMatch(/Open Health URL/);
  });

  it("fires recovery callbacks", async () => {
    const onRetry = vi.fn();
    const onReload = vi.fn();
    const onCopyDiagnostics = vi.fn();
    const { getByText } = render(ConnectionStatus, {
      props: { state: "disconnected", onRetry, onReload, onCopyDiagnostics },
    });
    await fireEvent.click(getByText("Retry"));
    await fireEvent.click(getByText("Reload"));
    await fireEvent.click(getByText("Copy Diagnostics"));
    expect(onRetry).toHaveBeenCalledOnce();
    expect(onReload).toHaveBeenCalledOnce();
    expect(onCopyDiagnostics).toHaveBeenCalledOnce();
  });

  it("links to the health URL", () => {
    const { getByText } = render(ConnectionStatus, { props: { state: "checking" } });
    expect(getByText("Open Health URL").getAttribute("href")).toBe("/api/health");
  });
});
