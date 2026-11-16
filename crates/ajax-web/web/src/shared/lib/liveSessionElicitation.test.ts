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

  it("labels fields without a title by their description, then by key (#1636)", () => {
    expect(
      parseElicitationFormSchema({
        type: "object",
        properties: {
          env: { type: "string", description: "Deployment environment" },
          target: { type: "string", title: "Target", description: "Where to deploy" },
          region: { type: "string" },
        },
      }),
    ).toEqual([
      expect.objectContaining({ name: "env", title: "Deployment environment" }),
      expect.objectContaining({ name: "target", title: "Target" }),
      expect.objectContaining({ name: "region", title: "region" }),
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
