import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import taskTerminalSource from "./TaskTerminal.tsx?raw";
import mountTaskTerminalSessionSource from "./mountTaskTerminalSession.ts?raw";
import useTaskTerminalSpeechSource from "./useTaskTerminalSpeech.ts?raw";

/** Shell + peeled mount/speech modules for source-contract asserts. */
const taskTerminalFeatureSource =
  `${taskTerminalSource}\n${mountTaskTerminalSessionSource}\n${useTaskTerminalSpeechSource}`;

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

describe("TaskTerminal link menu", () => {
  it("blurs the xterm textarea when opening the link menu while the keyboard is closed", () => {
    const onInteractionClick =
      taskTerminalFeatureSource.match(
        /const onInteractionClick\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {2,4}\};/,
      )?.[1] ?? "";
    const onLinkActivate =
      taskTerminalFeatureSource.match(
        /onLinkActivate:\s*\(\{[^}]*\}\)\s*=>\s*\{([\s\S]*?)\n {6}\},/,
      )?.[1] ?? "";

    const blurWhenKeyboardClosed =
      /if\s*\(\s*!isKeyboardOpen\(\)\s*\)\s*termTextarea\(\)\?\.blur\(\)/;

    expect(onInteractionClick).toMatch(/setLinkMenu/);
    expect(onInteractionClick).toMatch(blurWhenKeyboardClosed);
    expect(onLinkActivate).toMatch(/setLinkMenu/);
    expect(onLinkActivate).toMatch(blurWhenKeyboardClosed);
  });
});

describe("TaskTerminal iOS keyboard geometry", () => {
  it("anchors the xterm helper textarea to the host bottom for iOS keyboard placement", () => {
    const textareaCss =
      stylesSource.match(
        /\.terminal-host\s+textarea\.xterm-helper-textarea\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(textareaCss).toMatch(/bottom:\s*0/);
    expect(textareaCss).toMatch(/left:\s*0/);
    expect(textareaCss).not.toMatch(/left:\s*-9999/);
    expect(taskTerminalFeatureSource).toMatch(/style\.bottom\s*=\s*["']0["']/);
  });

  it("softens textarea clip/opacity so iOS treats it as an edit target", () => {
    const textareaCss =
      stylesSource.match(
        /\.terminal-host\s+textarea\.xterm-helper-textarea\s*\{([^}]*)\}/,
      )?.[1] ?? "";

    expect(textareaCss).toMatch(/opacity:\s*0\.01/);
    expect(textareaCss).toMatch(/clip-path:\s*none/);
    expect(taskTerminalFeatureSource).toMatch(/opacity\s*=\s*["']0\.01["']/);
    expect(taskTerminalFeatureSource).toMatch(/clip-path["'],\s*["']none["']/);
  });

  it("resets document scroll before focusing the terminal textarea", () => {
    // Path-agnostic by design: this import has been spelled "../viewport",
    // "@/viewport" and now "@/shared/lib/viewport" across slices 9's rounds. What
    // matters is that resetDocumentScroll comes from the viewport module.
    expect(taskTerminalFeatureSource).toMatch(
      /import\s*\{[^}]*resetDocumentScroll[^}]*\}\s*from\s*["'][^"']*\/viewport["']/,
    );

    const onInteractionClick = taskTerminalFeatureSource.match(
      /const onInteractionClick\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {2,4}\};/,
    )?.[1] ?? "";

    expect(onInteractionClick).toMatch(/resetDocumentScroll\s*\(\s*\)/);
    expect(onInteractionClick).toMatch(/focus\(\{\s*preventScroll:\s*true\s*\}\)/);
    expect(onInteractionClick.indexOf("resetDocumentScroll")).toBeLessThan(
      onInteractionClick.indexOf("focus({ preventScroll: true })"),
    );
  });

  it("re-fits through the expand settle window with discrete intent", () => {
    expect(taskTerminalFeatureSource).toMatch(/const EXPAND_REWRAP_MS\s*=\s*280/);
    const settleBody =
      taskTerminalFeatureSource.match(
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

    expect(taskTerminalFeatureSource).toMatch(/const syncHostToWrap\s*=\s*\(\)\s*=>/);
    expect(taskTerminalFeatureSource).toMatch(
      /classList\.contains\(["']keyboard-open["']\)/,
    );
    expect(taskTerminalFeatureSource).toMatch(/hostEl\.style\.height\s*=\s*next/);
    expect(taskTerminalFeatureSource).toMatch(/syncHostToWrap\(\)/);
  });

  it("skips fits while a terminal selection is active", () => {
    const scheduleFitBody =
      taskTerminalFeatureSource.match(
        /const scheduleFit\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {2,4}\};/,
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
    expect(taskTerminalFeatureSource).not.toMatch(/aria-label="Hide keyboard"/);
    expect(taskTerminalFeatureSource).not.toMatch(
      /className="terminal-key"[\s\S]*?⌄/,
    );
    expect(taskTerminalFeatureSource).toMatch(/aria-label="Expand terminal"/);
  });

  it("settles the band on any keyboard-open class edge (inline or fullscreen)", () => {
    const observerBody = extractBlock(
      taskTerminalFeatureSource,
      /const keyboardClassObserver\s*=\s*new MutationObserver/,
      /\n {2,4}keyboardClassObserver\.observe/,
    );

    expect(observerBody).toMatch(/MutationObserver/);
    expect(observerBody).toMatch(/nowOpen\s*===\s*wasKeyboardOpen/);
    expect(observerBody).toMatch(/resetDocumentScroll\s*\(\s*\)/);
    // Either spelling: call sites inside the mount effect go through the
    // onBandSettle effect event (slice 10), which delegates to scheduleBandSettle.
    expect(observerBody).toMatch(/(?:schedule|on)BandSettle\s*\(\s*\)/);
    expect(observerBody).not.toMatch(/EXPANDED_CLASS/);
    expect(observerBody).not.toMatch(/nowOpen\s*&&\s*!wasKeyboardOpen/);
    expect(taskTerminalFeatureSource).toMatch(
      /keyboardClassObserver\.observe\(\s*document\.documentElement[\s\S]*?attributeFilter:\s*\[["']class["']\]/,
    );
  });

  it("settles the band on expand enter, expand exit, and tap-focus", () => {
    const toggleBody =
      taskTerminalFeatureSource.match(/const toggleExpanded\s*=\s*\(\)\s*=>\s*\{([\s\S]*?)\n {2}\};/)?.[1] ??
      "";

    expect(toggleBody).toMatch(/if\s*\(\s*!entering\s*\)\s*\{[\s\S]*?scheduleBandSettle\s*\(\s*\)[\s\S]*?return/);
    expect(toggleBody).toMatch(/scheduleBandSettle\s*\(\s*\)\s*;\s*$/);
    expect(toggleBody.match(/scheduleBandSettle\s*\(\s*\)/g)?.length).toBe(2);
    expect(toggleBody).not.toMatch(/schedulePostLayoutRef(?:\.current)?\?\.\(false\)/);
    expect(taskTerminalFeatureSource).not.toMatch(/schedulePostLayoutRef(?:\.current)?\?\.\(false\)/);

    const onInteractionClick =
      taskTerminalFeatureSource.match(
        /const onInteractionClick\s*=\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {2,4}\};/,
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
      taskTerminalFeatureSource,
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
    expect(taskTerminalFeatureSource).toMatch(/scrollOnEraseInDisplay:\s*true/);
  });

  it("latches scrollOnErase to the seeded-open window only", () => {
    const mountBody =
      mountTaskTerminalSessionSource.match(
        /export function mountTaskTerminalSession\([\s\S]*?\)\s*:\s*\(\)\s*=>\s*void\s*\{([\s\S]*)\n\}\s*$/,
      )?.[1] ?? "";

    // Reveal must NOT latch off: attach ED2 can still be in flight after the
    // quiet window. Latch on the first post-reveal erase instead.
    const revealBody =
      mountBody.match(/const revealSeed = \(\) => \{([\s\S]*?)\n {2}\};/)?.[1] ?? "";
    expect(revealBody).not.toMatch(/scrollOnEraseInDisplay\s*=\s*false/);

    const onOutputBody =
      mountBody.match(/onOutput:\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {6}\},/)?.[1] ?? "";
    expect(onOutputBody).toMatch(/sawErase/);
    expect(onOutputBody).toMatch(
      /!isSeedPending\(\)[\s\S]*?scrollOnEraseInDisplay\s*=\s*false/,
    );

    const onOpenBody =
      mountBody.match(/onOpen:\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {6}\},/)?.[1] ?? "";
    expect(onOpenBody).toMatch(
      /if\s*\(\s*!seeded\s*\)\s*\{[\s\S]*?scrollOnEraseInDisplay\s*=\s*false/,
    );
    expect(onOpenBody).toMatch(
      /termRef\.current\.reset\(\)[\s\S]*?scrollOnEraseInDisplay\s*=\s*true/,
    );
  });

  it("names terminal control keys for assistive tech", () => {
    expect(taskTerminalFeatureSource).toMatch(/ariaLabel:\s*"Escape"/);
    // Visible ⌃C toolbar entry removed; keyboard Ctrl+C remains via Control modifier.
    expect(taskTerminalFeatureSource).toMatch(/aria-label="Control modifier"/);
    expect(taskTerminalFeatureSource).toMatch(/aria-label=\{key\.ariaLabel\}/);
    expect(taskTerminalFeatureSource).toMatch(/aria-label="Paste"/);
  });

  it("includes Backspace in CONTROL_KEYS with DEL payload", () => {
    const controlKeysBlock =
      taskTerminalFeatureSource.match(/const CONTROL_KEYS\s*=\s*\[([\s\S]*?)\];/)?.[1] ?? "";
    expect(controlKeysBlock).toMatch(/ariaLabel:\s*"Backspace"/);
    expect(controlKeysBlock).toMatch(/data:\s*"\\x7f"/);
  });

  it("marks Backspace and arrows as repeatable hotbar keys only", () => {
    expect(taskTerminalFeatureSource).toMatch(/REPEATABLE_KEY_DATA|isRepeatableKey/);
    const repeatableBlock =
      taskTerminalFeatureSource.match(
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

  it("skips xterm Backspace keydown so iOS can key-repeat", () => {
    const backspaceBranch = extractBlock(
      taskTerminalFeatureSource,
      /liveTerm\.attachCustomKeyEventHandler\(\(event\) => \{/,
      /\n {4,6}if \(event\.key !== " "/,
    );

    expect(backspaceBranch).toMatch(
      /event\.key === "Backspace" \|\| event\.key === "Delete"[\s\S]*?return false/,
    );
    expect(backspaceBranch).not.toMatch(/\.preventDefault\s*\(/);
  });

  it("seeds a zero-width space so iOS has deletable content", () => {
    expect(taskTerminalFeatureSource).toMatch(/const BACKSPACE_SENTINEL\s*=\s*"\\u200B"/);

    const hardenTextarea = extractBlock(
      taskTerminalFeatureSource,
      /const hardenMobileTextarea\s*=\s*\(\)\s*=>\s*\{/,
      /\n {2}\};/,
    );
    expect(hardenTextarea).toMatch(/seedBackspaceSentinel\(input\)/);
    expect(hardenTextarea).toMatch(/addEventListener\("focus",\s*\w+\)/);
  });

  it("sends DEL from beforeinput deleteContentBackward", () => {
    const beforeInput = extractBlock(
      taskTerminalFeatureSource,
      /const onTextareaBeforeInput\s*=\s*\(event:\s*InputEvent\)\s*=>\s*\{/,
      /\n {2}\};/,
    );

    expect(beforeInput).toMatch(/deleteInputPayload\(event\.inputType\)/);
    expect(beforeInput).toMatch(/sendKey\(consumeCtrl\(payload\)\)/);
    expect(beforeInput).not.toMatch(/\.preventDefault\s*\(/);

    // Measured on iOS 26: a held Delete repeats deleteContentBackward at ~100ms
    // and then escalates to deleteWordBackward. Dropping the escalation strands
    // the rest of the hold, which is what "hold backspace does nothing" looked
    // like in the app.
    const payloads = extractBlock(
      taskTerminalFeatureSource,
      /const deleteInputPayload\s*=\s*\(inputType:\s*string\)/,
      /\n {2}\};/,
    );
    expect(payloads).toMatch(/deleteContentBackward[\s\S]*?"\\x7f"/);
    expect(payloads).toMatch(/deleteWordBackward[\s\S]*?"\\x17"/);
  });

  it("blocks xterm empty paste without preventDefault and recovers from textarea input", () => {
    const onPaste = extractBlock(
      taskTerminalFeatureSource,
      /const onTextareaPaste\s*=\s*\(event:\s*ClipboardEvent\)\s*=>\s*\{/,
      /\n {2}\};/,
    );
    expect(onPaste).toMatch(/readPasteText\(event\.clipboardData\)/);
    expect(onPaste).toMatch(/pasteExpectRef\.current\s*=\s*true/);
    expect(onPaste).toMatch(/event\.stopImmediatePropagation\(\)/);
    expect(onPaste).not.toMatch(/navigator\.clipboard\?\.readText/);

    const emptyBranch = onPaste.match(
      /pasteExpectRef\.current\s*=\s*true;[\s\S]*?event\.stopImmediatePropagation\(\);/,
    )?.[0];
    expect(emptyBranch).toBeDefined();
    expect(emptyBranch).not.toMatch(/preventDefault/);

    const onInput = extractBlock(
      taskTerminalFeatureSource,
      /const onTextareaInput\s*=\s*\(event:\s*Event\)\s*=>\s*\{/,
      /\n {2}\};/,
    );
    expect(onInput).toMatch(/insertFromPaste/);
    expect(onInput).toMatch(/pasteExpectRef\.current/);
    expect(onInput).toMatch(/replaceAll\(BACKSPACE_SENTINEL/);
    expect(onInput).toMatch(/textarea\.value\s*=\s*BACKSPACE_SENTINEL/);
    expect(onInput).toMatch(/sendPastedText\(raw\)/);
    expect(onInput).toMatch(/inputType === "insertText"/);
  });

  it("reseeds the sentinel from input, never a beforeinput microtask", () => {
    const onInput = extractBlock(
      taskTerminalFeatureSource,
      /const onTextareaInput\s*=\s*\(event:\s*Event\)\s*=>\s*\{/,
      /\n {2}\};/,
    );
    expect(onInput).toMatch(/startsWith\("delete"\)/);
    expect(onInput).toMatch(/seedTermSentinel\(\)/);

    // The microtask checkpoint runs before the browser applies the deletion, so
    // a beforeinput-scheduled reseed always sees the sentinel still there, does
    // nothing, and leaves the field empty for the next repeat tick.
    expect(taskTerminalFeatureSource).not.toMatch(/queueMicrotask\(\s*\w*[Ss]entinel\s*\)/);
    expect(taskTerminalFeatureSource).toMatch(/addEventListener\("input",\s*\w+\)/);
  });

  it("removes the beforeinput and focus listeners with the identities it registered", () => {
    const cleanup = extractBlock(
      mountTaskTerminalSessionSource,
      /return \(\) => \{\n {4}disposed = true/,
      /\n {2}\};\n\}\s*$/,
    );

    // Matching names are not enough. hardenMobileTextarea runs through an
    // effect event (latest render's closure) while cleanup runs with the
    // effect's own closure, so a plain component-scope arrow resolves to two
    // different functions and the listener is never removed. Each handler must
    // be stable: declared at module scope, or via useEffectEvent.
    const registeredFocus =
      taskTerminalFeatureSource.match(/addEventListener\("focus",\s*(\w+)\)/)?.[1] ?? "add-missing";
    const registeredBeforeInput =
      taskTerminalFeatureSource.match(/addEventListener\("beforeinput",\s*(\w+)\)/)?.[1] ??
      "add-missing";
    const registeredInput =
      taskTerminalFeatureSource.match(/addEventListener\("input",\s*(\w+)\)/)?.[1] ?? "add-missing";

    expect(cleanup).toMatch(
      new RegExp(`removeEventListener\\("beforeinput",\\s*${registeredBeforeInput}\\)`),
    );
    expect(cleanup).toMatch(
      new RegExp(`removeEventListener\\("focus",\\s*${registeredFocus}\\)`),
    );

    expect(cleanup).toMatch(
      new RegExp(`removeEventListener\\("input",\\s*${registeredInput}\\)`),
    );

    for (const handler of [registeredFocus, registeredBeforeInput, registeredInput]) {
      const moduleScope = new RegExp(`^const ${handler}\\s*=`, "m");
      const effectEvent = new RegExp(`const ${handler}\\s*=\\s*useEffectEvent\\(`);
      expect(
        moduleScope.test(taskTerminalFeatureSource) || effectEvent.test(taskTerminalFeatureSource),
      ).toBe(true);
    }
  });
});

describe("TaskTerminal speech input", () => {
  it("auto-inserts ordered finals with no staging composer, and one Mic shortcut after Paste", () => {
    expect(taskTerminalFeatureSource).not.toMatch(/TerminalComposer/);
    expect(taskTerminalFeatureSource).not.toMatch(/terminal-composer/);
    expect(taskTerminalFeatureSource).not.toMatch(/insertComposerTranscript/);
    expect(taskTerminalFeatureSource).not.toMatch(/composerText/);
    expect(taskTerminalFeatureSource).toMatch(/createSpeechTransport/);
    expect(taskTerminalFeatureSource).toMatch(/speechInsertLedger/);
    expect(taskTerminalFeatureSource).toMatch(/undoInsertedSpeech/);
    expect(taskTerminalFeatureSource).toMatch(/isStandaloneStartOver/);

    // Contiguous finalTranscript deltas paste in onFinal (outside setState).
    const onFinal = taskTerminalFeatureSource.match(/onFinal:[\s\S]*?\n {4,8}\},/)?.[0] ?? "";
    expect(onFinal).toMatch(/pasteThroughTerm\(/);
    expect(onFinal).toMatch(/finalTranscript/);
    expect(onFinal).toMatch(/isStandaloneStartOver\(text\)/);
    expect(onFinal).toMatch(/undoInsertedSpeech\(\)/);

    const paste = taskTerminalSource.indexOf(">\n            Paste");
    const mic = taskTerminalSource.indexOf(">\n            Mic");
    expect(paste).toBeGreaterThan(-1);
    expect(mic).toBeGreaterThan(paste);
    expect(taskTerminalFeatureSource).toMatch(/Start voice input/);
    expect(taskTerminalFeatureSource).toMatch(/Stop voice input/);
    expect(taskTerminalFeatureSource).toMatch(/micArmed/);
    expect(taskTerminalFeatureSource).toMatch(/toggleMic\s*\(\s*\)/);
    expect(taskTerminalFeatureSource).toMatch(/request_stop/);
    expect(taskTerminalFeatureSource).toMatch(/speechTransportRef\.current\?\.stop\(\)/);
  });

  it("removes only the visible toolbar Ctrl+C entry and keeps the Ctrl path", () => {
    expect(taskTerminalFeatureSource).not.toMatch(/label:\s*["']⌃C["']/);
    expect(taskTerminalFeatureSource).toMatch(/aria-label=["']Control modifier["']/);
    expect(taskTerminalFeatureSource).toMatch(/sendKey\(consumeCtrl\(data\)\)/);
  });

  it("keeps Mic text visible across active speech states", () => {
    // Mic stays the fixed toolbar label (JSX text child); states only arm styling.
    expect(taskTerminalFeatureSource).toMatch(/>\n\s*Mic\n/);
    expect(taskTerminalFeatureSource).toMatch(/pause_pending/);
    expect(taskTerminalFeatureSource).toMatch(/finalizing/);
    expect(taskTerminalFeatureSource).toMatch(/is-armed/);
    expect(taskTerminalFeatureSource).toMatch(
      /speechModel\.state === "listening" \|\| speechModel\.state === "pause_pending"/,
    );
  });

  it("allows a recoverable error to retry voice input", () => {
    expect(taskTerminalFeatureSource).toMatch(/speechModelRef\.current\.state\s*===\s*["']error["']/);
    expect(taskTerminalFeatureSource).toMatch(/toggleMic\s*\(\s*\)/);
  });

  it("surfaces an unexpected STT socket close as a recoverable error", () => {
    const closeBody =
      taskTerminalFeatureSource.match(/onClosed:\s*\(\)\s*=>\s*\{([\s\S]*?)\n\s*\},/)?.[1] ?? "";

    expect(closeBody).toMatch(/current\.state\s*!==\s*["']finalizing["']/);
    expect(closeBody).toMatch(/Speech connection closed/);
  });

  it("never pastes partial speech into the PTY and never auto-sends Enter", () => {
    expect(taskTerminalFeatureSource).toMatch(/terminal-speech-status/);
    expect(taskTerminalFeatureSource).not.toMatch(/pasteThroughTerm\(speechModel\.partialTranscript/);
    expect(taskTerminalFeatureSource).not.toMatch(/sendInput\(["']\\r["']\)/);
    expect(taskTerminalFeatureSource).not.toMatch(/sendInput\(["']\\n["']\)/);
  });
});

describe("TaskTerminal seeded history reveal", () => {
  it("hides the interaction surface until seeded output goes quiet, then snaps", () => {
    const seedPendingCss =
      stylesSource.match(
        /\.terminal-interaction-wrap\.is-seed-pending\s+\.terminal-host,\s*\n\s*\.terminal-interaction-wrap\.is-seed-pending\s+\.terminal-scroll-spacer\s*\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(seedPendingCss).toMatch(/opacity:\s*0/);
    expect(stylesSource).not.toMatch(
      /\.terminal-interaction-wrap\.is-seed-pending\s*\{[^}]*opacity:\s*0/,
    );

    expect(taskTerminalFeatureSource).toMatch(/SEED_REVEAL_QUIET_MS\s*=\s*120/);
    expect(taskTerminalFeatureSource).toMatch(/SEED_REVEAL_MAX_MS\s*=\s*2000/);
    expect(taskTerminalFeatureSource).toMatch(/~\s*7 batches/);
    expect(taskTerminalFeatureSource).not.toMatch(/SEED_REVEAL_GATE_MIN_BYTES/);

    const mountBody =
      mountTaskTerminalSessionSource.match(
        /export function mountTaskTerminalSession\([\s\S]*?\)\s*:\s*\(\)\s*=>\s*void\s*\{([\s\S]*)\n\}\s*$/,
      )?.[1] ?? "";

    // Hiding starts at the seeded open, not on a byte-size guess about the frame.
    const onOpenBody =
      mountBody.match(/onOpen:\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {4,8}\},/)?.[1] ?? "";
    expect(onOpenBody).toMatch(/if\s*\(\s*seeded\s*\)\s*\{\s*\n\s*beginSeedPending\(\)/);
    expect(onOpenBody).toMatch(/if\s*\(\s*!seeded\s*\)\s*\{\s*\n\s*cancelSeedPending\(\)/);

    // Every write restarts the quiet window: the seed is scrollback only, and the
    // tmux attach repaint of the visible pane lands in later frames.
    const onOutputBody =
      mountBody.match(/onOutput:\s*\([^)]*\)\s*=>\s*\{([\s\S]*?)\n {4,8}\},/)?.[1] ?? "";
    expect(onOutputBody).toMatch(/termRef\.current\?\.write\(/);
    expect(onOutputBody).toMatch(/scrollSync\.applyOutput\(\)/);
    expect(onOutputBody).not.toMatch(/setFollowLive\(true\)/);
    expect(onOutputBody).toMatch(/deferSeedReveal\(\)/);
    expect(onOutputBody).not.toMatch(/classList\.remove\(["']is-seed-pending["']\)/);

    const revealBody =
      mountBody.match(/const revealSeed = \(\) => \{([\s\S]*?)\n {2,4}\};/)?.[1] ?? "";
    expect(revealBody).not.toMatch(/isFollowingLive\(\)/);
    expect(revealBody).toMatch(/scrollSync\.syncSpacer\(\)/);
    expect(revealBody).toMatch(/scrollSync\.setFollowLive\(true\)/);
    const syncSpacerIndex = revealBody.indexOf("scrollSync.syncSpacer()");
    const revealFollowIndex = revealBody.indexOf("scrollSync.setFollowLive(true)");
    expect(syncSpacerIndex).toBeGreaterThan(-1);
    expect(revealFollowIndex).toBeGreaterThan(syncSpacerIndex);
    const revealSnapIndex = revealBody.indexOf("scrollSync.setSyncingScroll(true)");
    expect(revealFollowIndex).toBeGreaterThan(-1);
    expect(revealSnapIndex).toBeGreaterThan(revealFollowIndex);
    expect(revealBody).toMatch(/scrollSync\.setSyncingScroll\(true\)/);
    expect(revealBody).toMatch(/scrollToBottom\(\)/);
    expect(revealBody).toMatch(/scrollSync\.scrollInteractionToBottom\(\)/);
    expect(revealBody).toMatch(/scrollSync\.setSyncingScroll\(false\)/);
    expect(revealBody).toMatch(/scrollSync\.refreshFollow\(\)/);
    const snapIndex = revealBody.indexOf("scrollSync.setSyncingScroll(true)");
    const removeIndex = revealBody.indexOf('classList.remove("is-seed-pending")');
    expect(snapIndex).toBeGreaterThan(-1);
    expect(removeIndex).toBeGreaterThan(snapIndex);

    const deferBody =
      mountBody.match(/const deferSeedReveal = \(\) => \{([\s\S]*?)\n {2,4}\};/)?.[1] ?? "";
    expect(deferBody).toMatch(/clearTimeout\(seedQuietTimer\)/);
    expect(deferBody).toMatch(/setTimeout\(revealSeed, SEED_REVEAL_QUIET_MS\)/);
    expect(deferBody).toMatch(/seedCapTimer \?\?= setTimeout\(revealSeed, SEED_REVEAL_MAX_MS\)/);

    // Timers start at the first write, not at open, so a silent socket never
    // reveals a still-empty grid on a wall-clock deadline.
    const beginBody =
      mountBody.match(/const beginSeedPending = \(\) => \{([\s\S]*?)\n {2,4}\};/)?.[1] ?? "";
    expect(beginBody).toMatch(/classList\.add\(["']is-seed-pending["']\)/);
    expect(beginBody).not.toMatch(/setTimeout/);

    // Mid-parse term scroll sync while hidden would fight the reveal snap.
    // Wrapper scroll must still run so followLive can drop for "New output".
    expect(mountBody).toMatch(
      /onScroll\(\(\)\s*=>\s*\{[\s\S]*?if\s*\(\s*isSeedPending\(\)\s*\)\s*return;[\s\S]*?scrollSync\.onTermScroll\(\)/,
    );
    const wrapScrollBody =
      mountBody.match(/const onWrapScroll = \(\) => \{([\s\S]*?)\n {2,4}\};/)?.[1] ?? "";
    expect(wrapScrollBody).toMatch(/onRestorePinnedScroll\(\)/);
    expect(wrapScrollBody).toMatch(/scrollSync\.onInteractionScroll\(\)/);
    expect(wrapScrollBody).not.toMatch(/isSeedPending\(\)/);
  });
});
