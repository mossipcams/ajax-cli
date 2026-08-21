import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import { Conversation } from "../conversation/public";
import { thoughtSnippet, type ConversationItem, type ToolCall } from "../session/public";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readOrderedStylesSource(join(here, "../../.."));

beforeEach(() => {
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const userProse = (id: string, text: string): ConversationItem => ({
  kind: "prose",
  id,
  role: "user",
  text,
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

const tool = (id: string, overrides: Partial<ToolCall> = {}): ConversationItem => ({
  kind: "tool",
  id,
  call: call({ callId: id, ...overrides }),
});

describe("ActivityDisclosure", () => {
  it("keeps thoughts, tools and plans out of the transcript behind one row", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Fix it"),
      { kind: "thought", id: "t1", text: "Checking the router" },
      tool("e1", { kind: "read", status: "completed" }),
      { kind: "plan", id: "p1", entries: [{ content: "Patch", status: "in_progress" }] },
      agentProse("a1", "Fixed."),
    ];
    render(<Conversation items={items} busy={false} />);

    expect(screen.getAllByTestId("session-turn-work-summary")).toHaveLength(1);
    expect(screen.queryByTestId("session-tool-card")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-thinking")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-plan")).not.toBeInTheDocument();
    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("Fixed.");
  });

  it("shows only the current operation while the turn runs", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Fix it"),
      tool("e1", { kind: "read", status: "completed", locations: ["/repo/src/config.ts"] }),
      tool("e2", {
        kind: "execute",
        title: "cargo test",
        status: "in_progress",
        locations: [],
      }),
    ];
    render(<Conversation items={items} busy />);

    const summary = screen.getByTestId("session-turn-work-summary");
    expect(summary).toHaveTextContent("Running cargo test…");
    expect(summary).not.toHaveTextContent("config.ts");
    expect(screen.queryByTestId("session-tool-card")).not.toBeInTheDocument();
  });

  it("falls back to the live plan step, then reasoning, with no call in flight", () => {
    const planned: ConversationItem[] = [
      userProse("u1", "Fix it"),
      { kind: "plan", id: "p1", entries: [{ content: "Patch the listener", status: "in_progress" }] },
    ];
    const { unmount } = render(<Conversation items={planned} busy />);
    expect(screen.getByTestId("session-turn-work-summary")).toHaveTextContent(
      "Patch the listener…",
    );
    unmount();

    render(
      <Conversation
        items={[userProse("u1", "Fix it"), { kind: "thought", id: "t1", text: "Checking auth" }]}
        busy
      />,
    );
    expect(screen.getByTestId("session-turn-work-summary")).toHaveTextContent("Checking auth");
  });

  it("collapses to a counted summary when the turn finishes", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Fix it"),
      tool("r1", { kind: "read", startedAt: 1_000, endedAt: 2_000 }),
      tool("r2", { kind: "read" }),
      tool("x1", { kind: "edit" }),
      tool("x2", { kind: "edit" }),
      tool("s1", { kind: "execute", startedAt: 3_000, endedAt: 39_000 }),
      agentProse("a1", "Done."),
    ];
    render(<Conversation items={items} busy={false} />);

    expect(screen.getByTestId("session-turn-work-summary")).toHaveTextContent(
      "Read 2 files · edited 2 files · ran 1 command · 38s",
    );
  });

  it("opens the full timeline on tap and keeps that choice as the turn grows", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Fix it"),
      { kind: "thought", id: "t1", text: "Checking the router" },
      tool("e1", { content: [{ type: "text", text: "ok" }] }),
    ];
    const view = render(<Conversation items={items} busy={false} />);

    fireEvent.click(screen.getByTestId("session-turn-work-summary"));
    expect(screen.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "true");
    expect(screen.getByTestId("session-tool-card")).toBeInTheDocument();
    expect(screen.getByTestId("session-thinking")).toBeInTheDocument();

    view.rerender(<Conversation items={[...items, tool("e2")]} busy={false} />);
    expect(screen.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "true");

    fireEvent.click(screen.getByTestId("session-turn-work-summary"));
    expect(screen.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "false");
  });

  it("keeps a manual collapse shut even when a later call fails", () => {
    const items: ConversationItem[] = [userProse("u1", "Fix it"), tool("e1")];
    const view = render(<Conversation items={items} busy={false} />);

    fireEvent.click(screen.getByTestId("session-turn-work-summary"));
    fireEvent.click(screen.getByTestId("session-turn-work-summary"));
    view.rerender(
      <Conversation items={[...items, tool("e2", { status: "failed" })]} busy={false} />,
    );

    expect(screen.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "false");
  });

  it("opens itself on a failure, with the failing output already visible", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Try again"),
      tool("e1", { status: "failed", content: [{ type: "text", text: "exit 1" }] }),
    ];
    render(<Conversation items={items} busy={false} />);

    expect(screen.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "true");
    expect(screen.getByTestId("session-turn-work")).toHaveClass("has-failure");
    expect(screen.getByTestId("session-turn-work-summary")).toHaveTextContent("1 failed");
    expect(screen.getByTestId("session-tool-output")).toHaveTextContent("exit 1");
  });

  it("opens itself while the turn waits on an approval", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Ship it"),
      tool("e1"),
      { kind: "permission", id: "q1", requestId: "7", title: "Run tests?", resolved: false },
    ];
    render(<Conversation items={items} busy />);

    expect(screen.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "true");
  });

  it("reveals a diff and command output once the timeline is open", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Change the port"),
      {
        kind: "tool",
        id: "e1",
        call: call({
          content: [
            {
              type: "diff",
              path: "/repo/src/config.ts",
              oldText: "port = 1\n",
              newText: "port = 2\n",
            },
          ],
        }),
      },
    ];
    render(<Conversation items={items} busy={false} />);

    expect(screen.queryByTestId("session-tool-diff")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-turn-work-summary"));
    fireEvent.click(screen.getByRole("button", { name: /Edited config\.ts/i }));

    const diff = screen.getByTestId("session-tool-diff");
    expect(diff).toHaveTextContent("-port = 1");
    expect(diff).toHaveTextContent("+port = 2");
  });

  it("renders the plan as a checklist inside the timeline", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Plan it"),
      {
        kind: "plan",
        id: "p1",
        entries: [
          { content: "Read", status: "completed" },
          { content: "Patch", status: "in_progress" },
        ],
      },
    ];
    render(<Conversation items={items} busy={false} />);

    fireEvent.click(screen.getByTestId("session-turn-work-summary"));
    const steps = screen.getAllByRole("listitem");
    expect(steps).toHaveLength(2);
    expect(steps[1]).toHaveAttribute("data-status", "in_progress");
  });

  it("keeps reasoning collapsed inside the timeline until it is asked for", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Fix it"),
      { kind: "thought", id: "t1", text: "Checking the router" },
    ];
    render(<Conversation items={items} busy={false} />);

    fireEvent.click(screen.getByTestId("session-turn-work-summary"));
    expect(screen.queryByTestId("session-thinking-body")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /thinking/i }));
    expect(screen.getByTestId("session-thinking-body")).toHaveTextContent("Checking the router");
  });

  it("renders a collapsed reasoning line as one unbroken run of prose", () => {
    const text =
      "The port is read in config.ts and again in the listener bootstrap, so both need to move together or the dev server binds twice.";
    render(
      <Conversation items={[userProse("u1", "Why?"), { kind: "thought", id: "t1", text }]} busy={false} />,
    );

    fireEvent.click(screen.getByTestId("session-turn-work-summary"));
    expect(screen.getByText(thoughtSnippet(text, 90))).toBeInTheDocument();
  });

  it("sets reasoning as a grid row rather than an italic aside", () => {
    expect(stylesSource.match(/\.session-thinking\s*\{([^}]*)\}/)?.[1] ?? "").not.toMatch(
      /font-style/,
    );
  });

  it("omits the other-step filler from the collapsed summary", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Fix it"),
      tool("s1", { kind: "search", status: "completed" }),
      agentProse("a1", "Done."),
    ];
    render(<Conversation items={items} busy={false} />);

    const summary = screen.getByTestId("session-turn-work-summary").textContent ?? "";
    expect(summary).not.toMatch(/other step/i);
  });

  it("files an answered permission into the timeline as history", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Clean up"),
      { kind: "permission", id: "q1", requestId: "r1", title: "Run tests?", resolved: true },
    ];
    render(<Conversation items={items} busy={false} />);

    expect(screen.queryByTestId("session-permission-marker")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-turn-work-summary"));
    expect(screen.getByTestId("session-permission-marker")).toHaveAttribute(
      "data-resolved",
      "true",
    );
  });

  it("disclosure body blocks sit flush to the rail without a second card indent", () => {
    expect(stylesSource.match(/\.session-toolcard-body\s*\{([^}]*)\}/)?.[1] ?? "").not.toMatch(
      /padding:[^;]*24px/,
    );
  });
});
