import { describe, it, expect } from "vitest";
import { headState, headTone, isTaskLevelAttention } from "./headView";

describe("headView", () => {
  describe("headState precedence", () => {
    it("prefers permission decision over agent status", () => {
      expect(
        headState({ requestId: "1", title: "Run?", detail: "" }, null, false, null),
      ).toBe("decision");
    });

    it("prefers elicitation decision over agent status", () => {
      expect(
        headState(null, { requestId: "e1", message: "Pick env", schema: {}, fields: [] }, false, null),
      ).toBe("decision");
    });

    it("maps session busy to working", () => {
      expect(headState(null, null, true, null)).toBe("working");
    });

    it("does not derive state from raw ACP status", () => {
      expect(headState(null, null, false, null)).toBe("idle");
    });

    it("maps task attention waiting/error to attention", () => {
      expect(headState(null, null, false, { status: "waiting" })).toBe("attention");
      expect(headState(null, null, false, { status: "error" })).toBe("attention");
    });
  });

  describe("headTone", () => {
    it("uses error tone for task attention errors", () => {
      expect(headTone("attention", { status: "error" })).toBe("error");
    });
  });

  describe("isTaskLevelAttention", () => {
    it("is false when an ACP decision owns the head", () => {
      expect(
        isTaskLevelAttention(
          "attention",
          { status: "waiting" },
          { requestId: "1", title: "Run?", detail: "" },
        ),
      ).toBe(false);
    });
  });
});
