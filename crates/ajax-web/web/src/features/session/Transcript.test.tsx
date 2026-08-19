import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import Transcript from "./Transcript";
import { thoughtSnippet, type ConversationItem, type ToolCall } from "./sessionThread";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readOrderedStylesSource(join(here, "../.."));

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

  it("auto-expands the live thought tail while a turn is in flight", () => {
    const items: ConversationItem[] = [{ kind: "thought", id: "e1", text: "Checking the router" }];
    render(<Transcript items={items} busy />);

    expect(screen.getByTestId("session-thinking-body")).toHaveTextContent("Checking the router");
  });

  it("collapses a thought when a later item arrives during the turn", () => {
    const items: ConversationItem[] = [
      { kind: "thought", id: "e1", text: "Checking the router" },
      { kind: "tool", id: "e2", call: call({ status: "in_progress" }) },
    ];
    render(<Transcript items={items} busy />);

    expect(screen.queryByTestId("session-thinking-body")).not.toBeInTheDocument();
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

  it("keeps the work chapter collapsed while the agent answer streams after tools settle", () => {
    const items: ConversationItem[] = [
      { kind: "prose", id: "u1", role: "user", text: "Fix it" },
      { kind: "thought", id: "e1", text: "Checking the router" },
      { kind: "tool", id: "e2", call: call({ callId: "a", status: "completed" }) },
      agentProse("a1", "Still **streaming**"),
    ];
    render(<Transcript items={items} busy />);

    expect(screen.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "false");
    expect(screen.queryByTestId("session-tool-card")).not.toBeInTheDocument();
    expect(screen.getByTestId("session-message-agent")).toHaveAttribute("data-live", "true");
  });

  it("collapses a settled run of work into one row, and opens it on tap", () => {
    const items: ConversationItem[] = [
      { kind: "prose", id: "u1", role: "user", text: "Fix it" },
      { kind: "thought", id: "e1", text: "Checking the router" },
      { kind: "tool", id: "e2", call: call({ callId: "a", startedAt: 1_000, endedAt: 3_000 }) },
      { kind: "tool", id: "e3", call: call({ callId: "b", startedAt: 3_000, endedAt: 131_000 }) },
      agentProse("a1", "Done."),
    ];
    render(<Transcript items={items} busy={false} />);

    const summary = screen.getByTestId("session-turn-work-summary");
    expect(summary).toHaveTextContent("Edited 2 files");
    expect(summary).toHaveTextContent("2m 10s");
    expect(screen.queryByTestId("session-tool-card")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-thinking")).not.toBeInTheDocument();

    fireEvent.click(summary);
    expect(screen.getAllByTestId("session-tool-card")).toHaveLength(2);
    expect(screen.getByTestId("session-thinking")).toBeInTheDocument();
  });

  it("leaves a run open when it is still running or something in it failed", () => {
    const items: ConversationItem[] = [
      { kind: "prose", id: "u1", role: "user", text: "Try again" },
      { kind: "tool", id: "e1", call: call({ callId: "a" }) },
      { kind: "tool", id: "e2", call: call({ callId: "b", status: "failed" }) },
    ];
    render(<Transcript items={items} busy={false} />);

    expect(screen.getByTestId("session-turn-work-summary")).toHaveTextContent("1 failed");
    expect(screen.getAllByTestId("session-tool-card")).toHaveLength(2);
    expect(screen.getByTestId("session-turn-work")).toHaveClass("has-failure");
  });

  // #970 A: the row uppercased label and payload alike, so `rm -rf` reached the
  // operator as `RM -RF` at the moment they were asked to approve it.
  it("keeps a permission title in its own case, apart from the chrome label", () => {
    const items: ConversationItem[] = [
      {
        kind: "permission",
        id: "e1",
        requestId: "r1",
        title: "Run `rm -rf target/debug`",
        resolved: false,
      },
    ];
    render(<Transcript items={items} busy={false} />);

    // Label and title are separate runs: the uppercase tracked cadence is
    // chrome, and case-folding the title would turn a command the operator has
    // to trust into `RM -RF TARGET/DEBUG`.
    expect(screen.getByText("Permission requested")).toHaveClass("session-note-label");
    expect(screen.getByText("Run rm -rf target/debug")).toHaveClass("session-note-text");
    expect(stylesSource).toMatch(
      /\.session-note-label\s*\{[^}]*text-transform:\s*uppercase/,
    );
    expect(stylesSource.match(/\.session-note-text\s*\{([^}]*)\}/)?.[1] ?? "").not.toMatch(
      /text-transform/,
    );
  });

  // #970 B: the row held its last 14 characters aside — right for two paths that
  // differ only at the end, wrong for prose, which came out as
  // "The port is read in config.… eed to move t…" across two spans.
  it("renders a collapsed reasoning line as one unbroken run of prose", () => {
    const text =
      "The port is read in config.ts and again in the listener bootstrap, so both need to move together or the dev server binds twice.";
    render(<Transcript items={[{ kind: "thought", id: "e1", text }]} busy={false} />);

    // getByText does not match across element boundaries, so this passes only
    // while the whole snippet lives in a single run.
    expect(screen.getByText(thoughtSnippet(text, 90))).toBeInTheDocument();
  });

  it("shows a single call as its own row rather than a summary of one", () => {
    const items: ConversationItem[] = [{ kind: "tool", id: "e1", call: call() }];
    render(<Transcript items={items} busy={false} />);

    expect(screen.queryByTestId("session-activity-summary")).not.toBeInTheDocument();
    expect(screen.getByTestId("session-tool-card")).toHaveTextContent("…/src/config.ts");
  });

  it("keeps prose out of the collapse: a reply follows the work chapter", () => {
    const items: ConversationItem[] = [
      { kind: "prose", id: "u1", role: "user", text: "Change the port" },
      { kind: "tool", id: "e1", call: call({ callId: "a" }) },
      agentProse("e2", "Changed the port."),
      { kind: "tool", id: "e3", call: call({ callId: "b" }) },
    ];
    render(<Transcript items={items} busy={false} />);

    expect(screen.getByTestId("session-message-user")).toHaveTextContent("Change the port");
    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("Changed the port.");
    expect(screen.getByTestId("session-turn-work")).toBeInTheDocument();
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
