import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import SystemPanel from "./SystemPanel";
import type { BrowserBackend } from "@/shared/lib/types";

const backend: BrowserBackend = { authority: "host-native", control_enabled: true };

describe("SystemPanel", () => {
  it("reports the transport link state as the server-independent truth", () => {
    render(<SystemPanel backend={backend} connection="backend unreachable" taskCount={4} />);
    const link = screen.getByTestId("system-link");
    expect(link).toHaveTextContent("Backend unreachable");
    expect(link).toHaveClass("tone-error");
  });

  it("tones a healthy link as success", () => {
    render(<SystemPanel backend={backend} connection="connected" taskCount={0} />);
    expect(screen.getByTestId("system-link")).toHaveClass("tone-success");
  });

  it("says when the backend will not accept control", () => {
    render(
      <SystemPanel
        backend={{ ...backend, control_enabled: false }}
        connection="connected"
        taskCount={0}
      />,
    );
    const control = screen.getByTestId("system-control");
    expect(control).toHaveTextContent("Read-only");
    expect(control).toHaveClass("tone-waiting");
  });

  it("surfaces a backend warning verbatim and hides the slot without one", () => {
    render(
      <SystemPanel
        backend={{ ...backend, warning: "control disabled by config" }}
        connection="connected"
        taskCount={0}
      />,
    );
    expect(screen.getByTestId("system-warning")).toHaveTextContent("control disabled by config");

    render(<SystemPanel backend={backend} connection="connected" taskCount={0} />);
    expect(screen.getAllByTestId("system-warning")).toHaveLength(1);
  });

  it("reports the authority and fleet size, and opens diagnostics", () => {
    const onOpenSettings = vi.fn();
    render(
      <SystemPanel
        backend={backend}
        connection="connected"
        taskCount={7}
        onOpenSettings={onOpenSettings}
      />,
    );
    expect(screen.getByText("host-native")).toBeInTheDocument();
    expect(screen.getByText("7")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Diagnostics" }));
    expect(onOpenSettings).toHaveBeenCalled();
  });
});
