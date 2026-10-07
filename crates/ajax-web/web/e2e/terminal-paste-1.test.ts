import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  terminalToolbar,
  terminalInputFrames,
  openLatestTerminalSocket,
  closeLatestTerminalSocket,
  emitLatestTerminalOutput,
  waitForTerminalSocket
} from "./fixtures";
import {
  MULTILINE_UNICODE_CLIPBOARD,
  activeTaskSocketCount,
  clickTerminalSurfaceInterior,
  expandTerminalButton,
  gotoTaskRoute,
  inputFrameCount,
  openTaskTerminal,
  scrollbackChunk,
  syntheticScrollGestureOnInteractionSurface
} from "./terminal-behavior-helpers";


test.beforeEach((_, testInfo) => {
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


test("multiline Unicode paste preserves content in one input frame", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: MULTILINE_UNICODE_CLIPBOARD });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(MULTILINE_UNICODE_CLIPBOARD);
});


test("native uri-list-only paste sends the link URL in one input frame", async ({ page }) => {
  await openTaskTerminal(page);
  await clickTerminalSurfaceInterior(page);

  const url = "https://example.com/a";
  const baseline = await inputFrameCount(page);

  await page.evaluate((pasteUrl) => {
    const textarea = document.querySelector(
      "textarea.xterm-helper-textarea",
    ) as HTMLTextAreaElement | null;
    if (!textarea) throw new Error("helper textarea missing");
    textarea.focus();
    const data = new DataTransfer();
    data.setData("text/uri-list", pasteUrl);
    data.setData("text/plain", "");
    textarea.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, url);

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(url);
});


test("native plain URL paste sends the link in one input frame", async ({ page }) => {
  await openTaskTerminal(page);
  await clickTerminalSurfaceInterior(page);

  const url = "https://example.com/plain";
  const baseline = await inputFrameCount(page);

  await page.evaluate((pasteUrl) => {
    const textarea = document.querySelector(
      "textarea.xterm-helper-textarea",
    ) as HTMLTextAreaElement | null;
    if (!textarea) throw new Error("helper textarea missing");
    textarea.focus();
    const data = new DataTransfer();
    data.setData("text/plain", pasteUrl);
    textarea.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, url);

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(url);
});


test("empty sync clipboardData paste recovers URL from helper textarea input", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await clickTerminalSurfaceInterior(page);

  const url = "https://example.com/textarea-recover";
  const baseline = await inputFrameCount(page);

  await page.evaluate((pasteUrl) => {
    const textarea = document.querySelector(
      "textarea.xterm-helper-textarea",
    ) as HTMLTextAreaElement | null;
    if (!textarea) throw new Error("helper textarea missing");
    textarea.focus();
    const data = new DataTransfer();
    textarea.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
    textarea.value = `\u200B${pasteUrl}`;
    textarea.dispatchEvent(
      new InputEvent("input", {
        inputType: "insertFromPaste",
        bubbles: true,
      }),
    );
  }, url);

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(url);
});


test("insertFromPaste beforeinput sends a link when clipboardData is empty", async ({ page }) => {
  await openTaskTerminal(page);
  await clickTerminalSurfaceInterior(page);

  const url = "https://example.com/beforeinput";
  const baseline = await inputFrameCount(page);

  await page.evaluate((pasteUrl) => {
    const textarea = document.querySelector(
      "textarea.xterm-helper-textarea",
    ) as HTMLTextAreaElement | null;
    if (!textarea) throw new Error("helper textarea missing");
    textarea.focus();
    textarea.dispatchEvent(
      new InputEvent("beforeinput", {
        inputType: "insertFromPaste",
        data: pasteUrl,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, url);

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(url);
});


test("bracketed paste wraps toolbar paste in DEC bracket mode", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: MULTILINE_UNICODE_CLIPBOARD });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, ["\x1b[?2004h"]);
  await page.evaluate(
    () => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
  );

  const baseline = await inputFrameCount(page);
  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  const bracketedText = MULTILINE_UNICODE_CLIPBOARD;
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(`\x1b[200~${bracketedText}\x1b[201~`);
});


test("clipboard fallback opens accessible paste controls when readText is unavailable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  await expect(page.getByRole("textbox", { name: "Paste text" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Send" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
  await expect(page.getByRole("status")).toContainText(/clipboard/i);
});


test("paste fallback retains unsent multiline Unicode text when socket closes before Send", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { autoOpen: false, clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await openLatestTerminalSocket(page);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  const input = page.getByRole("textbox", { name: "Paste text" });
  await expect(input).toBeVisible();
  await input.fill(MULTILINE_UNICODE_CLIPBOARD);

  const baseline = await inputFrameCount(page);
  await closeLatestTerminalSocket(page);

  await page.getByRole("button", { name: "Send" }).click();

  await expect(input).toBeVisible();
  await expect(input).toHaveValue(MULTILINE_UNICODE_CLIPBOARD);
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
  await expect(page.getByRole("status")).toContainText(/disconnect|unavailable|reconnect/i);
  await expect(page.getByRole("button", { name: "Reconnect" })).toBeVisible();
});


test("clipboard paste retains exact text in fallback when socket is disconnected", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { autoOpen: false, clipboardText: MULTILINE_UNICODE_CLIPBOARD });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await openLatestTerminalSocket(page);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);

  await closeLatestTerminalSocket(page);
  await expect(page.getByRole("button", { name: "Reconnect" })).toBeVisible();

  const baseline = await inputFrameCount(page);
  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  const input = page.getByRole("textbox", { name: "Paste text" });
  await expect(input).toBeVisible();
  await expect(input).toHaveValue(MULTILINE_UNICODE_CLIPBOARD);
  await expect.poll(async () => inputFrameCount(page)).toBe(baseline);
  await expect(page.getByRole("status")).toContainText(/disconnect|unavailable|reconnect/i);
});


test("hotbar Paste label stays inside its button on phone", async ({ page }) => {
  await openTaskTerminal(page);

  const pasteFit = await page.evaluate(() => {
    const paste = document.querySelector(
      '.terminal-keys .terminal-key[aria-label="Paste"]',
    ) as HTMLElement | null;
    if (!paste) return null;
    const style = getComputedStyle(paste);
    return {
      scrollWidth: paste.scrollWidth,
      clientWidth: paste.clientWidth,
      overflow: style.overflow,
      whiteSpace: style.whiteSpace,
    };
  });

  expect(pasteFit).not.toBeNull();
  expect(pasteFit!.overflow).toBe("hidden");
  expect(pasteFit!.whiteSpace).toBe("nowrap");
  expect(pasteFit!.scrollWidth).toBeLessThanOrEqual(pasteFit!.clientWidth + 1);
});


test("paste fallback preserves prior terminal focus when another control owns focus", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await page.getByRole("button", { name: "← Back" }).focus();

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();

  await page.getByRole("button", { name: "Cancel" }).click();
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);

  await page.getByRole("button", { name: "← Back" }).focus();
  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();
  await page.getByRole("textbox", { name: "Paste text" }).fill("fallback-text");

  const baseline = await inputFrameCount(page);
  await page.getByRole("button", { name: "Send" }).click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);
});


test("paste fallback restores terminal focus when terminal owned focus", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardUnavailable: true });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await clickTerminalSurfaceInterior(page);
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();

  await page.getByRole("button", { name: "Cancel" }).click();
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(true);
});


test("Paste preserves prior terminal focus when another control owns focus", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: "paste-me" });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);
  await page.getByRole("button", { name: "← Back" }).focus();

  await terminalToolbar(page).getByRole("button", { name: "Paste" }).click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const textarea = document.querySelector(".terminal-host textarea.xterm-helper-textarea");
        return textarea === document.activeElement;
      }),
    )
    .toBe(false);
});


test("Paste stays available after synthetic scroll gesture and fullscreen transitions", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { clipboardText: MULTILINE_UNICODE_CLIPBOARD });
  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 80)]);
  await syntheticScrollGestureOnInteractionSurface(page);

  const expand = expandTerminalButton(page);
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "true");
  await expand.click();
  await expect(expand).toHaveAttribute("aria-pressed", "false");

  const paste = terminalToolbar(page).getByRole("button", { name: "Paste" });
  await expect(paste).toBeVisible();

  const baseline = await inputFrameCount(page);
  await paste.click();

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe(MULTILINE_UNICODE_CLIPBOARD);
});
