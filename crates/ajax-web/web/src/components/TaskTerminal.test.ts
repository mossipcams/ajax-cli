import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import taskTerminalSource from "./TaskTerminal.svelte?raw";

const stylesSource = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "../styles.css"),
  "utf8",
);

function extractBlock(source: string, startPattern: RegExp, endPattern: RegExp): string {
  const start = source.search(startPattern);
  if (start < 0) return "";
  const from = source.slice(start);
  const end = from.search(endPattern);
  return end < 0 ? from : from.slice(0, end);
}

describe("TaskTerminal iOS keyboard geometry", () => {
  it("anchors the xterm helper textarea to the host bottom for iOS keyboard placement", () => {
    const textareaCss =
      taskTerminalSource.match(
        /\.terminal-host\s+:global\(textarea\.xterm-helper-textarea\)\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(textareaCss).toMatch(/bottom:\s*0/);
    expect(textareaCss).toMatch(/left:\s*0/);
    expect(textareaCss).not.toMatch(/left:\s*-9999/);
    expect(taskTerminalSource).toMatch(/style\.bottom\s*=\s*["']0["']/);
  });

  it("softens textarea clip/opacity so iOS treats it as an edit target", () => {
    const textareaCss =
      taskTerminalSource.match(
        /\.terminal-host\s+:global\(textarea\.xterm-helper-textarea\)\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(textareaCss).toMatch(/opacity:\s*0\.01/);
    expect(textareaCss).toMatch(/clip-path:\s*none/);
    expect(taskTerminalSource).toMatch(/opacity\s*=\s*["']0\.01["']/);
    expect(taskTerminalSource).toMatch(/clip-path["'],\s*["']none["']/);
  });

  it("resets document scroll before focusing the terminal textarea", () => {
    expect(taskTerminalSource).toMatch(/import\s*\{[^}]*resetDocumentScroll[^}]*\}\s*from\s*["']\.\.\/viewport["']/);

    const onInteractionClick = taskTerminalSource.match(
      /const onInteractionClick\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n    \};/,
    )?.[1] ?? "";

    expect(onInteractionClick).toMatch(/resetDocumentScroll\s*\(\s*\)/);
    expect(onInteractionClick).toMatch(/focus\(\{\s*preventScroll:\s*true\s*\}\)/);
    expect(onInteractionClick.indexOf("resetDocumentScroll")).toBeLessThan(
      onInteractionClick.indexOf("focus({ preventScroll: true })"),
    );
  });

  it("re-fits through the expand settle window with discrete intent", () => {
    expect(taskTerminalSource).toMatch(/const EXPAND_REWRAP_MS\s*=\s*280/);
    const settleBody =
      taskTerminalSource.match(
        /const scheduleBandSettle\s*=\s*\(\)\s*=>\s*\{([\s\S]*?)\n  \};/,
      )?.[1] ?? "";

    expect(settleBody).toMatch(/cancelExpandSettle\s*\(\s*\)/);
    expect(settleBody).toMatch(/requestAnimationFrame[\s\S]*?requestAnimationFrame/);
    expect(settleBody).toMatch(
      /setTimeout\([\s\S]*?schedulePostLayoutRef\?\.\(true\)[\s\S]*?EXPAND_REWRAP_MS/,
    );
    const discreteCalls = settleBody.match(/schedulePostLayoutRef\?\.\(true\)/g) ?? [];
    expect(discreteCalls).toHaveLength(4);
    expect(settleBody).not.toMatch(/schedulePostLayoutRef\?\.\(false\)/);
    expect(settleBody).not.toMatch(/schedulePostLayoutRef\?\.\(\s*\)/);
  });

  it("pins bottom controls so hotkeys stay above the keyboard band", () => {
    const mobileBlock =
      taskTerminalSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n  \}/,
      )?.[1] ?? "";

    expect(mobileBlock).toMatch(
      /:global\(html\.keyboard-open\)[\s\S]*?terminal-bottom-controls[\s\S]*?flex:\s*none/,
    );
  });

  it("flex-fills the mobile inline terminal so the details line sits at the page bottom", () => {
    const mobileBlock =
      taskTerminalSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n  \}/,
      )?.[1] ?? "";

    const inlineWrapRule =
      mobileBlock.match(
        /\n    \.terminal-panel:not\(\.is-expanded\)\s+\.terminal-interaction-wrap\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(inlineWrapRule).toMatch(/flex:\s*1\s+1\s+0%/);
    expect(inlineWrapRule).toMatch(/height:\s*auto/);
    // No fixed cap: the inline terminal fills the task column down to the
    // details line, same flex model as keyboard-open (no relayout jump).
    expect(inlineWrapRule).not.toMatch(/height:\s*min\(/);
    expect(mobileBlock).toMatch(
      /\.terminal-panel:not\(\.is-expanded\)\s+\.terminal-host[\s\S]*?height:\s*100%/,
    );
    expect(mobileBlock).toMatch(
      /:global\(html\.keyboard-open\)\s+\.terminal-panel:not\(\.is-expanded\)\s+\.terminal-interaction-wrap[\s\S]*?flex:\s*1\s+1\s+0%/,
    );

    // The task column must hand the leftover height to the inline panel.
    const stylesMobileBlock =
      stylesSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n\}/,
      )?.[1] ?? "";
    expect(stylesMobileBlock).toMatch(
      /\[data-testid="route-scroll"\]:has\(\[data-outlet="task"\]\) \.task-detail \.terminal-panel:not\(\.is-expanded\)\s*\{[^}]*flex:\s*1\s+1\s+0%/,
    );

    expect(taskTerminalSource).toMatch(/const syncHostToWrap\s*=\s*\(\)\s*=>/);
    expect(taskTerminalSource).toMatch(
      /classList\.contains\(["']keyboard-open["']\)/,
    );
    expect(taskTerminalSource).toMatch(/hostEl\.style\.height\s*=\s*next/);
    expect(taskTerminalSource).toMatch(/syncHostToWrap\(\)/);
  });

  it("skips ambient fits while a terminal selection is active", () => {
    const scheduleFitBody =
      taskTerminalSource.match(
        /const scheduleFit\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n    \};/,
      )?.[1] ?? "";

    expect(scheduleFitBody).toMatch(
      /!discreteIntent\s*&&\s*\(term\?\.getSelection\(\)\s*\?\?\s*["']['"]\)\.length\s*>\s*0/,
    );
  });

  it("distributes hotbar keys proportionally and drops safe-area pad when keyboard is open", () => {
    const mobileBlock =
      taskTerminalSource.match(
        /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*?)\n  \}/,
      )?.[1] ?? "";

    expect(mobileBlock).toMatch(/\.terminal-keys\s*\{[^}]*width:\s*100%/);
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?flex:\s*1\s+1\s+0/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?width:\s*0/,
    );
    // Inline hotbar sits mid-page (details line + nav below it), so it gets no
    // safe-area pad; only the fullscreen hotbar touches the screen edge.
    expect(mobileBlock).toMatch(/\.terminal-keys\s*\{[^}]*padding-bottom:\s*2px/);
    expect(mobileBlock).not.toMatch(
      /\n    \.terminal-keys\s*\{[^}]*env\(safe-area-inset-bottom\)/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-panel\.is-expanded\s+\.terminal-keys\s*\{[^}]*padding-bottom:\s*max\(2px,\s*env\(safe-area-inset-bottom\)\)/,
    );
    expect(mobileBlock).toMatch(
      /:global\(html\.keyboard-open\)\s+\.terminal-keys\s*\{[^}]*padding-bottom:\s*6px/,
    );
  });

  it("settles the band on any keyboard-open class edge (inline or fullscreen)", () => {
    const observerBody = extractBlock(
      taskTerminalSource,
      /const keyboardClassObserver\s*=\s*new MutationObserver/,
      /\n    keyboardClassObserver\.observe/,
    );

    expect(observerBody).toMatch(/MutationObserver/);
    expect(observerBody).toMatch(/nowOpen\s*===\s*wasKeyboardOpen/);
    expect(observerBody).toMatch(/resetDocumentScroll\s*\(\s*\)/);
    expect(observerBody).toMatch(/scheduleBandSettle\s*\(\s*\)/);
    expect(observerBody).not.toMatch(/EXPANDED_CLASS/);
    expect(observerBody).not.toMatch(/nowOpen\s*&&\s*!wasKeyboardOpen/);
    expect(taskTerminalSource).toMatch(
      /keyboardClassObserver\.observe\(\s*document\.documentElement[\s\S]*?attributeFilter:\s*\[["']class["']\]/,
    );
  });

  it("settles the band on expand enter, expand exit, and tap-focus", () => {
    const toggleBody =
      taskTerminalSource.match(/const toggleExpanded\s*=\s*\(\)\s*=>\s*\{([\s\S]*?)\n  \};/)?.[1] ??
      "";

    expect(toggleBody).toMatch(/if\s*\(\s*!entering\s*\)\s*\{[\s\S]*?scheduleBandSettle\s*\(\s*\)[\s\S]*?return/);
    expect(toggleBody).toMatch(/scheduleBandSettle\s*\(\s*\)\s*;\s*$/);
    expect(toggleBody.match(/scheduleBandSettle\s*\(\s*\)/g)?.length).toBe(2);
    expect(toggleBody).not.toMatch(/schedulePostLayoutRef\?\.\(false\)/);
    expect(taskTerminalSource).not.toMatch(/schedulePostLayoutRef\?\.\(false\)/);

    const onInteractionClick =
      taskTerminalSource.match(
        /const onInteractionClick\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n    \};/,
      )?.[1] ?? "";
    expect(onInteractionClick).toMatch(/scheduleBandSettle\s*\(\s*\)/);
    expect(onInteractionClick).not.toMatch(/EXPANDED_CLASS/);
    expect(onInteractionClick).not.toMatch(/terminal-expanded/);
  });

  it("pins expanded panel with top and height to the live visual-viewport band", () => {
    const expandedRule =
      taskTerminalSource.match(
        /:global\(html\.terminal-expanded\)\s+\.terminal-panel\.is-expanded\s*\{([\s\S]*?)\n    \}/,
      )?.[1] ?? "";

    expect(expandedRule).toMatch(/top:\s*var\(--app-top,\s*var\(--app-band-top,\s*0px\)\)/);
    expect(expandedRule).toMatch(
      /height:\s*var\(--app-height,\s*var\(--app-band-height/,
    );
    expect(expandedRule).toMatch(
      /max-height:\s*var\(--app-height,\s*var\(--app-band-height/,
    );
    expect(expandedRule).not.toMatch(/bottom:\s*max/);
  });

  it("shows Copy beside expand on the panel, not centered in the scroll wrap", () => {
    const cornerMarkup = extractBlock(
      taskTerminalSource,
      /class="terminal-corner-actions"/,
      /<\/div>\s*<div\s+class="terminal-status"/,
    );

    expect(cornerMarkup).toMatch(/data-testid="terminal-copy-overlay"/);
    expect(cornerMarkup).toMatch(/terminal-copy-overlay[\s\S]*?terminal-expand-corner/);
    expect(cornerMarkup.indexOf("terminal-copy-overlay")).toBeLessThan(
      cornerMarkup.indexOf("terminal-expand-corner"),
    );

    const interactionOpen = taskTerminalSource.indexOf('class="terminal-interaction-wrap"');
    const interactionClose = taskTerminalSource.indexOf("{#if copyNotice}");
    expect(interactionOpen).toBeGreaterThan(-1);
    expect(interactionClose).toBeGreaterThan(interactionOpen);
    const interactionMarkup = taskTerminalSource.slice(interactionOpen, interactionClose);
    expect(interactionMarkup).not.toMatch(/terminal-copy-overlay/);
    expect(interactionMarkup).not.toMatch(/terminal-expand-corner/);
    expect(interactionMarkup).not.toMatch(/copyNotice/);

    const cornerCss =
      taskTerminalSource.match(/\.terminal-corner-actions\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(cornerCss).toMatch(/position:\s*absolute/);
    expect(cornerCss).toMatch(/top:\s*6px/);
    expect(cornerCss).toMatch(/right:\s*6px/);
    expect(cornerCss).toMatch(/display:\s*flex/);
    expect(cornerCss).toMatch(/z-index:\s*8/);

    const overlayCss =
      taskTerminalSource.match(/\.terminal-copy-overlay\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(overlayCss).not.toMatch(/position:\s*absolute/);
    expect(overlayCss).not.toMatch(/left:\s*50%/);
    expect(overlayCss).not.toMatch(/top:\s*50%/);
    expect(overlayCss).toMatch(/min-width:\s*44px/);
    expect(overlayCss).toMatch(/min-height:\s*44px/);
  });

  it("names terminal control keys for assistive tech", () => {
    expect(taskTerminalSource).toMatch(/ariaLabel:\s*"Escape"/);
    expect(taskTerminalSource).toMatch(/ariaLabel:\s*"Control C"/);
    expect(taskTerminalSource).toMatch(/aria-label=\{key\.ariaLabel\}/);
    expect(taskTerminalSource).toMatch(/aria-label="Control modifier"/);
    expect(taskTerminalSource).toMatch(/aria-label="Paste"/);
  });
});
