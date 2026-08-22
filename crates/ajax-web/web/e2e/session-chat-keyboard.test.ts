import { expect, test, type Page } from "@playwright/test";
import {
  DETAIL_FIXTURE,
  emulateCoarsePointer,
  emulateHomeIndicatorInset,
  mockFetch,
  sessionEventJson,
  sessionSnapshotJson,
} from "./fixtures";

async function openSessionChat(page: Page) {
  await emulateCoarsePointer(page);
  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, session_capable: true, agent: "Cursor" },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as { type?: string; clientMessageId?: string };
      if (event.type === "prompt" && event.clientMessageId) {
        socket.send(
          sessionEventJson(0, { type: "prompt_accepted", clientMessageId: event.clientMessageId }),
        );
      }
    });
    socket.send(sessionSnapshotJson({ cursor: 0, model: "auto", turnState: "idle" }));
  });
  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible();
}

async function seedTranscript(page: Page, lines: number) {
  await page.evaluate(({ lines: count }) => {
    const thread = document.querySelector('[data-testid="session-thread"]');
    if (!thread) return;
    thread.innerHTML = "";
    for (let i = 0; i < count; i++) {
      const p = document.createElement("p");
      p.className = "session-reply";
      p.textContent = `Line ${i + 1}: ${"content ".repeat(24)}`;
      p.setAttribute("data-testid", "session-message-agent");
      thread.appendChild(p);
    }
  }, { lines });
}

type Geometry = {
  transcriptBottom: number;
  composerTop: number;
  composerBottom: number;
  layoutBottom: number;
  gapTranscriptComposer: number;
  gapComposerLayout: number;
  threadScrollHeight: number;
  threadClientHeight: number;
  threadScrollTop: number;
  surfacePaddingBottom: number;
  appViewportPosition: string;
  keyboardOpen: boolean;
  routeScrollCanScroll: boolean;
};

type TextareaFlushGeometry = {
  textareaBottom: number;
  composerBottom: number;
  layoutBottom: number;
  textareaComposerGap: number;
  textareaLayoutGap: number;
  composerPaddingBottom: number;
  textareaPaddingBottom: number;
  keyboardOpen: boolean;
};

async function readTextareaFlushGeometry(page: Page): Promise<TextareaFlushGeometry> {
  return page.evaluate(() => {
    const composer = document.querySelector('[data-testid="session-composer"]') as HTMLElement;
    const textarea = document.querySelector(
      '[data-testid="session-composer"] textarea',
    ) as HTMLTextAreaElement;
    const appViewport = document.querySelector('[data-testid="app-viewport"]') as HTMLElement;
    const appRoot = document.querySelector("#app") as HTMLElement;
    const layoutEl = appViewport ?? appRoot;
    const composerRect = composer.getBoundingClientRect();
    const textareaRect = textarea.getBoundingClientRect();
    const layoutBottom = layoutEl?.getBoundingClientRect().bottom ?? window.innerHeight;
    const composerStyle = getComputedStyle(composer);
    const textareaStyle = getComputedStyle(textarea);
    return {
      textareaBottom: textareaRect.bottom,
      composerBottom: composerRect.bottom,
      layoutBottom,
      textareaComposerGap: composerRect.bottom - textareaRect.bottom,
      textareaLayoutGap: layoutBottom - textareaRect.bottom,
      composerPaddingBottom: parseFloat(composerStyle.paddingBottom) || 0,
      textareaPaddingBottom: parseFloat(textareaStyle.paddingBottom) || 0,
      keyboardOpen: document.documentElement.classList.contains("keyboard-open"),
    };
  });
}

async function readGeometry(page: Page): Promise<Geometry> {
  return page.evaluate(() => {
    const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
    const composer = document.querySelector('[data-testid="session-composer"]') as HTMLElement;
    const surface = document.querySelector('[data-testid="session-chat-surface"]') as HTMLElement;
    const routeScroll = document.querySelector('[data-testid="route-scroll"]') as HTMLElement;
    const appViewport = document.querySelector('[data-testid="app-viewport"]') as HTMLElement;
    const appRoot = document.querySelector("#app") as HTMLElement;
    const layoutEl = appViewport ?? appRoot;
    const threadRect = thread.getBoundingClientRect();
    const composerRect = composer.getBoundingClientRect();
    const layoutBottom = layoutEl?.getBoundingClientRect().bottom ?? window.innerHeight;
    const surfaceStyle = surface?.style.paddingBottom ?? "";
    return {
      transcriptBottom: threadRect.bottom,
      composerTop: composerRect.top,
      composerBottom: composerRect.bottom,
      layoutBottom,
      gapTranscriptComposer: composerRect.top - threadRect.bottom,
      gapComposerLayout: layoutBottom - composerRect.bottom,
      threadScrollHeight: thread.scrollHeight,
      threadClientHeight: thread.clientHeight,
      threadScrollTop: thread.scrollTop,
      surfacePaddingBottom: parseInt(surfaceStyle, 10) || 0,
      appViewportPosition: appViewport ? getComputedStyle(appViewport).position : "",
      keyboardOpen: document.documentElement.classList.contains("keyboard-open"),
      routeScrollCanScroll:
        routeScroll.scrollHeight > routeScroll.clientHeight + 1 &&
        ["auto", "scroll"].includes(getComputedStyle(routeScroll).overflowY),
    };
  });
}

async function simulateKeyboard(
  page: Page,
  keyboardPx: number,
  opts: { innerHeightShrinks?: boolean } = {},
) {
  await page.evaluate(
    ({ keyboardPx, shrinkInner }) => {
      const vv = window.visualViewport;
      if (!vv) return;
      const fullH = shrinkInner ? window.innerHeight : Math.max(window.innerHeight, 800);
      const newVvH = fullH - keyboardPx;
      Object.defineProperty(vv, "height", { get: () => newVvH, configurable: true });
      Object.defineProperty(vv, "offsetTop", { get: () => 0, configurable: true });
      if (!shrinkInner) {
        Object.defineProperty(window, "innerHeight", {
          get: () => fullH,
          configurable: true,
        });
      } else {
        Object.defineProperty(window, "innerHeight", {
          get: () => newVvH,
          configurable: true,
        });
      }
      vv.dispatchEvent(new Event("resize"));
      document.documentElement.classList.toggle("keyboard-open", keyboardPx > 100);
      document.documentElement.style.setProperty("--app-height", `${newVvH}px`);
    },
    { keyboardPx, shrinkInner: opts.innerHeightShrinks ?? false },
  );
  await page.waitForTimeout(400);
}

async function dismissKeyboard(page: Page) {
  await page.evaluate(() => {
    const vv = window.visualViewport;
    if (!vv) return;
    const fullH = window.innerHeight;
    Object.defineProperty(vv, "height", { get: () => fullH, configurable: true });
    vv.dispatchEvent(new Event("resize"));
    document.documentElement.classList.remove("keyboard-open");
    document.documentElement.style.removeProperty("--app-height");
  });
  await page.waitForTimeout(400);
}

test.describe("Session chat keyboard geometry (#877)", () => {
  test("composer stays outside the transcript scroller", async ({ page }) => {
    await openSessionChat(page);
    const nested = await page.evaluate(() => {
      const thread = document.querySelector('[data-testid="session-thread"]');
      const composer = document.querySelector('[data-testid="session-composer"]');
      return thread?.contains(composer) ?? false;
    });
    expect(nested).toBe(false);
  });

  test("route-scroll is not the vertical scroll owner on session", async ({ page }) => {
    await openSessionChat(page);
    await seedTranscript(page, 40);
    const geo = await readGeometry(page);
    expect(geo.routeScrollCanScroll).toBe(false);
    expect(geo.threadScrollHeight).toBeGreaterThan(geo.threadClientHeight);
  });

  test("keyboard open while pinned keeps composer flush to transcript", async ({ page }) => {
    await openSessionChat(page);
    await seedTranscript(page, 30);
    await page.evaluate(() => {
      const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
      thread.scrollTop = thread.scrollHeight;
    });
    await simulateKeyboard(page, 280);
    const geo = await readGeometry(page);
    expect(geo.gapTranscriptComposer).toBeLessThan(8);
    expect(geo.surfacePaddingBottom).toBeGreaterThanOrEqual(250);
  });

  test("keyboard close while pinned clears blank region and repins live edge (#930)", async ({ page }) => {
    await openSessionChat(page);
    await seedTranscript(page, 30);
    const beforeOpen = await page.evaluate(() => {
      const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
      thread.scrollTop = thread.scrollHeight;
      return {
        scrollTop: thread.scrollTop,
        scrollHeight: thread.scrollHeight,
        clientHeight: thread.clientHeight,
      };
    });
    await simulateKeyboard(page, 280);
    const during = await readGeometry(page);
    await dismissKeyboard(page);
    const geo = await readGeometry(page);
    expect(geo.keyboardOpen).toBe(false);
    expect(geo.surfacePaddingBottom).toBe(0);
    expect(geo.appViewportPosition).not.toBe("fixed");
    expect(geo.gapTranscriptComposer).toBeLessThan(8);
    expect(geo.gapTranscriptComposer - during.gapTranscriptComposer).toBeLessThan(20);
    expect(geo.threadScrollTop + geo.threadClientHeight).toBeGreaterThanOrEqual(
      geo.threadScrollHeight - 48,
    );
    expect(geo.threadScrollHeight).toBeGreaterThanOrEqual(beforeOpen.scrollHeight);
  });

  test("keyboard dismiss while scrolled up preserves visible content (#930)", async ({ page }) => {
    await openSessionChat(page);
    await seedTranscript(page, 80);
    const anchor = await page.evaluate(() => {
      const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
      const mid = Array.from(thread.querySelectorAll("p")).find((p) =>
        p.textContent?.startsWith("Line 41:"),
      ) as HTMLElement | undefined;
      if (!mid) throw new Error("expected mid-history Line 41 message");

      const lineTop = mid.offsetTop;
      thread.scrollTop = Math.max(0, lineTop - Math.floor(thread.clientHeight / 3));
      thread.dispatchEvent(new Event("scroll", { bubbles: false }));

      const threadRect = thread.getBoundingClientRect();
      const rect = mid.getBoundingClientRect();
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      const hit = document.elementFromPoint(centerX, centerY);

      return {
        scrollTop: thread.scrollTop,
        scrollHeight: thread.scrollHeight,
        anchorText: mid.textContent?.slice(0, 60) ?? "",
        anchorInView:
          rect.bottom > threadRect.top + 4 &&
          rect.top < threadRect.bottom - 4 &&
          (hit === mid || mid.contains(hit)),
      };
    });
    expect(anchor.anchorInView).toBe(true);

    await simulateKeyboard(page, 260);
    await dismissKeyboard(page);

    const after = await page.evaluate(({ anchorText }) => {
      const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
      const mid = Array.from(thread.querySelectorAll("p")).find((p) =>
        p.textContent?.startsWith(anchorText.slice(0, 20)),
      ) as HTMLElement | undefined;
      if (!mid) throw new Error("mid-history anchor missing after dismiss");

      const threadRect = thread.getBoundingClientRect();
      const rect = mid.getBoundingClientRect();
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      const hit = document.elementFromPoint(centerX, centerY);

      return {
        scrollTop: thread.scrollTop,
        scrollHeight: thread.scrollHeight,
        anchorInView:
          rect.bottom > threadRect.top + 4 &&
          rect.top < threadRect.bottom - 4 &&
          (hit === mid || mid.contains(hit)),
      };
    }, { anchorText: anchor.anchorText });

    expect(after.scrollTop).toBe(anchor.scrollTop);
    expect(after.anchorInView).toBe(true);
    expect(after.scrollHeight).toBeGreaterThanOrEqual(anchor.scrollHeight);
  });

  test("keyboard open/close while scrolled up keeps transcript flush to composer", async ({ page }) => {
    await openSessionChat(page);
    await seedTranscript(page, 80);
    const thread = page.getByTestId("session-thread");
    await thread.evaluate((el) => {
      const target = Math.min(480, el.scrollHeight - el.clientHeight - 120);
      el.scrollTop = target;
      el.dispatchEvent(new Event("scroll", { bubbles: false }));
    });
    const before = await readGeometry(page);
    await simulateKeyboard(page, 260);
    const during = await readGeometry(page);
    await dismissKeyboard(page);
    const after = await readGeometry(page);
    expect(during.gapTranscriptComposer).toBeLessThan(8);
    expect(after.gapTranscriptComposer).toBeLessThan(8);
    expect(after.gapTranscriptComposer - before.gapTranscriptComposer).toBeLessThan(20);
    expect(after.surfacePaddingBottom).toBe(0);
  });

  test("tap transcript dismisses keyboard without keyboard-sized blank region", async ({ page }) => {
    await openSessionChat(page);
    await seedTranscript(page, 25);
    const composer = page.getByLabel("Message");
    await composer.click();
    await simulateKeyboard(page, 300);
    const during = await readGeometry(page);
    await page.getByTestId("session-thread").click({ position: { x: 40, y: 40 } });
    await page.waitForTimeout(400);
    const geo = await readGeometry(page);
    expect(geo.gapTranscriptComposer).toBeLessThan(8);
    expect(geo.surfacePaddingBottom).toBe(0);
    expect(geo.appViewportPosition).not.toBe("fixed");
    expect(geo.gapComposerLayout).toBeLessThan(48);
    const blankRegion = geo.threadClientHeight - (geo.threadScrollHeight - geo.threadScrollTop);
    expect(blankRegion).toBeLessThan(during.surfacePaddingBottom);
    expect(blankRegion).toBeLessThan(80);
  });

  test("streaming follows live bottom after keyboard dismiss while pinned", async ({ page }) => {
    await openSessionChat(page);
    await seedTranscript(page, 30);
    await page.evaluate(() => {
      const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
      thread.scrollTop = thread.scrollHeight;
    });
    const composer = page.getByLabel("Message");
    await composer.click();
    await simulateKeyboard(page, 280);
    await page.getByTestId("session-thread").click({ position: { x: 40, y: 40 } });
    await page.waitForTimeout(400);
    await page.evaluate(() => {
      const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
      const p = document.createElement("p");
      p.className = "session-reply";
      p.textContent = `Streaming line: ${"chunk ".repeat(40)}`;
      p.setAttribute("data-testid", "session-message-agent");
      thread.appendChild(p);
    });
    await page.waitForTimeout(200);
    const geo = await readGeometry(page);
    expect(geo.surfacePaddingBottom).toBe(0);
    expect(geo.gapTranscriptComposer).toBeLessThan(8);
    expect(geo.threadScrollTop + geo.threadClientHeight).toBeGreaterThanOrEqual(
      geo.threadScrollHeight - 48,
    );
  });

  test("multiline composer growth while pinned keeps live edge", async ({ page }) => {
    await openSessionChat(page);
    await seedTranscript(page, 20);
    await page.evaluate(() => {
      const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
      thread.scrollTop = thread.scrollHeight;
    });
    const composer = page.getByLabel("Message");
    await composer.fill("line one\nline two\nline three\nline four");
    await page.waitForTimeout(200);
    const geo = await readGeometry(page);
    expect(geo.gapTranscriptComposer).toBeLessThan(8);
    expect(geo.threadScrollTop + geo.threadClientHeight).toBeGreaterThanOrEqual(
      geo.threadScrollHeight - 48,
    );
  });

  test("does not reserve surface padding when innerHeight shrinks (PWA path)", async ({ page }) => {
    await openSessionChat(page);
    await simulateKeyboard(page, 280, { innerHeightShrinks: true });
    const geo = await readGeometry(page);
    expect(geo.surfacePaddingBottom).toBe(0);
  });

  test("short transcript: composer docks to viewport bottom after keyboard dismiss", async ({
    page,
  }) => {
    await openSessionChat(page);
    await seedTranscript(page, 2);
    await simulateKeyboard(page, 260);
    await dismissKeyboard(page);
    const geo = await readGeometry(page);
    expect(geo.gapTranscriptComposer).toBeLessThan(8);
  });

  test("long transcript: no stale keyboard band after dismiss", async ({ page }) => {
    await openSessionChat(page);
    await seedTranscript(page, 50);
    await page.evaluate(() => {
      const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
      thread.scrollTop = thread.scrollHeight;
    });
    await simulateKeyboard(page, 300);
    await dismissKeyboard(page);
    const geo = await readGeometry(page);
    const blankRegion = geo.threadClientHeight - (geo.threadScrollHeight - geo.threadScrollTop);
    expect(blankRegion).toBeLessThan(80);
    expect(geo.surfacePaddingBottom).toBe(0);
  });
});

test.describe("Session chat home-indicator inset (#1034)", () => {
  test.beforeEach(async ({ page }) => {
    await emulateHomeIndicatorInset(page, 34);
  });

  test("textarea bottom is flush to composer at rest with emulated safe-area", async ({
    page,
  }) => {
    await openSessionChat(page);
    const geo = await readTextareaFlushGeometry(page);
    expect(geo.keyboardOpen).toBe(false);
    expect(geo.textareaComposerGap).toBeLessThan(4);
    expect(geo.textareaLayoutGap).toBeLessThan(8);
    expect(geo.composerPaddingBottom).toBeLessThan(4);
    expect(geo.textareaPaddingBottom).toBeLessThan(8);
  });

  test("keyboard open drops safe-area pad under the textarea row", async ({ page }) => {
    await openSessionChat(page);
    const composer = page.getByLabel("Message");
    await composer.click();
    await simulateKeyboard(page, 280);
    const geo = await readTextareaFlushGeometry(page);
    expect(geo.keyboardOpen).toBe(true);
    expect(geo.textareaPaddingBottom).toBeLessThan(8);
    expect(geo.textareaComposerGap).toBeLessThan(4);
  });

  test("tap-dismiss keeps textarea flush after keyboard with emulated safe-area", async ({
    page,
  }) => {
    await openSessionChat(page);
    const composer = page.getByLabel("Message");
    await composer.click();
    await simulateKeyboard(page, 300);
    await page.getByTestId("session-thread").click({ position: { x: 40, y: 40 } });
    await page.waitForTimeout(400);
    const geo = await readTextareaFlushGeometry(page);
    expect(geo.keyboardOpen).toBe(false);
    expect(geo.textareaComposerGap).toBeLessThan(4);
    expect(geo.textareaLayoutGap).toBeLessThan(8);
    expect(geo.composerPaddingBottom).toBeLessThan(4);
    expect(geo.textareaPaddingBottom).toBeLessThan(8);
  });

  test("hotbar action icons sit in a trailing cluster", async ({ page }) => {
    await openSessionChat(page);
    const layout = await page.evaluate(() => {
      const hotbar = document.querySelector(
        '[data-testid="session-composer-hotbar"]',
      ) as HTMLElement;
      const actions = document.querySelector(
        '[data-testid="session-composer-actions"]',
      ) as HTMLElement;
      const model = hotbar.querySelector(".session-composer-model") as HTMLElement | null;
      const attach = actions.querySelector(".session-composer-attach") as HTMLElement;
      const mic = actions.querySelector(".session-composer-mic") as HTMLElement;
      const send = actions.querySelector(".session-composer-send") as HTMLElement;
      const hotbarRect = hotbar.getBoundingClientRect();
      const centerX = (el: HTMLElement) => el.getBoundingClientRect().left + el.offsetWidth / 2;
      return {
        modelBeforeActions:
          model === null ||
          (actions.compareDocumentPosition(model) & Node.DOCUMENT_POSITION_PRECEDING) !== 0,
        actionsMarginLeft: getComputedStyle(actions).marginLeft,
        attachCenter: centerX(attach),
        micCenter: centerX(mic),
        sendCenter: centerX(send),
        hotbarMid: hotbarRect.left + hotbarRect.width / 2,
      };
    });
    expect(layout.modelBeforeActions).toBe(true);
    expect(parseFloat(layout.actionsMarginLeft)).toBeGreaterThan(0);
    expect(layout.attachCenter).toBeGreaterThan(layout.hotbarMid);
    expect(layout.micCenter).toBeGreaterThan(layout.attachCenter);
    expect(layout.sendCenter).toBeGreaterThan(layout.micCenter);
  });
});
