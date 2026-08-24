import { describe, it, expect } from "vitest";
import { groupConversationTurns } from "./groupTurns";
import { opensByDefault, turnDigest } from "./turnDigest";
import type { ConversationItem } from "../session/public";

const user = (id: string, text: string): ConversationItem => ({
  kind: "prose",
  id,
  role: "user",
  text,
});

const tool = (
  id: string,
  kind: string,
  status: string,
  content: ConversationItem extends { kind: "tool"; call: infer C }
    ? C extends { content: infer T }
      ? T
      : never
    : never = [] as never,
): ConversationItem => ({
  kind: "tool",
  id,
  call: { callId: id, title: kind, kind, status, locations: [], content },
});

const digestOf = (items: ConversationItem[]) => turnDigest(groupConversationTurns(items)[0]);

describe("turnDigest", () => {
  it("titles the turn with its own prompt and summarises what came of it", () => {
    const digest = digestOf([
      user("u1", "run the full suite"),
      tool("t1", "execute", "completed"),
    ]);

    expect(digest.ask).toBe("run the full suite");
    expect(digest.outcome).toBe("Ran 1 command");
    expect(digest.failed).toBe(false);
  });

  it("reports a failure so the collapsed row can carry it", () => {
    const digest = digestOf([user("u1", "run it"), tool("t1", "execute", "failed")]);

    expect(digest.failed).toBe(true);
    expect(digest.outcome).toContain("1 failed");
  });

  it("counts what the turn changed on disk, once per file", () => {
    const diff = [
      {
        type: "diff" as const,
        path: "crates/ajax-core/src/lifecycle.rs",
        oldText: "a\nb\n",
        newText: "a\nc\nd\n",
      },
    ];
    const digest = digestOf([
      user("u1", "fix it"),
      tool("t1", "edit", "completed", diff as never),
      tool("t2", "edit", "completed", diff as never),
    ]);

    expect(digest.changed).toHaveLength(1);
    expect(digest.changed[0].path).toBe("crates/ajax-core/src/lifecycle.rs");
    expect(digest.changed[0].added).toBeGreaterThan(0);
  });

  it("has no outcome line for a turn that only talked", () => {
    const digest = digestOf([
      user("u1", "what is this repo?"),
      { kind: "prose", id: "a1", role: "agent", text: "A task runner." },
    ]);

    expect(digest.outcome).toBeNull();
  });
});

describe("which turns open themselves", () => {
  const settled = { ask: "x", outcome: "Ran 1 command", changed: [], failed: false, awaiting: false };

  it("opens the turn the operator came back for", () => {
    expect(opensByDefault(settled, { isLast: true, isLive: false })).toBe(true);
    expect(opensByDefault(settled, { isLast: false, isLive: true })).toBe(true);
  });

  it("opens a turn that still wants an answer", () => {
    expect(
      opensByDefault({ ...settled, awaiting: true }, { isLast: false, isLive: false }),
    ).toBe(true);
  });

  // Opening every failure sounds right and reads wrong: the tool card and its
  // output expand, and three turns of history collapse into one.
  it("leaves a failed turn as a line, because the line already says so", () => {
    expect(
      opensByDefault({ ...settled, failed: true }, { isLast: false, isLive: false }),
    ).toBe(false);
  });

  it("leaves ordinary history closed", () => {
    expect(opensByDefault(settled, { isLast: false, isLive: false })).toBe(false);
  });
});
