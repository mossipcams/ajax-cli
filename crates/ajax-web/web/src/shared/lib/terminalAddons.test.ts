import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Terminal } from "@xterm/xterm";

const fitDispose = vi.fn();
const webLinksDispose = vi.fn();
const serializeDispose = vi.fn();
const loadAddon = vi.fn();
let webLinksHandler: ((event: MouseEvent, uri: string) => void) | undefined;

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: vi.fn(function FitAddonMock(this: { dispose: () => void }) {
    this.dispose = fitDispose;
  }),
}));

vi.mock("@xterm/addon-web-links", () => ({
  WebLinksAddon: vi.fn(function WebLinksAddonMock(
    this: { dispose: () => void },
    handler: typeof webLinksHandler,
  ) {
    webLinksHandler = handler;
    this.dispose = webLinksDispose;
  }),
}));

vi.mock("@xterm/addon-serialize", () => ({
  SerializeAddon: vi.fn(function SerializeAddonMock(this: {
    dispose: () => void;
    serialize: () => string;
  }) {
    this.dispose = serializeDispose;
    this.serialize = vi.fn().mockReturnValue("serialized");
  }),
}));

import { attachTerminalAddons } from "./terminalAddons";

describe("attachTerminalAddons", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    webLinksHandler = undefined;
  });

  it("loads fit, web-links, and serialize addons", () => {
    const term = { loadAddon } as unknown as Terminal;
    const onLinkActivate = vi.fn();

    attachTerminalAddons(term, { onLinkActivate });

    expect(loadAddon).toHaveBeenCalledTimes(3);
  });

  it("forwards link clicks through the Ajax handler instead of navigating", () => {
    const term = { loadAddon } as unknown as Terminal;
    const onLinkActivate = vi.fn();
    const open = vi.fn();
    vi.stubGlobal("open", open);

    attachTerminalAddons(term, { onLinkActivate });
    expect(webLinksHandler).toBeTypeOf("function");

    const event = {
      preventDefault: vi.fn(),
      clientX: 8,
      clientY: 16,
    } as unknown as MouseEvent;
    webLinksHandler?.(event, "https://example.com");

    expect(onLinkActivate).toHaveBeenCalledWith({
      url: "https://example.com",
      clientX: 8,
      clientY: 16,
    });
    expect(open).not.toHaveBeenCalled();
  });

  it("dispose cleans up every addon and link service", () => {
    const term = { loadAddon } as unknown as Terminal;
    const onLinkActivate = vi.fn();
    const bundle = attachTerminalAddons(term, { onLinkActivate });

    bundle.dispose();

    expect(fitDispose).toHaveBeenCalled();
    expect(webLinksDispose).toHaveBeenCalled();
    expect(serializeDispose).toHaveBeenCalled();

    const event = { clientX: 0, clientY: 0 } as MouseEvent;
    webLinksHandler?.(event, "https://example.com");
    expect(onLinkActivate).not.toHaveBeenCalled();
  });
});
