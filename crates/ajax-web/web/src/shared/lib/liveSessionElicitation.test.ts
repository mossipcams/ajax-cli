import { describe, expect, it } from "vitest";
import { buildElicitationContent, isElicitationValid, parseElicitationFormSchema } from "./liveSessionElicitation";

describe("parseElicitationFormSchema", () => {
  it("parses string, enum, boolean, and number fields", () => {
    expect(
      parseElicitationFormSchema({
        type: "object",
        properties: {
          target: { type: "string", title: "Target", enum: ["staging", "production"] },
          confirmed: { type: "boolean", title: "Confirmed" },
          replicas: { type: "number", title: "Replicas", minimum: 1, maximum: 5 },
        },
        required: ["target"],
      }),
    ).toEqual([
      expect.objectContaining({ name: "target", kind: "enum", required: true }),
      expect.objectContaining({ name: "confirmed", kind: "boolean", required: false }),
      expect.objectContaining({ name: "replicas", kind: "number", required: false }),
    ]);
  });
});

describe("isElicitationValid", () => {
  it("rejects required string and enum fields left blank", () => {
    const fields = parseElicitationFormSchema({
      type: "object",
      properties: {
        name: { type: "string", title: "Name" },
        target: { type: "string", title: "Target", enum: ["staging", "production"] },
      },
      required: ["name", "target"],
    });
    expect(isElicitationValid(fields, { name: "", target: "" })).toBe(false);
    expect(isElicitationValid(fields, { name: "Matt", target: "staging" })).toBe(true);
    expect(buildElicitationContent(fields, { name: "", target: "" })).toEqual({});
  });
});
