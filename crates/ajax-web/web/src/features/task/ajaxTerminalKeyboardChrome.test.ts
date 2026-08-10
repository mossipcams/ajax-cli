import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readFileSync(join(here, "../../styles.css"), "utf8");

describe("ajax terminal keyboard compact chrome", () => {
  it("caps board height below a typical iOS soft keyboard", () => {
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s*\{[^}]*max-height:\s*220px/,
    );
  });

  it("uses compact button chrome for the Ajax theme", () => {
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-button\s*\{[^}]*height:\s*34px/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-button\s*\{[^}]*min-height:\s*34px/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-button\s*\{[^}]*max-height:\s*36px/,
    );
  });

  it("ships inputMode=none hardening in TaskTerminal", () => {
    const source = readFileSync(join(here, "TaskTerminal.tsx"), "utf8");
    expect(source).toMatch(/setAttribute\(\s*"inputmode"\s*,\s*"none"\s*\)/);
    expect(source).toMatch(/AjaxTerminalKeyboard/);
  });
});
