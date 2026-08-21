import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import TurnActivity from "./TurnActivity";
import type { ConversationItem } from "../session/public";

describe("TurnActivity", () => {
  it("renders nothing when a turn has no work items", () => {
    const { container } = render(<TurnActivity items={[]} live={false} attention={false} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows a summary row when work items exist", () => {
    const items: ConversationItem[] = [
      {
        kind: "tool",
        id: "t1",
        call: {
          callId: "t1",
          title: "Read",
          kind: "read",
          status: "completed",
          locations: ["/repo/a.ts"],
          content: [],
        },
      },
    ];
    render(<TurnActivity items={items} live={false} attention={false} />);
    expect(screen.getByTestId("session-turn-work-summary")).toBeInTheDocument();
  });
});
