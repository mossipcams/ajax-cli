import { describe, expect, it } from "vitest";
import {
  imageSource,
  parseOutputContentBlock,
  parseToolContent,
  resourceLabel,
} from "./liveSessionOutputContent";

describe("liveSessionOutputContent", () => {
  it("parses image and resource_link blocks", () => {
    expect(
      parseOutputContentBlock({
        type: "image",
        mimeType: "image/png",
        uri: "https://example.com/a.png",
      }),
    ).toEqual({
      type: "image",
      mimeType: "image/png",
      uri: "https://example.com/a.png",
    });

    expect(
      parseOutputContentBlock({
        type: "resource_link",
        name: "README.md",
        uri: "file:///README.md",
      }),
    ).toEqual({
      type: "resource_link",
      name: "README.md",
      uri: "file:///README.md",
    });
  });

  it("builds image src from uri or data", () => {
    expect(
      imageSource({
        type: "image",
        mimeType: "image/png",
        uri: "https://example.com/a.png",
      }),
    ).toBe("https://example.com/a.png");
    expect(
      imageSource({
        type: "image",
        mimeType: "image/png",
        data: "aGVsbG8=",
      }),
    ).toBe("data:image/png;base64,aGVsbG8=");
  });

  it("drops terminal tool content", () => {
    expect(parseToolContent({ type: "terminal", terminalId: "term-1" })).toBeNull();
  });

  it("labels resource links by title or name", () => {
    expect(
      resourceLabel({
        type: "resource_link",
        name: "README.md",
        uri: "file:///README.md",
        title: "Project readme",
      }),
    ).toBe("Project readme");
  });
});
