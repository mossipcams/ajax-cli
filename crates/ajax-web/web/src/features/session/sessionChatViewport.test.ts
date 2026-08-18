import { describe, it, expect, afterEach } from "vitest";
import {
  claimSessionViewportOwnership,
  releaseSessionViewportOwnership,
  sessionSurfaceStyle,
  SESSION_VIEWPORT_ATTR,
} from "./sessionChatViewport";

describe("sessionChatViewport", () => {
  afterEach(() => {
    releaseSessionViewportOwnership();
  });

  it("claims and releases session viewport ownership on documentElement", () => {
    claimSessionViewportOwnership();
    expect(document.documentElement.getAttribute(SESSION_VIEWPORT_ATTR)).toBe("owned");
    releaseSessionViewportOwnership();
    expect(document.documentElement.hasAttribute(SESSION_VIEWPORT_ATTR)).toBe(false);
  });

  it("returns paddingBottom style only for iOS Safari keyboard band", () => {
    expect(sessionSurfaceStyle(800, 800, false)).toBeUndefined();
    expect(sessionSurfaceStyle(800, 500, true)).toEqual({ paddingBottom: 300 });
    expect(sessionSurfaceStyle(500, 480, true)).toBeUndefined();
  });
});
