import { describe, it, expect } from "vitest";
import { headState, headTone, isTaskLevelAttention } from "./headView";

describe("headView", () => {
  describe("headState precedence", () => {
    it("prefers permission decision over agent status", () => {
      expect(
        headState({ requestId: "1", title: "Run?", detail: "" }, null, false, null, "running"),
      ).toBe("decision");
    });

    it("prefers elicitation decision over agent status", () => {
      expect(
        headState(null, { requestId: "e1", message: "Pick env", schema: {}, fields: [] }, false, null, "running"),
      ).toBe("decision");
    });

    it("maps ACP waiting and requires_action to attention", () => {
      expect(headState(null, null, false, null, "waiting")).toBe("attention");
      expect(headState(null, null, false, null, "requires_action")).toBe("attention");
    });

    it("maps ACP running or session busy to working", () => {
      expect(headState(null, null, false, null, "running")).toBe("working");
      expect(headState(null, null, true, null, "idle")).toBe("working");
    });

    it("maps task attention waiting/error to attention", () => {
      expect(headState(null, null, false, { status: "waiting" }, "idle")).toBe("attention");
      expect(headState(null, null, false, { status: "error" }, "idle")).toBe("attention");
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
