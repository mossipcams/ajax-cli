import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../../..");
const designMd = readFileSync(join(repoRoot, "DESIGN.md"), "utf8");
const stylesCss = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "styles.css"),
  "utf8",
);

/** Parse `colors:` hex map from DESIGN.md YAML frontmatter. */
function designColors(): Record<string, string> {
  const fm = designMd.split("---", 2)[1] ?? "";
  const colorsBlock = fm.match(/\ncolors:\n([\s\S]*?)\n(?:typography|rounded|spacing|components):/)?.[1] ?? "";
  const out: Record<string, string> = {};
  for (const match of colorsBlock.matchAll(/^\s{2}([a-z0-9-]+):\s*"?(#[0-9a-fA-F]{6})"?\s*$/gm)) {
    out[match[1]] = match[2].toLowerCase();
  }
  return out;
}

/** Resolve `--name: value` from :root, following one level of `var(--other)`. */
function rootCustomProps(css: string): Record<string, string> {
  const root = css.match(/:root\s*\{([\s\S]*?)\n\}/)?.[1] ?? "";
  const raw: Record<string, string> = {};
  for (const match of root.matchAll(/--([a-z0-9-]+)\s*:\s*([^;]+);/g)) {
    raw[match[1]] = match[2].trim().toLowerCase();
  }
  const resolve = (name: string, depth = 0): string | undefined => {
    const value = raw[name];
    if (!value) return undefined;
    const ref = value.match(/^var\(--([a-z0-9-]+)\)$/);
    if (ref && depth < 4) return resolve(ref[1], depth + 1);
    return value;
  };
  const out: Record<string, string> = {};
  for (const name of Object.keys(raw)) {
    const resolved = resolve(name);
    if (resolved) out[name] = resolved;
  }
  return out;
}

describe("DESIGN.md color lock", () => {
  it("exposes every DESIGN.md color as a :root custom property with the same hex", () => {
    const design = designColors();
    const css = rootCustomProps(stylesCss);

    expect(Object.keys(design).length).toBeGreaterThanOrEqual(18);
    for (const [name, hex] of Object.entries(design)) {
      expect(css[name], `--${name} missing or drifted`).toBe(hex);
    }
  });

  it("keeps role aliases pointed at the DESIGN.md palette", () => {
    const css = rootCustomProps(stylesCss);
    expect(css.paper).toBe(css["soft-charcoal"]);
    expect(css.accent).toBe(css["soft-steel-blue"]);
    expect(css.warn).toBe(css["attention-amber"]);
    expect(css.danger).toBe(css["fault-rose"]);
    expect(css.ok).toBe(css["done-sage"]);
  });
});

describe("Tailwind contract", () => {
  it("imports only Tailwind utilities (preflight and theme off)", () => {
    expect(stylesCss).toMatch(
      /@import\s+"tailwindcss\/utilities"(?:\s+layer\([^)]*\))?\s*;/,
    );
    expect(stylesCss).not.toMatch(/@import\s+"tailwindcss"\s*;/);
    expect(stylesCss).not.toMatch(/tailwindcss\/preflight/);
    expect(stylesCss).not.toMatch(/tailwindcss\/theme/);
  });

  it("declares a single @theme inline block mapped only to existing tokens", () => {
    const blocks = stylesCss.match(/@theme\s+inline\s*\{[\s\S]*?\n\}/g) ?? [];
    expect(blocks).toHaveLength(1);
    const inner = blocks[0].replace(/^[\s\S]*?\{/, "").replace(/\n\}[\s\S]*$/, "");
    expect(inner).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    const decls = inner.match(/--color-[a-z0-9-]+\s*:\s*[^;\n]+;/g) ?? [];
    expect(decls.length).toBeGreaterThanOrEqual(6);
    for (const decl of decls) {
      expect(decl.trim()).toMatch(
        /^--color-[a-z0-9-]+\s*:\s*var\(--[a-z0-9-]+\)\s*;$/,
      );
    }
  });

  it("maps the locked role tokens onto existing custom properties", () => {
    const block = stylesCss.match(/@theme\s+inline\s*\{([\s\S]*?)\n\}/)![1];
    const required: Array<[string, string]> = [
      ["color-paper", "paper"],
      ["color-ink", "ink"],
      ["color-accent", "accent"],
      ["color-warn", "warn"],
      ["color-danger", "danger"],
      ["color-ok", "ok"],
    ];
    for (const [name, ref] of required) {
      const re = new RegExp(`--${name}\\s*:\\s*var\\(--${ref}\\)\\s*;`);
      expect(block, `--color-${name} → var(--${ref}) missing`).toMatch(re);
    }
  });
});
