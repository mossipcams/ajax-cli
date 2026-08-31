import { describe, it, expect, afterEach } from "vitest";
import {
  claimSessionViewportOwnership,
  releaseSessionViewportOwnership,
  sessionSurfaceStyle,
  SESSION_VIEWPORT_ATTR,
} from "@/shared/lib/sessionViewport";

describe("session viewport helpers", () => {
  afterEach(() => {
    releaseSessionViewportOwnership();
  });

  it("claims and releases session viewport ownership on documentElement", () => {
    claimSessionViewportOwnership();
    expect(document.documentElement.getAttribute(SESSION_VIEWPORT_ATTR)).toBe("owned");
    releaseSessionViewportOwnership();
    expect(document.documentElement.hasAttribute(SESSION_VIEWPORT_ATTR)).toBe(false);
  });

  it("returns undefined surface style when keyboard is open (#1122 CSS chain owns band)", () => {
    expect(sessionSurfaceStyle(800, 800, false)).toBeUndefined();
    expect(sessionSurfaceStyle(800, 500, true)).toBeUndefined();
    expect(sessionSurfaceStyle(500, 480, true)).toBeUndefined();
  });
});
