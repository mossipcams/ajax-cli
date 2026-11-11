import { describe, it, expect } from "vitest";
import { join } from "node:path";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";

const ERROR_TINT = "rgba(235, 130, 123, 0.14)";

describe("ResultPanel error toast styles (#1602)", () => {
  const testDir = (import.meta as ImportMeta & { dirname: string }).dirname;
  const css = readOrderedStylesSource(join(testDir, "../.."));

  it("keeps the opaque raised paper under the translucent error tint", () => {
    const block = css.match(/\.result-panel\.is-error\s*\{([^}]*)\}/)?.[1] ?? "";
    const background = block.match(/background:\s*([^;]+);/)?.[1] ?? "";

    expect(background).toContain("var(--paper-raised)");
    expect(background).toContain(ERROR_TINT);
  });
});
