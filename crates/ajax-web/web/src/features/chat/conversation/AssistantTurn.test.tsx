import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import AssistantTurn from "./AssistantTurn";
import type { ConversationItem } from "../session/public";

const agentProse = (text: string): ConversationItem => ({
  kind: "prose",
  id: "a1",
  role: "agent",
  text,
});

describe("AssistantTurn — live pending indicator", () => {
  it("shows the pending indicator with no prose before the first paragraph break", () => {
    render(<AssistantTurn item={agentProse("Still **strea")} live />);

    expect(screen.getByTestId("session-reply-pending")).toBeInTheDocument();
    expect(screen.getByTestId("session-reply-pending")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByTestId("session-message-agent")).not.toHaveTextContent("Still");
    expect(screen.getByTestId("session-message-agent")).not.toHaveTextContent("strea");
  });

  it("shows settled prose and the indicator when a later paragraph is still pending", () => {
    render(
      <AssistantTurn item={agentProse("First paragraph.\n\nSecond partial")} live />,
    );

    expect(screen.getByTestId("session-reply-pending")).toBeInTheDocument();
    expect(screen.getByText("First paragraph.")).toBeInTheDocument();
    expect(screen.queryByText(/Second partial/)).not.toBeInTheDocument();
  });

  it("omits the pending indicator on a settled row", () => {
    render(<AssistantTurn item={agentProse("Done:\n\n- item")} live={false} />);

    expect(screen.queryByTestId("session-reply-pending")).not.toBeInTheDocument();
  });
});

describe("AssistantTurn — copy control", () => {
  beforeEach(() => {
    vi.stubGlobal("navigator", {
      ...navigator,
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("shows a copy control on a settled assistant message", () => {
    render(<AssistantTurn item={agentProse("Done:\n\n- item")} live={false} />);

    expect(screen.getByRole("button", { name: "Copy answer" })).toBeInTheDocument();
  });

  it("omits the copy control on the live streaming row", () => {
    render(<AssistantTurn item={agentProse("Half a sen")} live />);

    expect(screen.queryByRole("button", { name: /copy/i })).not.toBeInTheDocument();
  });

  it("copies markdown source text, not rendered prose", async () => {
    const source = "Done:\n\n- **item**";
    render(<AssistantTurn item={agentProse(source)} live={false} />);

    fireEvent.click(screen.getByRole("button", { name: "Copy answer" }));

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(source);
    });
    expect(screen.getByRole("button", { name: "Copied" })).toHaveAttribute("data-copied", "true");
  });

  it("does not throw or show success when clipboard write is rejected", async () => {
    vi.mocked(navigator.clipboard.writeText).mockRejectedValue(new DOMException("denied"));

    render(<AssistantTurn item={agentProse("Settled answer.")} live={false} />);

    expect(() => {
      fireEvent.click(screen.getByRole("button", { name: "Copy answer" }));
    }).not.toThrow();

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith("Settled answer.");
    });
    expect(screen.getByRole("button", { name: "Copy answer" })).toHaveAttribute(
      "data-copied",
      "false",
    );
  });
});

describe("Conversation — copy control placement", () => {
  it("does not add a copy control to operator messages", async () => {
    const { default: UserTurn } = await import("./UserTurn");
    const item: ConversationItem = {
      kind: "prose",
      id: "u1",
      role: "user",
      text: "Explain it",
    };
    render(<UserTurn item={item} />);

    expect(screen.queryByRole("button", { name: /copy/i })).not.toBeInTheDocument();
  });
});
