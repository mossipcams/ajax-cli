import { describe, it, expect } from "vitest";
import { autoGrow } from "./autoGrow";

describe("autoGrow", () => {
  it("grows the textarea when content exceeds the visible height", () => {
    const node = document.createElement("textarea");
    Object.defineProperty(node, "scrollHeight", { value: 120, configurable: true });
    Object.defineProperty(node, "clientHeight", { value: 40, configurable: true });

    autoGrow(node, false);
    expect(node.style.height).toBe("120px");
  });

  it("resets height before shrinking", () => {
    const node = document.createElement("textarea");
    node.style.height = "120px";
    Object.defineProperty(node, "scrollHeight", { value: 40, configurable: true });
    Object.defineProperty(node, "clientHeight", { value: 120, configurable: true });

    autoGrow(node, true);
    expect(node.style.height).toBe("40px");
  });
});
