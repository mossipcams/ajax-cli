import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import Conversation from "./Conversation";
import type { ConversationItem } from "../session/public";

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

describe("Conversation — assistant response reveal", () => {
  // Word-by-word reveal reflows the column under a reader. A live answer shows
  // the paragraphs it has finished; the unfinished one waits.
  it("reveals only completed paragraphs while the turn runs", () => {
    const items = [
      userProse("u1", "Explain it"),
      agentProse("a1", "The port is read once at startup.\n\nIt is then han"),
    ];
    render(<Conversation items={items} busy />);

    const message = screen.getByTestId("session-message-agent");
    expect(message).toHaveTextContent("The port is read once at startup.");
    expect(message).not.toHaveTextContent("It is then han");
  });

  it("shows nothing for an answer with no completed paragraph yet", () => {
    const items = [userProse("u1", "Explain it"), agentProse("a1", "Still **strea")];
    render(<Conversation items={items} busy />);

    expect(screen.queryByTestId("session-message-agent")).not.toBeInTheDocument();
  });

  it("reveals the whole response, markdown and all, once the turn ends", () => {
    const items = [userProse("u1", "Explain it"), agentProse("a1", "Done:\n\n- item")];
    render(<Conversation items={items} busy={false} />);

    const message = screen.getByTestId("session-message-agent");
    expect(message).not.toHaveAttribute("data-live");
    expect(screen.getByRole("listitem")).toHaveTextContent("item");
  });

  // A break inside a fence is content, not a paragraph boundary; cutting there
  // would render half a code block as prose.
  it("never cuts a live answer inside a fenced block", () => {
    const items = [
      userProse("u1", "Show me"),
      agentProse("a1", "Here:\n\n```sh\ncargo test\n\nnpm run web:test"),
    ];
    render(<Conversation items={items} busy />);

    const message = screen.getByTestId("session-message-agent");
    expect(message).toHaveTextContent("Here:");
    expect(message).not.toHaveTextContent("cargo test");
  });

  // #1043: a one-paragraph "Let me look at the handler." has no paragraph break to
  // wait for, so gating it on one hid the agent's own words for the whole turn
  // — including the sentence explaining a permission ask sitting on screen.
  it("reveals a completed message once the turn moves past it", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Fix login"),
      agentProse("a1", "Let me look at the handler."),
      {
        kind: "tool",
        id: "x1",
        call: {
          callId: "c1",
          title: "Read",
          kind: "read",
          status: "in_progress",
          locations: [],
          content: [],
        },
      },
    ];
    render(<Conversation items={items} busy />);

    expect(screen.getByTestId("session-message-agent")).toHaveTextContent(
      "Let me look at the handler.",
    );
  });

  it("still holds back the answer still being written", () => {
    const items = [userProse("u1", "Explain it"), agentProse("a1", "Half a sen")];
    render(<Conversation items={items} busy />);

    expect(screen.queryByTestId("session-message-agent")).not.toBeInTheDocument();
  });

  it("settles earlier turns while the newest one is still live", () => {
    const items = [
      userProse("u1", "First"),
      agentProse("a1", "First answer, whole and unbroken."),
      userProse("u2", "Second"),
      agentProse("a2", "Partial second"),
    ];
    render(<Conversation items={items} busy />);

    const messages = screen.getAllByTestId("session-message-agent");
    expect(messages).toHaveLength(1);
    expect(messages[0]).toHaveTextContent("First answer, whole and unbroken.");
  });
});

describe("Conversation — what stays in the conversation", () => {
  it("keeps an unanswered permission ask in the column, without the buttons", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Clean up"),
      {
        kind: "permission",
        id: "q1",
        requestId: "r1",
        title: "Run `rm -rf target/debug`",
        resolved: false,
      },
    ];
    render(<Conversation items={items} busy={false} />);

    const marker = screen.getByTestId("session-permission-marker");
    expect(marker).toHaveAttribute("data-resolved", "false");
    // #970 A: the row uppercased label and payload alike, so `rm -rf` reached
    // the operator as `RM -RF` at the moment they were asked to approve it.
    expect(screen.getByText("Permission requested")).toHaveClass("session-note-label");
    expect(screen.getByText("Run rm -rf target/debug")).toHaveClass("session-note-text");
    expect(stylesSource).toMatch(/\.session-note-label\s*\{[^}]*text-transform:\s*uppercase/);
    expect(stylesSource.match(/\.session-note-text\s*\{([^}]*)\}/)?.[1] ?? "").not.toMatch(
      /text-transform/,
    );
    // The Approve/Reject pair lives in the sticky head; a second copy here
    // would be a control that scrolls away mid-decision.
    expect(screen.queryByRole("button", { name: /approve/i })).not.toBeInTheDocument();
  });

  it("files an answered permission into the timeline as history", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Clean up"),
      { kind: "permission", id: "q1", requestId: "r1", title: "Run tests?", resolved: true },
    ];
    render(<Conversation items={items} busy={false} />);

    expect(screen.queryByTestId("session-permission-marker")).not.toBeInTheDocument();
  });

  it("keeps an error in the column and puts a host note on a divider", () => {
    const items: ConversationItem[] = [
      userProse("u1", "Switch harness"),
      { kind: "note", id: "n1", tone: "error", text: "The agent stopped." },
      { kind: "note", id: "n2", tone: "info", text: "Client switched harness. Context reset." },
    ];
    render(<Conversation items={items} busy={false} />);

    expect(screen.getByTestId("session-note-error")).toHaveTextContent("The agent stopped.");
    expect(screen.getByTestId("session-note-info")).toHaveClass("session-divider");
  });
});
