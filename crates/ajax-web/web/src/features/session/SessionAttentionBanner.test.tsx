import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import SessionAttentionBanner from "./SessionAttentionBanner";
import type { SessionAttentionItem } from "./types";

const permissionItem: SessionAttentionItem = {
  handle: "web/other-task",
  requestId: "7",
  kind: "permission",
  title: "Permission needed",
  summary: "Permission: Run tests",
};

describe("SessionAttentionBanner", () => {
  it("hides when only the current session has attention", () => {
    render(
      <SessionAttentionBanner
        currentHandle="web/other-task"
        items={[permissionItem]}
        onRespond={vi.fn()}
        onOpenTask={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("ajax-web-session-attention-rail")).toBeNull();
  });

  it("approves a remote permission without navigating", () => {
    const onRespond = vi.fn();
    render(
      <SessionAttentionBanner
        currentHandle="web/current"
        items={[permissionItem]}
        onRespond={onRespond}
        onOpenTask={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId("ajax-web-session-attention-approve"));
    expect(onRespond).toHaveBeenCalledWith(permissionItem, {
      type: "permission",
      outcome: "allow-once",
    });
  });

  it("expands a question composer and sends the reply", () => {
    const onRespond = vi.fn();
    const question: SessionAttentionItem = {
      handle: "web/other-task",
      requestId: "9",
      kind: "question",
      title: "Question",
      summary: "Ship tonight?",
    };
    render(
      <SessionAttentionBanner
        currentHandle="web/current"
        items={[question]}
        onRespond={onRespond}
        onOpenTask={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId("ajax-web-session-attention-reply"));
    fireEvent.change(screen.getByTestId("ajax-web-session-attention-input"), {
      target: { value: "Yes" },
    });
    fireEvent.click(screen.getByTestId("ajax-web-session-attention-send-reply"));
    expect(onRespond).toHaveBeenCalledWith(question, { type: "question", text: "Yes" });
  });

  it("opens a review task and reports the review response", () => {
    const onRespond = vi.fn();
    const onOpenTask = vi.fn();
    const review: SessionAttentionItem = {
      handle: "web/ready",
      requestId: "review:web/ready",
      kind: "review",
      title: "Ready for review",
      summary: "PR ready",
    };
    render(
      <SessionAttentionBanner
        currentHandle="web/current"
        items={[review]}
        onRespond={onRespond}
        onOpenTask={onOpenTask}
      />,
    );
    fireEvent.click(screen.getByTestId("ajax-web-session-attention-open"));
    expect(onRespond).toHaveBeenCalledWith(review, { type: "review", action: "open" });
    expect(onOpenTask).toHaveBeenCalledWith("web/ready");
  });
});
