import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import PermissionPanel from "./PermissionPanel";

describe("PermissionPanel", () => {
  it("renders title and detail", () => {
    render(
      <PermissionPanel
        decision={{ requestId: "1", title: "Run cargo test?", detail: "Needs approval" }}
        connected
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />,
    );
    expect(screen.getByTestId("session-decision")).toHaveTextContent("Run cargo test?");
    expect(screen.getByTestId("session-decision")).toHaveTextContent("Needs approval");
  });

  it("disables controls while disconnected", () => {
    render(
      <PermissionPanel
        decision={{ requestId: "1", title: "Run?", detail: "" }}
        connected={false}
        onApprove={vi.fn()}
        onReject={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Approve" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reject" })).toBeDisabled();
  });

  it("calls approve and reject handlers", () => {
    const onApprove = vi.fn();
    const onReject = vi.fn();
    render(
      <PermissionPanel
        decision={{ requestId: "1", title: "Run?", detail: "" }}
        connected
        onApprove={onApprove}
        onReject={onReject}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    fireEvent.click(screen.getByRole("button", { name: "Reject" }));
    expect(onApprove).toHaveBeenCalledOnce();
    expect(onReject).toHaveBeenCalledOnce();
  });
});
