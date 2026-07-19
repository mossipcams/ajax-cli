import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

// Repo root lives four levels above this file:
//   crates/ajax-web/web/src/toolchain.test.ts -> repo root
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../../../");

type PackageJson = {
  scripts?: Record<string, string>;
  devDependencies?: Record<string, string>;
};

function readPackageJson(): PackageJson {
  const raw = readFileSync(resolve(repoRoot, "package.json"), "utf8");
  return JSON.parse(raw) as PackageJson;
}

// After the TypeScript alias inversion, `node_modules/.bin/tsc` resolves to the
// hoisted TypeScript 5 binary (it is the primary `typescript` dependency).
// If `web:check` invokes a bare `tsc`, it silently downgrades the typechecker
// to TS 5 with no failing signal while `web:check` believes it is running TS 7.
// These assertions encode that contract so a regression in the wiring fails
// loudly instead of typechecking against the wrong compiler.
describe("web toolchain wiring", () => {
  const pkg = readPackageJson();
  const webCheck = pkg.scripts?.["web:check"] ?? "";

  it("web:check references the typescript-7 alias path, not a bare tsc", () => {
    expect(webCheck, "web:check must exist").toBeTypeOf("string");
    expect(webCheck.length, "web:check must not be empty").toBeGreaterThan(0);
    expect(
      webCheck.includes("node_modules/typescript-7/bin/tsc"),
      `web:check must invoke the typescript-7 alias binary; got: ${webCheck}`,
    ).toBe(true);
    expect(
      /^\s*tsc\b/.test(webCheck),
      `web:check must not invoke a bare hoisted \`tsc\` (would resolve to TS 5); got: ${webCheck}`,
    ).toBe(false);
  });

  it("typescript-7 devDependency is an npm: alias pinned to a 7.x release", () => {
    const alias = pkg.devDependencies?.["typescript-7"];
    expect(alias, "devDependencies.typescript-7 must exist").toBeTypeOf("string");
    expect(
      /^npm:typescript@7\./.test(alias as string),
      `typescript-7 must be an npm:typescript@ alias pinned to a 7.x version; got: ${alias}`,
    ).toBe(true);
  });

  it("web:lint script exists and invokes eslint", () => {
    const webLint = pkg.scripts?.["web:lint"] ?? "";
    expect(webLint, "web:lint must exist").toBeTypeOf("string");
    expect(
      /\beslint\b/.test(webLint),
      `web:lint must invoke eslint; got: ${webLint}`,
    ).toBe(true);
  });

  it("verify script runs web:lint", () => {
    const verify = pkg.scripts?.["verify"] ?? "";
    expect(verify, "verify must exist").toBeTypeOf("string");
    expect(
      verify.includes("npm run web:lint"),
      `verify must include \`npm run web:lint\`; got: ${verify}`,
    ).toBe(true);
  });
});