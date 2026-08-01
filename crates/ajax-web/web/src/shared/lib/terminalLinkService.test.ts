import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { createTerminalLinkService } from "./terminalLinkService";

vi.mock("./clipboard", () => ({
  copyText: vi.fn().mockResolvedValue(true),
}));

import { copyText } from "./clipboard";

type AnchorRecorder = {
  clickSpy: ReturnType<typeof vi.spyOn>;
  lastAnchor: () => HTMLAnchorElement | null;
  anchorCount: () => number;
};

function spyAnchorClick(): AnchorRecorder {
  let last: HTMLAnchorElement | null = null;
  const appended: HTMLAnchorElement[] = [];

  const clickSpy = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(() => {
      // Intentionally no-op: we don't want jsdom to attempt a real
      // navigation when the anchor is clicked.
    });

  const originalAppend = document.body.appendChild.bind(document.body);
  vi.spyOn(document.body, "appendChild").mockImplementation((node) => {
    if (node instanceof HTMLAnchorElement) {
      appended.push(node);
      last = node;
    }
    return originalAppend(node as Node);
  });

  const originalRemove = Element.prototype.remove;
  vi
    .spyOn(Element.prototype, "remove")
    .mockImplementation(function (this: Element) {
      const idx = this instanceof HTMLAnchorElement
        ? appended.indexOf(this)
        : -1;
      if (idx >= 0) appended.splice(idx, 1);
      originalRemove.call(this);
    });

  return {
    clickSpy,
    lastAnchor: () => last,
    anchorCount: () => document.body.querySelectorAll("a").length,
  };
}

describe("createTerminalLinkService", () => {
  let open: ReturnType<typeof vi.fn>;
  let initialHref: string;
  let recorder: AnchorRecorder;

  beforeEach(() => {
    open = vi.fn();
    vi.stubGlobal("open", open);
    initialHref = window.location.href;
    recorder = spyAnchorClick();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("onOpen opens https:// with noopener and noreferrer via blank anchor only", () => {
    const service = createTerminalLinkService();

    service.onOpen("https://example.com/path");

    const anchor = recorder.lastAnchor();
    expect(anchor).not.toBeNull();
    expect(anchor!.target).toBe("_blank");
    expect(anchor!.rel.split(/\s+/)).toEqual(
      expect.arrayContaining(["noopener", "noreferrer"]),
    );
    expect(anchor!.href).toBe("https://example.com/path");
    expect(recorder.clickSpy).toHaveBeenCalledTimes(1);
    // Successful opens must use the blank anchor only, never window.open,
    // which can replace the Ajax document on iOS standalone PWAs.
    expect(open).not.toHaveBeenCalled();
    expect(window.location.href).toBe(initialHref);
    // Anchor is removed from the DOM after click.
    expect(recorder.anchorCount()).toBe(0);
  });

  it("onOpen opens http:// with noopener and noreferrer via blank anchor only and never sets window.location", () => {
    const service = createTerminalLinkService();

    service.onOpen("http://example.com/");

    const anchor = recorder.lastAnchor();
    expect(anchor).not.toBeNull();
    expect(anchor!.target).toBe("_blank");
    expect(anchor!.rel.split(/\s+/)).toEqual(
      expect.arrayContaining(["noopener", "noreferrer"]),
    );
    expect(anchor!.href).toBe("http://example.com/");
    expect(recorder.clickSpy).toHaveBeenCalledTimes(1);
    expect(open).not.toHaveBeenCalled();
    expect(window.location.href).toBe(initialHref);
    expect(recorder.anchorCount()).toBe(0);
  });

  it.each([
    ["javascript:alert(1)"],
    ["data:text/html,hi"],
    ["file:///tmp"],
    ["not a url"],
    ["https://user:pass@example.com/x"],
    ["https://user@example.com/x"],
  ])("onOpen rejects %s and does not open anything", (badUrl) => {
    const service = createTerminalLinkService();

    service.onOpen(badUrl);

    expect(recorder.clickSpy).not.toHaveBeenCalled();
    expect(recorder.lastAnchor()).toBeNull();
    expect(open).not.toHaveBeenCalled();
    expect(window.location.href).toBe(initialHref);
    expect(recorder.anchorCount()).toBe(0);
  });

  it("onCopy returns true when copyText succeeds", async () => {
    vi.mocked(copyText).mockResolvedValue(true);
    const service = createTerminalLinkService();

    const copied = await service.onCopy("https://example.com");

    expect(copyText).toHaveBeenCalledWith("https://example.com");
    expect(copied).toBe(true);
  });

  it("onCopy returns false when copyText fails", async () => {
    vi.mocked(copyText).mockResolvedValue(false);
    const service = createTerminalLinkService();

    const copied = await service.onCopy("https://example.com");

    expect(copyText).toHaveBeenCalledWith("https://example.com");
    expect(copied).toBe(false);
  });

  it("handleLinkClick notifies subscribers and does not call window.open", () => {
    const service = createTerminalLinkService();
    const listener = vi.fn();
    service.subscribe(listener);
    const event = {
      preventDefault: vi.fn(),
      clientX: 12,
      clientY: 34,
    };

    service.handleLinkClick(event, "https://ajax.dev");

    expect(event.preventDefault).toHaveBeenCalled();
    expect(listener).toHaveBeenCalledWith({
      url: "https://ajax.dev",
      clientX: 12,
      clientY: 34,
    });
    expect(open).not.toHaveBeenCalled();
  });

  it("dispose stops further notifications", () => {
    const service = createTerminalLinkService();
    const listener = vi.fn();
    service.subscribe(listener);
    service.dispose();

    service.handleLinkClick({ clientX: 0, clientY: 0 }, "https://example.com");

    expect(listener).not.toHaveBeenCalled();
  });
});
