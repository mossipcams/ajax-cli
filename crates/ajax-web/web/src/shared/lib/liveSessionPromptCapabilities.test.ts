import { describe, expect, it } from "vitest";
import {
  allowsEmbeddedResources,
  allowsImageAttachments,
  parseLivePromptCapabilities,
} from "./liveSessionPromptCapabilities";

describe("parseLivePromptCapabilities", () => {
  it("parses advertised image and embeddedContext flags", () => {
    expect(parseLivePromptCapabilities({ image: true, embeddedContext: true })).toEqual({
      image: true,
      embeddedContext: true,
    });
  });

  it("returns explicit false defaults when nothing is advertised", () => {
    expect(parseLivePromptCapabilities({})).toEqual({
      image: false,
      embeddedContext: false,
    });
  });
});

describe("attachment gating", () => {
  it("allows image only when advertised", () => {
    expect(allowsImageAttachments({ image: true })).toBe(true);
    expect(allowsImageAttachments({ image: false, embeddedContext: true })).toBe(false);
  });

  it("allows embedded resources only when advertised", () => {
    expect(allowsEmbeddedResources({ embeddedContext: true })).toBe(true);
    expect(allowsEmbeddedResources({ image: true })).toBe(false);
  });
});
