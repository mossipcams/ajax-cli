import { describe, it, expect } from "vitest";
import taskTerminalSource from "./TaskTerminal.svelte?raw";

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
    expect(taskTerminalSource).toMatch(/schedulePostLayoutRef\?\.\(true\)/);
    expect(taskTerminalSource).toMatch(
      /setTimeout\([\s\S]*?schedulePostLayoutRef\?\.\(true\)[\s\S]*?EXPAND_REWRAP_MS/,
    );
    expect(taskTerminalSource).toMatch(/requestAnimationFrame[\s\S]*?requestAnimationFrame/);
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

  it("settles the band on any keyboard-open class edge (inline or fullscreen)", () => {
    expect(taskTerminalSource).toMatch(/MutationObserver/);
    expect(taskTerminalSource).toMatch(/const scheduleBandSettle\s*=/);
    expect(taskTerminalSource).toMatch(
      /nowOpen\s*===\s*wasKeyboardOpen[\s\S]*?scheduleBandSettle\(\)/,
    );
    expect(taskTerminalSource).not.toMatch(
      /nowOpen\s*&&\s*!wasKeyboardOpen[\s\S]*?EXPANDED_CLASS/,
    );
  });

  it("settles the band on expand enter, expand exit, and tap-focus", () => {
    expect(taskTerminalSource).toMatch(
      /const toggleExpanded\s*=\s*\(\)\s*=>\s*\{[\s\S]*?scheduleBandSettle\(\)[\s\S]*?scheduleBandSettle\(\)/,
    );
    expect(taskTerminalSource).not.toMatch(
      /schedulePostLayoutRef\?\.\(false\)/,
    );
    const onInteractionClick =
      taskTerminalSource.match(
        /const onInteractionClick\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n    \};/,
      )?.[1] ?? "";
    expect(onInteractionClick).toMatch(/scheduleBandSettle\(\)/);
    expect(onInteractionClick).not.toMatch(/EXPANDED_CLASS/);
  });

  it("pins expanded panel with top and bottom to the live visual-viewport band", () => {
    const expandedRule =
      taskTerminalSource.match(
        /:global\(html\.terminal-expanded\)\s+\.terminal-panel\.is-expanded\s*\{([\s\S]*?)\n    \}/,
      )?.[1] ?? "";

    expect(expandedRule).toMatch(/top:\s*var\(--app-top/);
    expect(expandedRule).toMatch(/bottom:\s*max\([\s\S]*?calc\(/);
    expect(expandedRule).toMatch(/height:\s*auto/);
  });

  it("shows Copy beside expand on the panel, not centered in the scroll wrap", () => {
    expect(taskTerminalSource).toMatch(/class="terminal-corner-actions"/);
    expect(taskTerminalSource).toMatch(/data-testid="terminal-copy-overlay"/);
    expect(taskTerminalSource).toMatch(
      /terminal-corner-actions[\s\S]*?terminal-copy-overlay[\s\S]*?terminal-expand-corner/,
    );

    const interactionWrapMarkup =
      taskTerminalSource.match(
        /class="terminal-interaction-wrap"[\s\S]*?<\/div>\s*\{#if copyNotice\}/,
      )?.[0] ?? "";
    expect(interactionWrapMarkup).not.toMatch(/terminal-copy-overlay/);

    const cornerCss =
      taskTerminalSource.match(/\.terminal-corner-actions\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(cornerCss).toMatch(/position:\s*absolute/);
    expect(cornerCss).toMatch(/top:\s*6px/);
    expect(cornerCss).toMatch(/right:\s*6px/);
    expect(cornerCss).toMatch(/display:\s*flex/);

    const overlayCss =
      taskTerminalSource.match(/\.terminal-copy-overlay\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(overlayCss).not.toMatch(/left:\s*50%/);
    expect(overlayCss).not.toMatch(/top:\s*50%/);
  });
});
