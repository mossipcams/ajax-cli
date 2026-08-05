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

  it("shows status then approve without navigating", () => {
    const onRespond = vi.fn();
    const onOpenTask = vi.fn();
    render(
      <SessionAttentionBanner
        currentHandle="web/current"
        items={[permissionItem]}
        onRespond={onRespond}
        onOpenTask={onOpenTask}
      />,
    );
    expect(screen.getByTestId("ajax-web-session-attention-banner")).toHaveTextContent(
      "Needs permission",
    );
    expect(screen.getByTestId("ajax-web-session-attention-banner")).toHaveTextContent(
      "web/other-task",
    );
    fireEvent.click(screen.getByTestId("ajax-web-session-attention-approve"));
    expect(onRespond).toHaveBeenCalledWith(permissionItem, {
      type: "permission",
      outcome: "allow-once",
    });
    expect(onOpenTask).not.toHaveBeenCalled();
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

  it("opens the review task from the Open action", () => {
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
    expect(screen.queryByTestId("ajax-web-session-attention-hit")).toBeNull();
    fireEvent.click(screen.getByTestId("ajax-web-session-attention-open"));
    expect(onRespond).toHaveBeenCalledWith(review, { type: "review", action: "open" });
    expect(onOpenTask).toHaveBeenCalledWith("web/ready");
  });

  it("animates in as a top-of-page toast", () => {
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      cb(0);
      return 1;
    });
    render(
      <SessionAttentionBanner
        currentHandle="web/current"
        items={[permissionItem]}
        onRespond={vi.fn()}
        onOpenTask={vi.fn()}
      />,
    );
    expect(screen.getByTestId("ajax-web-session-attention-rail").className).toContain(
      "is-visible",
    );
    vi.unstubAllGlobals();
  });
});
