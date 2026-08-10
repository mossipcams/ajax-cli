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
      /\.ajax-terminal-keyboard\s*\{[^}]*padding-bottom:\s*calc\(var\(--ajax-kb-bottom\)\s*\+\s*env\(safe-area-inset-bottom/,
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
  });

  it("matches measured iOS portrait key-plane metrics", () => {
    expect(stylesSource).toMatch(/--ajax-kb-gap-x:\s*6px/);
    expect(stylesSource).toMatch(/--ajax-kb-gap-y:\s*10px/);
    expect(stylesSource).toMatch(/--ajax-kb-side:\s*3px/);
    expect(stylesSource).toMatch(/--ajax-kb-key-h:\s*clamp\(39px/);
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-rows\s*\{[^}]*gap:\s*var\(--ajax-kb-gap-y\)/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-theme\s+\.hg-row\s*\{[^}]*gap:\s*var\(--ajax-kb-gap-x\)/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-kb-key-wrap\.ajax-kb-wrap-mod\s*\{[^}]*flex:\s*1\.33/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-kb-key-wrap\.ajax-kb-wrap-bksp\s*\{[^}]*flex:\s*1\.33/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-kb-key-wrap\.ajax-kb-wrap-enter\s*\{[^}]*flex:\s*2\.78/,
    );
    expect(stylesSource).toMatch(
      /\.ajax-kb-key-wrap\.ajax-kb-wrap-space\s*\{[^}]*flex:\s*5\.79/,
    );
    expect(stylesSource).toMatch(
      /\.hg-button\.ajax-kb-half\s*\{[^}]*margin:\s*0\s+calc\(var\(--ajax-kb-gap-x\)\s*\/\s*-2\)/,
    );
  });

  it("ships WebKit switch haptic hit-targets as siblings of keys", () => {
    expect(stylesSource).toMatch(/\.ajax-terminal-keyboard\s+\.ajax-kb-key-wrap\s*\{/);
    expect(stylesSource).toMatch(
      /\.ajax-terminal-keyboard\s+\.ajax-kb-haptic-hit\s*\{[^}]*opacity:\s*0\.001/,
    );
    const haptics = readFileSync(join(here, "ajaxTerminalKeyboardHaptics.ts"), "utf8");
    expect(haptics).toMatch(/setAttribute\(\s*"switch"\s*,\s*""\s*\)/);
    expect(haptics).toMatch(/ajax-kb-key-wrap/);
    const keyboard = readFileSync(join(here, "AjaxTerminalKeyboard.tsx"), "utf8");
    expect(keyboard).toMatch(/attachAjaxKeyboardHaptics/);
  });

  it("ships inputMode=none hardening in TaskTerminal", () => {
    const source = readFileSync(join(here, "TaskTerminal.tsx"), "utf8");
    expect(source).toMatch(/setAttribute\(\s*"inputmode"\s*,\s*"none"\s*\)/);
    expect(source).toMatch(/AjaxTerminalKeyboard/);
  });
});
