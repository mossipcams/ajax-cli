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

  it("keeps an optional enum's blank option enabled so the choice can be cleared", () => {
    render(
      <ElicitationPanel
        decision={{
          requestId: "e4",
          message: "Pick an environment",
          schema: {
            type: "object",
            properties: {
              env: { type: "string", title: "Environment", enum: ["staging", "prod"] },
            },
          },
          fields: [],
        }}
        connected
        onAccept={vi.fn()}
        onDecline={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    const blank = screen.getByRole("option", { name: "Select…" }) as HTMLOptionElement;
    expect(blank.disabled).toBe(false);
    expect(screen.getByLabelText("Environment")).toHaveValue("");
  });

  it("shows a required single-option enum as unselected until the operator picks it", () => {
    const onAccept = vi.fn();
    render(
      <ElicitationPanel
        decision={{
          requestId: "e3",
          message: "Confirm environment",
          schema: {
            type: "object",
            properties: {
              env: { type: "string", title: "Environment", enum: ["staging"] },
            },
            required: ["env"],
          },
          fields: [],
        }}
        connected
        onAccept={onAccept}
        onDecline={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    const select = screen.getByLabelText("Environment");
    expect(select).toHaveValue("");
    expect(screen.getByRole("button", { name: "Accept" })).toBeDisabled();

    fireEvent.change(select, { target: { value: "staging" } });
    expect(screen.getByRole("button", { name: "Accept" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    expect(onAccept).toHaveBeenCalledWith({ env: "staging" });
  });

  it("shows each field's question text under its title and wires it as the description", () => {
    render(
      <ElicitationPanel
        decision={{
          requestId: "e5",
          message: "Claude has questions for you",
          schema: {
            type: "object",
            properties: {
              question_0: {
                type: "string",
                title: "Eval set",
                description: "Which evaluation set should the benchmark run against?",
                oneOf: [
                  { const: "Held-out", title: "Held-out" },
                  { const: "Regression", title: "Regression" },
                ],
              },
              question_1: {
                type: "string",
                title: "Metrics",
                description: "Which metrics matter? (comma-separated: Accuracy, Latency)",
              },
              same: { type: "string", title: "Same", description: "Same" },
            },
            required: [],
          },
          fields: [],
        }}
        connected
        onAccept={vi.fn()}
        onDecline={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("Which evaluation set should the benchmark run against?")).toBeInTheDocument();
    expect(screen.getByLabelText("Eval set")).toHaveAccessibleDescription(
      "Which evaluation set should the benchmark run against?",
    );
    expect(screen.getByLabelText("Metrics")).toHaveAccessibleDescription(
      "Which metrics matter? (comma-separated: Accuracy, Latency)",
    );
    expect(screen.getAllByText("Same")).toHaveLength(1);
    expect(screen.getByLabelText("Same")).not.toHaveAttribute("aria-describedby");
  });

  it("shows the selected enum option's description under the select", () => {
    render(
      <ElicitationPanel
        decision={{
          requestId: "e6",
          message: "Claude has questions for you",
          schema: {
            type: "object",
            properties: {
              host: {
                type: "string",
                title: "Hardware",
                description: "Which GPU host should run the job?",
                oneOf: [
                  { const: "local", title: "sayso-llm", description: "Local host, idle right now." },
                  { const: "cloud", title: "Cloud A100", description: "Faster, but costs money." },
                ],
              },
            },
            required: [],
          },
          fields: [],
        }}
        connected
        onAccept={vi.fn()}
        onDecline={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    const select = screen.getByLabelText("Hardware");
    expect(screen.queryByText("Local host, idle right now.")).not.toBeInTheDocument();
    expect(screen.queryByText("Faster, but costs money.")).not.toBeInTheDocument();

    fireEvent.change(select, { target: { value: "cloud" } });
    expect(screen.getByText("Faster, but costs money.")).toBeInTheDocument();
    expect(screen.queryByText("Local host, idle right now.")).not.toBeInTheDocument();
    expect(select).toHaveAccessibleDescription("Which GPU host should run the job? Faster, but costs money.");
  });

  it("ships elicitation styles through the chat stylesheet graph", () => {
    expect(stylesSource).toContain(".session-elicitation");
  });
});
