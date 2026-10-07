import { test, expect } from "@playwright/test";
import {
  terminalInteractionSurface,
  terminalToolbar,
  terminalInputFrames,
  emitLatestTerminalOutput
} from "./fixtures";
import {
  boxesIntersect,
  chromeDisplayState,
  clearKeyboardBand,
  clickTerminalSurfaceInterior,
  expandTerminalButton,
  expectFlushToBand,
  inputFrameCount,
  openTaskTerminal,
  readBandFlushGeometry,
  scrollInteractionSurfaceAway,
  scrollbackChunk,
  simulateKeyboardBand,
  visibleAppBand
} from "./terminal-behavior-helpers";


test.beforeEach(({}, testInfo) => {
  const desktopOnly =
    testInfo.title ===
    "desktop expanded mode keeps terminal bounded and task details summary reachable";
  if (desktopOnly) {
    test.skip(
      testInfo.project.name !== "desktop-chromium",
      "desktop expanded layout only",
    );
  } else {
    test.skip(
      testInfo.project.name !== "mobile-webkit",
      "terminal acceptance is mobile-webkit only",
    );
  }
});


test("phone fullscreen keeps background controls inert until exit", async ({ page }) => {
  await openTaskTerminal(page);
  const expandProbe = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');

  const backProbe = page.locator(".task-detail .back");
  const summaryProbe = page.locator(".meta-details summary");
  const dismissProbe = page.locator(".result-panel button.pill");

  await page.locator("[data-action='review']").click();
  await expect(page.locator(".result-panel")).toBeVisible({ timeout: 10_000 });

  await expandProbe.evaluate((el) => (el as HTMLButtonElement).click());
  await expect(expandProbe).toHaveAttribute("aria-pressed", "true");

  expect(
    await page.evaluate(() => {
      const header = document.querySelector(".task-detail .detail-header");
      const chrome = document.querySelector(".cockpit-chrome");
      const nav = document.querySelector(".bottom-nav");
      const meta = document.querySelector(".meta-details");
      const result = document.querySelector(".result-panel");
      return (
        header instanceof HTMLElement &&
        header.inert &&
        chrome instanceof HTMLElement &&
        chrome.inert &&
        nav instanceof HTMLElement &&
        nav.inert &&
        meta instanceof HTMLElement &&
        meta.inert &&
        result instanceof HTMLElement &&
        result.inert
      );
    }),
  ).toBe(true);

  await backProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".task-detail .back") === document.activeElement,
    ),
  ).toBe(false);

  await dismissProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".result-panel button.pill") === document.activeElement,
    ),
  ).toBe(false);

  await expandProbe.evaluate((el) => (el as HTMLButtonElement).click());
  await expect(expandProbe).toHaveAttribute("aria-pressed", "false");

  expect(
    await page.evaluate(() => {
      const header = document.querySelector(".task-detail .detail-header");
      const chrome = document.querySelector(".cockpit-chrome");
      const nav = document.querySelector(".bottom-nav");
      const meta = document.querySelector(".meta-details");
      const result = document.querySelector(".result-panel");
      return (
        header instanceof HTMLElement &&
        !header.inert &&
        chrome instanceof HTMLElement &&
        !chrome.inert &&
        nav instanceof HTMLElement &&
        !nav.inert &&
        meta instanceof HTMLElement &&
        !meta.inert &&
        result instanceof HTMLElement &&
        !result.inert
      );
    }),
  ).toBe(true);

  await backProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".task-detail .back") === document.activeElement,
    ),
  ).toBe(true);

  await dismissProbe.evaluate((el) => (el as HTMLElement).focus());
  expect(
    await page.evaluate(
      () => document.querySelector(".result-panel button.pill") === document.activeElement,
    ),
  ).toBe(true);

  await summaryProbe.evaluate((el) => (el as HTMLElement).click());
  expect(
    await page.evaluate(() => document.querySelector(".meta-details")?.hasAttribute("open")),
  ).toBe(true);

  await page.evaluate(() => {
    window.location.hash = "#/";
  });
  await expect(page.locator("[data-outlet='dashboard']")).toBeVisible({ timeout: 10_000 });
});


test("fullscreen exit blurs the terminal textarea without PTY input", async ({ page }) => {
  await openTaskTerminal(page);
  const expand = expandTerminalButton(page);

  await clickTerminalSurfaceInterior(page);
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);

  const baseline = await inputFrameCount(page);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);

  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
});


test("fullscreen band keeps expand tappable under keyboard-open offset band", async ({ page }) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);

  const expand = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  const band = await visibleAppBand(page);
  const box = await expand.boundingBox();
  expect(box).not.toBeNull();
  expect(boxesIntersect(box!, band)).toBe(true);

  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");
});


test("fullscreen with keyboard-open pins panel bottom to the visual viewport band", async ({ page }) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);

  const expand = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  expectFlushToBand(await readBandFlushGeometry(page, '[data-testid="task-terminal-panel"]'), {
    expanded: true,
  });
});


test("expand then keyboard-open still pins panel bottom to the visual viewport band", async ({
  page,
}) => {
  await openTaskTerminal(page);

  const expand = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");

  await simulateKeyboardBand(page);

  expectFlushToBand(await readBandFlushGeometry(page, '[data-testid="task-terminal-panel"]'), {
    expanded: true,
  });
});


test("inline keyboard-open pins task-detail to the visual viewport band", async ({ page }) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);

  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });

  const chrome = await chromeDisplayState(page);
  expect(chrome.detailHeader).not.toBe("none");
  expect(chrome.interactPanel).not.toBe("none");
});


test("inline keyboard-open keeps the whole detail-header row inside the visible band", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page, { top: 47, height: 420 });

  const band = await visibleAppBand(page);
  const header = page.locator('[data-testid="mobile-chrome-header"]');
  const back = header.locator(".back");
  const status = header.locator(".interact-pill");

  await expect(header).toBeVisible();
  await expect(back).toBeVisible();
  await expect(status).toBeVisible();

  const headerBox = await header.boundingBox();
  const backBox = await back.boundingBox();
  const statusBox = await status.boundingBox();
  expect(headerBox).not.toBeNull();
  expect(backBox).not.toBeNull();
  expect(statusBox).not.toBeNull();

  expect(headerBox!.y).toBeGreaterThanOrEqual(band.top - 1);
  expect(headerBox!.y + headerBox!.height).toBeLessThanOrEqual(band.bottom + 1);
  expect(boxesIntersect(backBox!, band)).toBe(true);
  expect(boxesIntersect(statusBox!, band)).toBe(true);
});


test("keyboard band CSS uses height pin and forbids 100lvh bottom math", async ({ page }) => {
  await openTaskTerminal(page);

  const contract = await page.evaluate(() => {
    const texts = Array.from(document.styleSheets).flatMap((sheet) => {
      try {
        return Array.from(sheet.cssRules).map((rule) => rule.cssText);
      } catch {
        return [] as string[];
      }
    });
    const joined = texts.join("\n");
    return {
      hasLvhBottom: /bottom:\s*max\(\s*0px,\s*calc\(\s*100lvh\s*-\s*var\(--app-top/.test(joined),
      taskDetailHeightPin:
        /keyboard-open:not\(\.terminal-expanded\)[\s\S]*?\.task-detail[\s\S]*?height:\s*var\(--app-height/.test(
          joined,
        ) ||
        /html\.keyboard-open:not\(\.terminal-expanded\)\s+\.task-detail/.test(joined),
      appHeightVarInUse: /height:\s*var\(--app-height/.test(joined),
    };
  });

  expect(contract.hasLvhBottom).toBe(false);
  expect(contract.appHeightVarInUse).toBe(true);
});


test("exit fullscreen while keyboard-open pins inline task-detail to the band", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);

  const expand = page.locator('[data-testid="task-terminal-panel"] .terminal-expand-corner');
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });
});


test("keyboard-open pin tracks live visual-viewport band CSS updates", async ({ page }) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page, { top: 40, height: 460 });
  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });

  await simulateKeyboardBand(page, { top: 72, height: 390 });
  const after = await readBandFlushGeometry(page, ".task-detail");
  expectFlushToBand(after, { expanded: false });
  expect(after!.bandTop).toBe(72);
  expect(after!.bandHeight).toBe(390);
  expect(after!.pinnedTop).toBeCloseTo(72, 0);
  expect(after!.pinnedHeight).toBeCloseTo(390, 0);
});


test("keyboard close then reopen still pins inline task-detail flush to the band", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await simulateKeyboardBand(page);
  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });

  await clearKeyboardBand(page);
  const cleared = await page.evaluate(() => ({
    keyboardOpen: document.documentElement.classList.contains("keyboard-open"),
    detailPosition: getComputedStyle(document.querySelector(".task-detail")!).position,
  }));
  expect(cleared.keyboardOpen).toBe(false);
  expect(cleared.detailPosition).not.toBe("fixed");

  await simulateKeyboardBand(page, { top: 48, height: 420 });
  expectFlushToBand(await readBandFlushGeometry(page, ".task-detail"), { expanded: false });
});


test("keyboard-open hides cockpit chrome and bottom nav on task route", async ({ page }) => {
  await openTaskTerminal(page);

  const before = await chromeDisplayState(page);
  expect(before.cockpit).not.toBe("none");
  expect(before.bottomNav).not.toBe("none");

  await simulateKeyboardBand(page);
  const hidden = await chromeDisplayState(page);
  expect(hidden.cockpit).toBe("none");
  expect(hidden.bottomNav).toBe("none");
  expect(hidden.detailHeader).not.toBe("none");
  expect(hidden.interactPanel).not.toBe("none");

  await clearKeyboardBand(page);
  const restored = await chromeDisplayState(page);
  expect(restored.cockpit).not.toBe("none");
  expect(restored.bottomNav).not.toBe("none");
});


test("terminal-expanded hides cockpit chrome and bottom nav on task route", async ({ page }) => {
  await openTaskTerminal(page);

  const before = await chromeDisplayState(page);
  expect(before.cockpit).not.toBe("none");
  expect(before.bottomNav).not.toBe("none");

  await page.evaluate(() => {
    document.documentElement.classList.add("terminal-expanded");
  });
  const hidden = await chromeDisplayState(page);
  expect(hidden.cockpit).toBe("none");
  expect(hidden.bottomNav).toBe("none");
  expect(hidden.detailHeader).toBe("none");
  expect(hidden.interactPanel).toBe("none");

  await page.evaluate(() => {
    document.documentElement.classList.remove("terminal-expanded");
  });
  const restored = await chromeDisplayState(page);
  expect(restored.cockpit).not.toBe("none");
  expect(restored.bottomNav).not.toBe("none");
});


test("interaction wrap hides scrollbar chrome", async ({ page }) => {
  await openTaskTerminal(page);
  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 120)]);

  const wrap = terminalInteractionSurface(page);
  await expect(wrap).toHaveCSS("scrollbar-width", "none");

  const webkitHidden = await wrap.evaluate((_el) => {
    const rules = Array.from(document.styleSheets).flatMap((sheet) => {
      try {
        return Array.from(sheet.cssRules);
      } catch {
        return [];
      }
    });
    const selectorMatches = rules.some(
      (rule) =>
        rule instanceof CSSStyleRule &&
        rule.selectorText.includes(".terminal-interaction-wrap") &&
        rule.selectorText.includes("::-webkit-scrollbar") &&
        rule.style.display === "none",
    );
    return selectorMatches;
  });
  expect(webkitHidden).toBe(true);
});


test("keyboard activation does not reuse pointer focus ownership", async ({ page }) => {
  await openTaskTerminal(page);
  const toolbar = terminalToolbar(page);
  const tab = toolbar.getByRole("button", { name: "Tab" });
  const esc = toolbar.getByRole("button", { name: "Escape" });

  await clickTerminalSurfaceInterior(page);
  await tab.click();

  await page.getByRole("button", { name: "← Back" }).focus();

  await tab.focus();
  await page.keyboard.press("Enter");

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);

  await esc.focus();
  await page.keyboard.press("Space");

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);
});


test("terminal Space input preserves scroll and keyboard-band geometry", async ({ page }) => {
  await openTaskTerminal(page);
  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await scrollInteractionSurfaceAway(page);
  await page.evaluate(() => {
    const ta = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
    if (!(ta instanceof HTMLTextAreaElement)) throw new Error("textarea missing");
    ta.focus({ preventScroll: true });
  });
  await simulateKeyboardBand(page);
  await new Promise((r) => setTimeout(r, 400));

  const read = () =>
    page.evaluate(() => {
      const host = document.querySelector(
        "[data-testid='task-terminal-panel'] .terminal-host",
      ) as (HTMLElement & { __xterm?: { buffer: { active: { viewportY: number } } } }) | null;
      const wrap = document.querySelector(
        "[data-testid='terminal-interaction-surface']",
      ) as HTMLElement | null;
      const detail = document.querySelector(".task-detail")?.getBoundingClientRect();
      const panel = document
        .querySelector("[data-testid='task-terminal-panel']")
        ?.getBoundingClientRect();
      const ta = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
      return {
        wrapScrollTop: wrap?.scrollTop ?? -1,
        viewportY: host?.__xterm?.buffer.active.viewportY ?? -1,
        detailTop: detail?.top ?? -1,
        detailHeight: detail?.height ?? -1,
        panelTop: panel?.top ?? -1,
        panelHeight: panel?.height ?? -1,
        focused: ta === document.activeElement,
        keyboardOpen: document.documentElement.classList.contains("keyboard-open"),
      };
    });

  const baseline = await inputFrameCount(page);
  const before = await read();
  expect(before.focused).toBe(true);
  expect(before.keyboardOpen).toBe(true);
  expect(before.wrapScrollTop).toBeGreaterThan(0);

  await page.keyboard.press("Space");
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  expect((await terminalInputFrames(page)).at(-1)?.data).toBe(" ");
  await new Promise((r) => setTimeout(r, 400));

  const after = await read();
  expect(after.wrapScrollTop).toBe(before.wrapScrollTop);
  expect(after.viewportY).toBe(before.viewportY);
  expect(Math.abs(after.detailTop - before.detailTop)).toBeLessThanOrEqual(1);
  expect(Math.abs(after.detailHeight - before.detailHeight)).toBeLessThanOrEqual(1);
  expect(Math.abs(after.panelTop - before.panelTop)).toBeLessThanOrEqual(1);
  expect(Math.abs(after.panelHeight - before.panelHeight)).toBeLessThanOrEqual(1);
  expect(after.focused).toBe(true);
  expect(after.keyboardOpen).toBe(true);
});
