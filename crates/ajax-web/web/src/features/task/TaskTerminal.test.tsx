import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import taskTerminalSource from "./TaskTerminal.tsx?raw";

const stylesSource = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "../../styles.css"),
  "utf8",
);

function extractBlock(source: string, startPattern: RegExp, endPattern: RegExp): string {
  const start = source.search(startPattern);
  if (start < 0) return "";
  const from = source.slice(start);
  const end = from.search(endPattern);
  return end < 0 ? from : from.slice(0, end);
}

function terminalMobileBlock(): string {
  const tail = taskTerminalStylesSection();
  const match = tail.match(
    /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\)\s*\{([\s\S]*)\n\}\s*$/,
  );
  return match?.[1] ?? "";
}

function taskTerminalStylesSection(): string {
  const start = stylesSource.indexOf("/* TaskTerminal");
  const end = stylesSource.indexOf("/* TAILWIND THEME");
  if (start < 0 || end <= start) return "";
  return stylesSource.slice(start, end);
}

describe("TaskTerminal iOS keyboard geometry", () => {
  it("anchors the xterm helper textarea to the host bottom for iOS keyboard placement", () => {
    const textareaCss =
      stylesSource.match(
        /\.terminal-host\s+textarea\.xterm-helper-textarea\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(textareaCss).toMatch(/bottom:\s*0/);
    expect(textareaCss).toMatch(/left:\s*0/);
    expect(textareaCss).not.toMatch(/left:\s*-9999/);
    expect(taskTerminalSource).toMatch(/style\.bottom\s*=\s*["']0["']/);
  });

  it("softens textarea clip/opacity so iOS treats it as an edit target", () => {
    const textareaCss =
      stylesSource.match(
        /\.terminal-host\s+textarea\.xterm-helper-textarea\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(textareaCss).toMatch(/opacity:\s*0\.01/);
    expect(textareaCss).toMatch(/clip-path:\s*none/);
    expect(taskTerminalSource).toMatch(/opacity\s*=\s*["']0\.01["']/);
    expect(taskTerminalSource).toMatch(/clip-path["'],\s*["']none["']/);
  });

  it("resets document scroll before focusing the terminal textarea", () => {
    // Path-agnostic by design: this import has been spelled "../viewport",
    // "@/viewport" and now "@/shared/lib/viewport" across slices 9's rounds. What
    // matters is that resetDocumentScroll comes from the viewport module.
    expect(taskTerminalSource).toMatch(
      /import\s*\{[^}]*resetDocumentScroll[^}]*\}\s*from\s*["'][^"']*\/viewport["']/,
    );

    const onInteractionClick = taskTerminalSource.match(
      /const onInteractionClick\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {4}\};/,
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
        /const scheduleBandSettle\s*=\s*\(\)\s*=>\s*\{([\s\S]*?)\n {2}\};/,
      )?.[1] ?? "";

    expect(settleBody).toMatch(/cancelExpandSettle\s*\(\s*\)/);
    expect(settleBody).toMatch(/requestAnimationFrame[\s\S]*?requestAnimationFrame/);
    expect(settleBody).toMatch(
      /setTimeout\([\s\S]*?schedulePostLayoutRef(?:\.current)?\?\.\(true\)[\s\S]*?EXPAND_REWRAP_MS/,
    );
    const discreteCalls = settleBody.match(/schedulePostLayoutRef(?:\.current)?\?\.\(true\)/g) ?? [];
    expect(discreteCalls).toHaveLength(4);
    expect(settleBody).not.toMatch(/schedulePostLayoutRef(?:\.current)?\?\.\(false\)/);
    expect(settleBody).not.toMatch(/schedulePostLayoutRef(?:\.current)?\?\.\(\s*\)/);
  });

  it("pins bottom controls so hotkeys stay above the keyboard band", () => {
    const mobileBlock = terminalMobileBlock();

    expect(mobileBlock).toMatch(
      /html\.keyboard-open[\s\S]*?terminal-bottom-controls[\s\S]*?flex:\s*none/,
    );
  });

  it("flex-fills the mobile inline terminal so the details line sits at the page bottom", () => {
    const mobileBlock = terminalMobileBlock();

    const inlineWrapRule =
      mobileBlock.match(
        /\n {2}\.terminal-panel:not\(\.is-expanded\)\s+\.terminal-interaction-wrap\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(inlineWrapRule).toMatch(/flex:\s*1\s+1\s+0%/);
    expect(inlineWrapRule).toMatch(/height:\s*auto/);
    expect(inlineWrapRule).not.toMatch(/height:\s*min\(/);
    expect(mobileBlock).toMatch(
      /\.terminal-panel:not\(\.is-expanded\)\s+\.terminal-host[\s\S]*?height:\s*100%/,
    );
    expect(mobileBlock).toMatch(
      /html\.keyboard-open\s+\.terminal-panel:not\(\.is-expanded\)\s+\.terminal-interaction-wrap[\s\S]*?flex:\s*1\s+1\s+0%/,
    );

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

  it("skips fits while a terminal selection is active", () => {
    const scheduleFitBody =
      taskTerminalSource.match(
        /const scheduleFit\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {4}\};/,
      )?.[1] ?? "";

    // discreteIntent must not bypass: open-path scheduleImmediate(true) can
    // land a late rAF after selection and otherwise unmount Copy under the tap.
    expect(scheduleFitBody).toMatch(
      /\(term(?:Ref\.current)?\?\.getSelection\(\)\s*\?\?\s*["']['"]\)\.length\s*>\s*0/,
    );
    expect(scheduleFitBody).not.toMatch(
      /!discreteIntent\s*&&\s*\(term(?:Ref\.current)?\?\.getSelection\(\)/,
    );
  });

  it("distributes hotbar keys proportionally and drops safe-area pad when keyboard is open", () => {
    const mobileBlock = terminalMobileBlock();

    expect(mobileBlock).toMatch(/\.terminal-keys\s*\{[^}]*width:\s*100%/);
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?flex:\s*1\s+1\s+0/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?width:\s*0/,
    );
    expect(mobileBlock).toMatch(/\.terminal-keys\s*\{[^}]*padding:\s*4px\s+6px\s+2px/);
    expect(mobileBlock).not.toMatch(
      /\n {2}\.terminal-keys\s*\{[^}]*env\(safe-area-inset-bottom\)/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-panel\.is-expanded\s+\.terminal-keys\s*\{[^}]*padding-bottom:\s*max\(2px,\s*env\(safe-area-inset-bottom\)\)/,
    );
    expect(mobileBlock).toMatch(
      /html\.keyboard-open\s+\.terminal-keys\s*\{[^}]*padding-bottom:\s*6px/,
    );
  });

  it("tunes mobile hotbar key chrome for iOS WebKit", () => {
    const mobileBlock = terminalMobileBlock();

    expect(mobileBlock).toMatch(/\.terminal-keys\s*\{[^}]*gap:\s*4px/);
    expect(mobileBlock).toMatch(/\.terminal-keys\s*\{[^}]*padding:\s*4px\s+6px/);
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?min-height:\s*36px/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?padding:\s*2px\s+1px/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?font-size:\s*var\(--text-label\)/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?font-family:\s*var\(--sans\)/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?-webkit-text-size-adjust:\s*100%/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?overflow:\s*hidden/,
    );
    expect(mobileBlock).toMatch(
      /\.terminal-keys\s+\.terminal-key[\s\S]*?white-space:\s*nowrap/,
    );
  });

  it("omits the hotbar Hide keyboard control", () => {
    expect(taskTerminalSource).not.toMatch(/aria-label="Hide keyboard"/);
    expect(taskTerminalSource).not.toMatch(
      /className="terminal-key"[\s\S]*?⌄/,
    );
    expect(taskTerminalSource).toMatch(/aria-label="Expand terminal"/);
  });

  it("settles the band on any keyboard-open class edge (inline or fullscreen)", () => {
    const observerBody = extractBlock(
      taskTerminalSource,
      /const keyboardClassObserver\s*=\s*new MutationObserver/,
      /\n {4}keyboardClassObserver\.observe/,
    );

    expect(observerBody).toMatch(/MutationObserver/);
    expect(observerBody).toMatch(/nowOpen\s*===\s*wasKeyboardOpen/);
    expect(observerBody).toMatch(/resetDocumentScroll\s*\(\s*\)/);
    // Either spelling: call sites inside the mount effect go through the
    // onBandSettle effect event (slice 10), which delegates to scheduleBandSettle.
    expect(observerBody).toMatch(/(?:schedule|on)BandSettle\s*\(\s*\)/);
    expect(observerBody).not.toMatch(/EXPANDED_CLASS/);
    expect(observerBody).not.toMatch(/nowOpen\s*&&\s*!wasKeyboardOpen/);
    expect(taskTerminalSource).toMatch(
      /keyboardClassObserver\.observe\(\s*document\.documentElement[\s\S]*?attributeFilter:\s*\[["']class["']\]/,
    );
  });

  it("settles the band on expand enter, expand exit, and tap-focus", () => {
    const toggleBody =
      taskTerminalSource.match(/const toggleExpanded\s*=\s*\(\)\s*=>\s*\{([\s\S]*?)\n {2}\};/)?.[1] ??
      "";

    expect(toggleBody).toMatch(/if\s*\(\s*!entering\s*\)\s*\{[\s\S]*?scheduleBandSettle\s*\(\s*\)[\s\S]*?return/);
    expect(toggleBody).toMatch(/scheduleBandSettle\s*\(\s*\)\s*;\s*$/);
    expect(toggleBody.match(/scheduleBandSettle\s*\(\s*\)/g)?.length).toBe(2);
    expect(toggleBody).not.toMatch(/schedulePostLayoutRef(?:\.current)?\?\.\(false\)/);
    expect(taskTerminalSource).not.toMatch(/schedulePostLayoutRef(?:\.current)?\?\.\(false\)/);

    const onInteractionClick =
      taskTerminalSource.match(
        /const onInteractionClick\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {4}\};/,
      )?.[1] ?? "";
    expect(onInteractionClick).toMatch(/(?:schedule|on)BandSettle\s*\(\s*\)/);
    expect(onInteractionClick).not.toMatch(/EXPANDED_CLASS/);
    expect(onInteractionClick).not.toMatch(/terminal-expanded/);
  });

  it("pins expanded panel with top and height to the live visual-viewport band", () => {
    const expandedRule =
      taskTerminalStylesSection().match(
        /html\.terminal-expanded\s+\.terminal-panel\.is-expanded\s*\{([\s\S]*?)\n {2}\}/,
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
      /className="terminal-corner-actions"/,
      /<\/div>\s*<div\s+className="terminal-status"/,
    );

    expect(cornerMarkup).toMatch(/data-testid="terminal-copy-overlay"/);
    expect(cornerMarkup).toMatch(/terminal-copy-overlay[\s\S]*?terminal-expand-corner/);
    expect(cornerMarkup.indexOf("terminal-copy-overlay")).toBeLessThan(
      cornerMarkup.indexOf("terminal-expand-corner"),
    );

    const interactionOpen = taskTerminalSource.indexOf('className="terminal-interaction-wrap"');
    const interactionClose = taskTerminalSource.indexOf("{copyNotice ?");
    expect(interactionOpen).toBeGreaterThan(-1);
    expect(interactionClose).toBeGreaterThan(interactionOpen);
    const interactionMarkup = taskTerminalSource.slice(interactionOpen, interactionClose);
    expect(interactionMarkup).not.toMatch(/terminal-copy-overlay/);
    expect(interactionMarkup).not.toMatch(/terminal-expand-corner/);
    expect(interactionMarkup).not.toMatch(/copyNotice/);

    const cornerCss =
      stylesSource.match(/\.terminal-corner-actions\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(cornerCss).toMatch(/position:\s*absolute/);
    expect(cornerCss).toMatch(/top:\s*6px/);
    expect(cornerCss).toMatch(/right:\s*6px/);
    expect(cornerCss).toMatch(/display:\s*flex/);
    expect(cornerCss).toMatch(/z-index:\s*8/);

    const overlayCss =
      stylesSource.match(/\.terminal-copy-overlay\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(overlayCss).not.toMatch(/position:\s*absolute/);
    expect(overlayCss).not.toMatch(/left:\s*50%/);
    expect(overlayCss).not.toMatch(/top:\s*50%/);
    expect(overlayCss).toMatch(/min-width:\s*44px/);
    expect(overlayCss).toMatch(/min-height:\s*44px/);
  });

  it("enables scroll-on-erase so attach ED2 pushes seeded viewport into scrollback", () => {
    expect(taskTerminalSource).toMatch(/scrollOnEraseInDisplay:\s*true/);
  });

  it("names terminal control keys for assistive tech", () => {
    expect(taskTerminalSource).toMatch(/ariaLabel:\s*"Escape"/);
    expect(taskTerminalSource).toMatch(/ariaLabel:\s*"Control C"/);
    expect(taskTerminalSource).toMatch(/aria-label=\{key\.ariaLabel\}/);
    expect(taskTerminalSource).toMatch(/aria-label="Control modifier"/);
    expect(taskTerminalSource).toMatch(/aria-label="Paste"/);
  });

  it("includes Backspace in CONTROL_KEYS with DEL payload", () => {
    const controlKeysBlock =
      taskTerminalSource.match(/const CONTROL_KEYS\s*=\s*\[([\s\S]*?)\];/)?.[1] ?? "";
    expect(controlKeysBlock).toMatch(/ariaLabel:\s*"Backspace"/);
    expect(controlKeysBlock).toMatch(/data:\s*"\\x7f"/);
  });

  it("marks Backspace and arrows as repeatable hotbar keys only", () => {
    expect(taskTerminalSource).toMatch(/REPEATABLE_KEY_DATA|isRepeatableKey/);
    const repeatableBlock =
      taskTerminalSource.match(
        /(?:REPEATABLE_KEY_DATA|repeatableKeyData)\s*=\s*(?:new Set\(\[|Set\(\[)([\s\S]*?)\]\)/,
      )?.[1] ?? "";
    expect(repeatableBlock).toMatch(/\\x7f/);
    expect(repeatableBlock).toMatch(/\\x1b\[D/);
    expect(repeatableBlock).toMatch(/\\x1b\[A/);
    expect(repeatableBlock).toMatch(/\\x1b\[B/);
    expect(repeatableBlock).toMatch(/\\x1b\[C/);
    expect(repeatableBlock).not.toMatch(/\\x1b"/);
    expect(repeatableBlock).not.toMatch(/\\t/);
    expect(repeatableBlock).not.toMatch(/Paste/);
  });
});
