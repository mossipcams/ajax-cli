import { describe, expect, it } from "vitest";
import { visibleTaskActions } from "./taskActions";

describe("visibleTaskActions", () => {
  it("removes task-open equivalents while preserving real actions", () => {
    expect(
      visibleTaskActions([
        { action: "open", label: "Open", destructive: false, confirmation_required: false },
        { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
        { action: "review", label: "Review", destructive: false, confirmation_required: false },
      ]),
    ).toEqual([
      { action: "review", label: "Review", destructive: false, confirmation_required: false },
    ]);
  });
});
