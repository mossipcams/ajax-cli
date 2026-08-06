import { describe, expect, it } from "vitest";
import {
  buildSessionFeed,
  currentHandleAttentions,
  hasCurrentHandleDecisionPending,
  truncateProgressText,
} from "./sessionCards";
import type { SessionAttentionItem, WebSessionMessage, WebSessionProgress } from "./types";

const permission: SessionAttentionItem = {
  handle: "web/current",
  requestId: "1",
  kind: "permission",
  title: "Permission",
  summary: "Run tests",
};

describe("sessionCards", () => {
  it("truncates progress text at the client limit", () => {
    const long = "a".repeat(300);
    const truncated = truncateProgressText(long);
    expect(truncated.length).toBeLessThanOrEqual(280);
    expect(truncated.endsWith("…")).toBe(true);
  });

  it("builds operator and progress cards plus current-handle decisions", () => {
    const messages: WebSessionMessage[] = [
      { id: "u1", role: "user", text: "hello" },
      { id: "a1", role: "assistant", text: "working", streaming: true },
    ];
    const feed = buildSessionFeed(messages, "web/current", [
      permission,
      { ...permission, handle: "web/other", requestId: "2" },
    ]);
    expect(feed).toHaveLength(3);
    expect(feed[0]).toMatchObject({ kind: "operator", text: "hello" });
    expect(feed[1]).toMatchObject({ kind: "progress", streaming: true });
    expect(feed[2]).toMatchObject({ kind: "decision", attention: permission });
  });

  it("detects current-handle decision pending", () => {
    expect(currentHandleAttentions("web/current", [permission])).toHaveLength(1);
    expect(hasCurrentHandleDecisionPending("web/current", [permission])).toBe(true);
    expect(hasCurrentHandleDecisionPending("web/other", [permission])).toBe(false);
  });

  it("keeps structured tool and file progress as feed cards", () => {
    const progress: WebSessionProgress[] = [
      { id: "tool-1", kind: "tool", toolName: "Run tests", status: "running", summary: "cargo test" },
      { id: "file-1", kind: "file", path: "src/lib.rs", status: "changed", summary: "updated exports" },
    ];
    const feed = buildSessionFeed([], "web/current", [], progress);
    expect(feed).toMatchObject([
      { kind: "tool", toolName: "Run tests", status: "running" },
      { kind: "file", path: "src/lib.rs", status: "changed" },
    ]);
  });
});
