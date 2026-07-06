// @ts-nocheck
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = process.cwd();
const packageSource = readFileSync(resolve(root, "package.json"), "utf8");

describe("terminal renderer rollback", () => {
  it("does not depend on the rcarmo ghostty-web fork", () => {
    expect(packageSource).not.toContain("github:rcarmo/ghostty-web");
    expect(packageSource).not.toContain("#v0.9.4");
  });
});
