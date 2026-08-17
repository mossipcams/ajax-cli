import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import Transcript from "./Transcript";
import type { ConversationItem, ToolCall } from "./sessionThread";

beforeEach(() => {
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const agentProse = (id: string, text: string): ConversationItem => ({
  kind: "prose",
  id,
  role: "agent",
  text,
});

const call = (overrides: Partial<ToolCall> = {}): ToolCall => ({
  callId: "c1",
  title: "Edit config",
  kind: "edit",
  status: "completed",
  locations: ["/repo/src/config.ts"],
  content: [],
  ...overrides,
});

describe("Transcript", () => {
  it("renders the live agent tail as markdown while busy", () => {
    render(<Transcript items={[agentProse("e1", "Still **streaming**")]} busy />);
    const message = screen.getByTestId("session-message-agent");
    expect(message).toHaveAttribute("data-live", "true");
    expect(message).toHaveClass("is-live");
    expect(message).toHaveTextContent("Still streaming");
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
    expect(screen.getByText("streaming").tagName).toBe("STRONG");
  });

  it("renders settled agent prose as markdown after the turn ends", () => {
    render(<Transcript items={[agentProse("e1", "Done:\n\n- item")]} busy={false} />);
    const message = screen.getByTestId("session-message-agent");
    expect(message).not.toHaveAttribute("data-live");
    expect(screen.getByRole("listitem")).toHaveTextContent("item");
  });

  it("keeps earlier agent prose on markdown when a new tail streams", () => {
    const items = [agentProse("e1", "First:\n\n- done"), agentProse("e2", "Next **chunk**")];
    render(<Transcript items={items} busy />);
    const messages = screen.getAllByTestId("session-message-agent");
    expect(messages[0]).not.toHaveAttribute("data-live");
    expect(screen.getByRole("listitem")).toHaveTextContent("done");
    expect(messages[1]).toHaveAttribute("data-live", "true");
    expect(messages[1]).toHaveTextContent("Next chunk");
    expect(screen.getByText("chunk").tagName).toBe("STRONG");
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
  });

  it("keeps reasoning collapsed until it is asked for", () => {
    const items: ConversationItem[] = [{ kind: "thought", id: "e1", text: "Checking the router" }];
    render(<Transcript items={items} busy={false} />);

    expect(screen.queryByTestId("session-thinking-body")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /thinking/i }));
    expect(screen.getByTestId("session-thinking-body")).toHaveTextContent("Checking the router");
  });

  it("renders a tool call's diff as a diff, not as prose", () => {
    const items: ConversationItem[] = [
      {
        kind: "tool",
        id: "e1",
        call: call({
          content: [
            { type: "diff", path: "/repo/src/config.ts", oldText: "port = 1\n", newText: "port = 2\n" },
          ],
        }),
      },
    ];
    render(<Transcript items={items} busy={false} />);

    // Completed and quiet: the header states the outcome, the body is opt-in.
    expect(screen.queryByTestId("session-tool-diff")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Edit config/i }));

    const diff = screen.getByTestId("session-tool-diff");
    expect(diff).toHaveTextContent("-port = 1");
    expect(diff).toHaveTextContent("+port = 2");
  });

  it("opens a failed tool call by default", () => {
    const items: ConversationItem[] = [
      {
        kind: "tool",
        id: "e1",
        call: call({ status: "failed", content: [{ type: "text", text: "exit 1" }] }),
      },
    ];
    render(<Transcript items={items} busy={false} />);
    expect(screen.getByTestId("session-tool-output")).toHaveTextContent("exit 1");
  });

  it("renders the plan as a checklist with the current step marked", () => {
    const items: ConversationItem[] = [
      {
        kind: "plan",
        id: "e1",
        entries: [
          { content: "Read", status: "completed" },
          { content: "Patch", status: "in_progress" },
        ],
      },
    ];
    render(<Transcript items={items} busy={false} />);
    const steps = screen.getAllByRole("listitem");
    expect(steps).toHaveLength(2);
    expect(steps[1]).toHaveAttribute("data-status", "in_progress");
  });

  it("marks a permission ask in history without offering the buttons twice", () => {
    const items: ConversationItem[] = [
      { kind: "permission", id: "e1", requestId: "7", title: "Run tests?", resolved: true },
    ];
    render(<Transcript items={items} busy={false} />);
    const marker = screen.getByTestId("session-permission-marker");
    expect(marker).toHaveAttribute("data-resolved", "true");
    expect(marker).toHaveTextContent("Run tests?");
    // The Approve/Reject pair lives in the sticky head; a second copy here
    // would be a control that scrolls away mid-decision.
    expect(screen.queryByRole("button", { name: /approve/i })).not.toBeInTheDocument();
  });
});
