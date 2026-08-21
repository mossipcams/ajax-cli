import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import { ElicitationPanel } from "./public";

const stylesSource = readOrderedStylesSource(
  join(dirname(fileURLToPath(import.meta.url)), "../../.."),
);

describe("ElicitationPanel", () => {
  it("renders a schema-driven form and dispatches accept, decline, and cancel", () => {
    const onAccept = vi.fn();
    const onDecline = vi.fn();
    const onCancel = vi.fn();
    render(
      <ElicitationPanel
        decision={{
          requestId: "e1",
          message: "Pick deployment target",
          schema: {
            type: "object",
            properties: {
              target: { type: "string", title: "Target", enum: ["staging", "production"] },
              confirmed: { type: "boolean", title: "Confirmed" },
            },
            required: ["target"],
          },
          fields: [],
        }}
        connected
        onAccept={onAccept}
        onDecline={onDecline}
        onCancel={onCancel}
      />,
    );

    expect(screen.getByTestId("session-elicitation")).toHaveAttribute("aria-label", "Agent request");
    expect(screen.getByText("Pick deployment target")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Target"), { target: { value: "staging" } });
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    expect(onAccept).toHaveBeenCalledWith({ target: "staging", confirmed: false });
    fireEvent.click(screen.getByRole("button", { name: "Decline" }));
    expect(onDecline).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("blocks Accept until required fields have values", () => {
    const onAccept = vi.fn();
    render(
      <ElicitationPanel
        decision={{
          requestId: "e2",
          message: "Name required",
          schema: {
            type: "object",
            properties: {
              name: { type: "string", title: "Name" },
            },
            required: ["name"],
          },
          fields: [],
        }}
        connected
        onAccept={onAccept}
        onDecline={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    const accept = screen.getByRole("button", { name: "Accept" });
    expect(accept).toBeDisabled();
    fireEvent.click(accept);
    expect(onAccept).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Matt" } });
    expect(accept).toBeEnabled();
    fireEvent.click(accept);
    expect(onAccept).toHaveBeenCalledWith({ name: "Matt" });
  });

  it("ships elicitation styles through the chat stylesheet graph", () => {
    expect(stylesSource).toContain(".session-elicitation");
  });
});
