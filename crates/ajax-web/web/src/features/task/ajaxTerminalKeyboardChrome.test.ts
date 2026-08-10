import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readFileSync(join(here, "../../styles.css"), "utf8");

describe("ajax terminal keyboard compact chrome", () => {
  it("pins the Ajax board to the page bottom", () => {
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s*\{[^}]*position:\s*fixed/,
    );
    expect(stylesSource).toMatch(/\.ajax-terminal-keyboard\s*\{[^}]*bottom:\s*0/);
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s*\{[^}]*padding-bottom:\s*env\(safe-area-inset-bottom/,
    );
  });

  it("labels the dismiss key as Done", () => {
    const layout = readFileSync(join(here, "ajaxTerminalKeyboardLayout.ts"), "utf8");
    expect(layout).toMatch(/"\{hide\}":\s*"Done"/);
  });

  it("uses Soft Steel Blue return and Soft Charcoal paper steps", () => {
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s*\{[^}]*background:\s*var\(--paper-tint\)/,
    );
    expect(stylesSource).toMatch(
      /\.hg-button\.ajax-kb-enter\s*\{[^}]*background:\s*var\(--soft-steel-blue\)/,
    );
    expect(stylesSource).toMatch(
      /\.hg-button\.ajax-kb-enter\s*\{[^}]*color:\s*var\(--soft-charcoal\)/,
    );
    expect(stylesSource).toMatch(
      /\.hg-button\.ajax-kb-done\s*\{[^}]*background:\s*transparent/,
    );
  });

  it("uses iOS inter-key gaps and zeros library button margins", () => {
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-rows\s*\{[^}]*gap:\s*11px/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-row\s*\{[^}]*gap:\s*6px/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-row:not\(:last-child\)\s*\{[^}]*margin-bottom:\s*0/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-row\s+\.hg-button:not\(:last-child\)[\s\S]*?\{[^}]*margin-right:\s*0/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-button\s*\{[^}]*height:\s*36px/,
    );
  });

  it("ships inputMode=none hardening in TaskTerminal", () => {
    const source = readFileSync(join(here, "TaskTerminal.tsx"), "utf8");
    expect(source).toMatch(/setAttribute\(\s*"inputmode"\s*,\s*"none"\s*\)/);
    expect(source).toMatch(/AjaxTerminalKeyboard/);
  });
});
