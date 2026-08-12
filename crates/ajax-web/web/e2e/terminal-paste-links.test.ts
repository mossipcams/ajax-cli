// Focused paste-link coverage. Lives outside terminal-behavior.test.ts so this
// PR does not touch that already-over-LOC file (see #783 / 0eaf9094).

import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  terminalInputFrames,
  waitForTerminalSocket,
} from "./fixtures";

async function openTaskTerminal(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalSurface(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
}

async function clickTerminalSurfaceInterior(page: import("@playwright/test").Page) {
  const surface = terminalSurface(page);
  const box = await surface.boundingBox();
  if (!box) throw new Error("terminal surface box missing");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

async function inputFrameCount(page: import("@playwright/test").Page) {
  return (await terminalInputFrames(page)).length;
}

test("insertText beforeinput with a full http(s) URL sends the link once", async ({
  page,
}) => {
  await openTaskTerminal(page);
  await clickTerminalSurfaceInterior(page);

  const url = "https://example.com/insert-text-beforeinput";
  const baseline = await inputFrameCount(page);

  await page.evaluate((pasteUrl) => {
    const textarea = document.querySelector(
      "textarea.xterm-helper-textarea",
    ) as HTMLTextAreaElement | null;
    if (!textarea) throw new Error("helper textarea missing");
    textarea.focus();
    textarea.dispatchEvent(
      new InputEvent("beforeinput", {
        inputType: "insertText",
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
