import { describe, it, expect } from "vitest";
import { slugifyTaskTitle, startTaskHandle } from "./taskSlug";

describe("taskSlug", () => {
  it("slugifies a title the same way core slugify_title does", () => {
    expect(slugifyTaskTitle("Fix Login")).toBe("fix-login");
  });

  it("builds a repo/slug handle for navigation", () => {
    expect(startTaskHandle("web", "Fix Login")).toBe("web/fix-login");
  });
});
