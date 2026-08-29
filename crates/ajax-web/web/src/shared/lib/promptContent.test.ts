import { describe, expect, it, vi } from "vitest";
import {
  attachmentFromFile,
  attachmentsArePreparing,
  estimatePromptFrameBytes,
  fitPromptContentBlocks,
  flattenAttachmentBlocks,
  hasComposerPromptContent,
  hasPromptContent,
  hasReadyAttachments,
  MAX_PROMPT_FRAME_BYTES,
  prepareImageFileForPrompt,
  promptFrameFits,
} from "./promptContent";

describe("promptContent", () => {
  // ajax-cli#1110: prompt content may be text, blocks, or both.
  it("treats an attachment-only prompt as content while rejecting a truly empty prompt", () => {
    const blocks = [{ type: "image" as const, data: "aGVsbG8=", mimeType: "image/png" }];
    expect(hasPromptContent("   ", blocks)).toBe(true);
    expect(hasPromptContent("hello", [])).toBe(true);
    expect(hasPromptContent("   ", [])).toBe(false);
  });

  it("returns null when neither image nor embeddedContext is advertised", async () => {
    const file = new File(["hello"], "notes.md", { type: "text/markdown" });
    await expect(
      attachmentFromFile(file, { image: false, embeddedContext: false }),
    ).resolves.toBeNull();
  });

  it("embeds text files when embeddedContext is advertised", async () => {
    const file = new File(["hello"], "notes.txt", { type: "text/plain" });
    const attachment = await attachmentFromFile(file, { embeddedContext: true });
    expect(attachment?.blocks[0]).toMatchObject({
      type: "resource",
      text: "hello",
    });
  });

  it("flattens only ready attachment blocks in order", () => {
    const blocks = flattenAttachmentBlocks([
      {
        id: "a",
        label: "one",
        status: "ready",
        blocks: [{ type: "resource_link", name: "one", uri: "file:///one" }],
      },
      {
        id: "b",
        label: "two",
        status: "preparing",
        blocks: [{ type: "image", data: "aGVsbG8=", mimeType: "image/png" }],
      },
      {
        id: "c",
        label: "three",
        status: "ready",
        blocks: [{ type: "image", data: "aGVsbG8=", mimeType: "image/png" }],
      },
    ]);
    expect(blocks).toHaveLength(2);
    expect(blocks[1]).toMatchObject({ type: "image" });
  });

  it("counts only ready attachments for composer send eligibility", () => {
    const ready = {
      id: "a",
      label: "photo.jpg",
      status: "ready" as const,
      blocks: [{ type: "image" as const, data: "aGVsbG8=", mimeType: "image/jpeg" }],
    };
    const preparing = { ...ready, id: "b", status: "preparing" as const, blocks: [] };
    expect(hasComposerPromptContent("", [preparing])).toBe(false);
    expect(hasComposerPromptContent("", [ready])).toBe(true);
    expect(hasReadyAttachments([preparing])).toBe(false);
    expect(attachmentsArePreparing([preparing])).toBe(true);
  });

  it("prepares a large image file at attach time so the prompt frame fits", async () => {
    const hugeData = "A".repeat(300_000);
    const file = new File([hugeData], "photo.jpg", { type: "image/jpeg" });

    const nativeCreateElement = Document.prototype.createElement;
    vi.spyOn(document, "createElement").mockImplementation(function (this: Document, tagName: string) {
      const element = nativeCreateElement.call(this, tagName);
      if (tagName !== "canvas") return element;
      const canvas = element as HTMLCanvasElement;
      canvas.getContext = vi.fn(() => ({
        drawImage: vi.fn(),
      })) as unknown as HTMLCanvasElement["getContext"];
      canvas.toDataURL = vi.fn((_type?: string, quality?: number) => {
        const scale = typeof quality === "number" ? quality : 0.92;
        const targetLen = Math.max(512, Math.floor(1200 * scale));
        return `data:image/jpeg;base64,${"B".repeat(targetLen)}`;
      });
      return canvas;
    });
    vi.spyOn(globalThis, "Image").mockImplementation(function MockImage(this: HTMLImageElement) {
      Object.defineProperty(this, "naturalWidth", { value: 4000 });
      Object.defineProperty(this, "naturalHeight", { value: 3000 });
      setTimeout(() => this.onload?.(new Event("load")), 0);
      return this;
    } as unknown as typeof Image);

    const prepared = await prepareImageFileForPrompt(file, "hello");
    expect("error" in prepared).toBe(false);
    if ("error" in prepared) return;
    expect(promptFrameFits("hello", prepared.blocks)).toBe(true);
  });

  it("returns an error when image preparation cannot decode the file", async () => {
    vi.spyOn(globalThis, "Image").mockImplementation(function MockImage(this: HTMLImageElement) {
      setTimeout(() => this.onerror?.(new Event("error")), 0);
      return this;
    } as unknown as typeof Image);

    const file = new File(["x".repeat(300_000)], "bad.jpg", { type: "image/jpeg" });
    const prepared = await prepareImageFileForPrompt(file);
    expect(prepared).toEqual({ error: expect.stringContaining("too large") });
  });

  it("compresses a large image attachment so the prompt frame fits", async () => {
    const hugeData = "A".repeat(300_000);
    const blocks = [{ type: "image" as const, data: hugeData, mimeType: "image/jpeg" }];
    expect(estimatePromptFrameBytes("hello", blocks)).toBeGreaterThan(MAX_PROMPT_FRAME_BYTES);

    const nativeCreateElement = Document.prototype.createElement;
    vi.spyOn(document, "createElement").mockImplementation(function (this: Document, tagName: string) {
      const element = nativeCreateElement.call(this, tagName);
      if (tagName !== "canvas") return element;
      const canvas = element as HTMLCanvasElement;
      canvas.getContext = vi.fn(() => ({
        drawImage: vi.fn(),
      })) as unknown as HTMLCanvasElement["getContext"];
      canvas.toDataURL = vi.fn((_type?: string, quality?: number) => {
        const scale = typeof quality === "number" ? quality : 0.92;
        const targetLen = Math.max(512, Math.floor(1200 * scale));
        return `data:image/jpeg;base64,${"B".repeat(targetLen)}`;
      });
      return canvas;
    });
    vi.spyOn(globalThis, "Image").mockImplementation(function MockImage(this: HTMLImageElement) {
      Object.defineProperty(this, "naturalWidth", { value: 4000 });
      Object.defineProperty(this, "naturalHeight", { value: 3000 });
      setTimeout(() => this.onload?.(new Event("load")), 0);
      return this;
    } as unknown as typeof Image);

    const fitted = await fitPromptContentBlocks("hello", blocks);
    expect(fitted.error).toBeUndefined();
    expect(promptFrameFits("hello", fitted.blocks)).toBe(true);
  });
});
