import { describe, it, expect, vi, afterEach } from "vitest";
import { createTerminalLinkService } from "./terminalLinkService";

vi.mock("./clipboard", () => ({
  copyText: vi.fn().mockResolvedValue(true),
}));

import { copyText } from "./clipboard";

describe("createTerminalLinkService", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("onOpen opens with noopener and noreferrer", () => {
    const open = vi.fn();
    vi.stubGlobal("open", open);
    const service = createTerminalLinkService();

    service.onOpen("https://example.com/path");

    expect(open).toHaveBeenCalledWith(
      "https://example.com/path",
      "_blank",
      "noopener,noreferrer",
    );
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
    const open = vi.fn();
    vi.stubGlobal("open", open);
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
