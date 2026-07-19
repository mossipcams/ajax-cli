import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import ConnectionStatus from "./ConnectionStatus";
import connectionStatusSource from "./ConnectionStatus.tsx?raw";

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

  it("links to the health URL", () => {
    render(<ConnectionStatus state="checking" />);
    expect(screen.getByRole("link", { name: "Open Health URL" })).toHaveAttribute(
      "href",
      "/api/health",
    );
  });
});
