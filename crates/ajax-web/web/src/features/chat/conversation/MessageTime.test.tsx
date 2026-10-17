import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import Conversation from "./Conversation";
import { formatMessageTime } from "./MessageTime";
import type { ConversationItem } from "../session/public";

describe("formatMessageTime", () => {
  it("renders Chicago time with its zone label", () => {
    const noonUtc = Date.UTC(2026, 0, 15, 18, 5);
    expect(formatMessageTime(noonUtc, noonUtc)).toBe("12:05 PM CST");
    const summer = Date.UTC(2026, 6, 15, 18, 5);
    expect(formatMessageTime(summer, summer)).toBe("1:05 PM CST");
  });

  it("adds the date when the message is from another day", () => {
    const sent = Date.UTC(2026, 0, 15, 18, 5);
    expect(formatMessageTime(sent, sent + 86_400_000)).toBe("Jan 15, 12:05 PM CST");
  });
});

describe("Conversation timestamps", () => {
  it("shows one time under each prompt and response, outside the message text", () => {
    localStorage.clear();
    const items: ConversationItem[] = [
      { kind: "prose", id: "i1", role: "user", text: "Question" },
      { kind: "prose", id: "i2", role: "agent", text: "Answer" },
    ];
    render(<Conversation items={items} busy={false} handle="t1" />);
    expect(screen.getAllByTestId("session-message-time")).toHaveLength(2);
    expect(screen.getByTestId("session-message-user")).toHaveTextContent(/^Question$/);
  });
});
