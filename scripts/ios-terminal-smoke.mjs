import { webkit, devices } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import path from "node:path";

const baseUrl = process.env.AJAX_SMOKE_URL ?? "https://ajax.mossyhome.net/";
const handle = process.env.AJAX_SMOKE_TASK ?? "ajax-cli/ajax-web-ui-ux";
const screenshotDir = process.env.AJAX_SMOKE_SCREENSHOTS ?? "/tmp/ajax-ios-terminal-smoke";
const accessId = process.env.CF_ACCESS_CLIENT_ID;
const accessSecret = process.env.CF_ACCESS_CLIENT_SECRET;
const sendControlKeys = process.env.AJAX_SMOKE_SEND_CONTROL_KEYS === "1";
const typeText = process.env.AJAX_SMOKE_TYPE_TEXT ?? " ";
const pressEnter = process.env.AJAX_SMOKE_PRESS_ENTER === "1";

if (!accessId || !accessSecret) {
  console.error("CF_ACCESS_CLIENT_ID and CF_ACCESS_CLIENT_SECRET are required.");
  process.exit(2);
}

await mkdir(screenshotDir, { recursive: true });

const browser = await webkit.launch({ headless: true });
const iphone = devices["iPhone 15"];
const context = await browser.newContext({
  ...iphone,
  extraHTTPHeaders: {
    "CF-Access-Client-Id": accessId,
    "CF-Access-Client-Secret": accessSecret,
  },
  ignoreHTTPSErrors: true,
});

const page = await context.newPage();
const logs = [];
const requestFailures = [];
const websocketEvents = [];

page.on("console", (msg) => logs.push(`${msg.type()}: ${msg.text()}`));
page.on("pageerror", (error) => logs.push(`pageerror: ${error.message}`));
page.on("requestfailed", (request) => {
  requestFailures.push(`${request.method()} ${request.url()} ${request.failure()?.errorText}`);
});
page.on("websocket", (socket) => {
  websocketEvents.push(`open ${socket.url()}`);
  socket.on("framesent", (frame) => websocketEvents.push(`sent ${String(frame.payload).slice(0, 80)}`));
  socket.on("framereceived", (frame) =>
    websocketEvents.push(`received ${String(frame.payload).slice(0, 80)}`),
  );
  socket.on("close", () => websocketEvents.push("close"));
});

const shot = async (name) => {
  await page.screenshot({ path: path.join(screenshotDir, `${name}.png`), fullPage: true });
};

const metrics = async () =>
  page.evaluate(() => {
    const rect = (selector) => {
      const element = document.querySelector(selector);
      if (!element) return null;
      const box = element.getBoundingClientRect();
      return {
        x: box.x,
        y: box.y,
        width: box.width,
        height: box.height,
        top: box.top,
        bottom: box.bottom,
        scrollWidth: element.scrollWidth,
        clientWidth: element.clientWidth,
        scrollHeight: element.scrollHeight,
        clientHeight: element.clientHeight,
      };
    };

    return {
      href: location.href,
      viewport: {
        innerWidth,
        innerHeight,
        visualWidth: window.visualViewport?.width ?? null,
        visualHeight: window.visualViewport?.height ?? null,
        visualOffsetTop: window.visualViewport?.offsetTop ?? null,
      },
      root: {
        className: document.documentElement.className,
        scrollTop: document.scrollingElement?.scrollTop ?? null,
        scrollWidth: document.documentElement.scrollWidth,
        clientWidth: document.documentElement.clientWidth,
        scrollHeight: document.documentElement.scrollHeight,
        clientHeight: document.documentElement.clientHeight,
      },
      body: {
        className: document.body.className,
        scrollWidth: document.body.scrollWidth,
        clientWidth: document.body.clientWidth,
        scrollHeight: document.body.scrollHeight,
        clientHeight: document.body.clientHeight,
      },
      taskDetail: rect(".task-detail"),
      terminalPanel: rect(".terminal-panel"),
      terminalHost: rect(".task-terminal-viewport"),
      xterm: rect(".xterm"),
      xtermScreen: rect(".xterm-screen"),
      xtermViewport: rect(".xterm-viewport"),
      active: {
        tag: document.activeElement?.tagName ?? null,
        className: String(document.activeElement?.className ?? ""),
      },
      text: document.body.innerText.slice(0, 2000),
    };
  });

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const scrollDocumentBy = async (x, y) => {
  await page.evaluate(
    ([scrollX, scrollY]) => {
      window.scrollBy(scrollX, scrollY);
    },
    [x, y],
  );
};

const scrollTerminalHistoryBy = async (y) => {
  await page.evaluate((deltaY) => {
    const viewport = document.querySelector(".xterm-viewport");
    if (viewport) viewport.scrollTop += deltaY;
  }, y);
};

try {
  await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 30_000 });
  await page.waitForTimeout(3_000);
  await shot("01-dashboard");

  assert(await page.locator("[data-handle]").first().isVisible(), "dashboard task cards did not render");

  await scrollDocumentBy(0, 700);
  await page.waitForTimeout(400);
  const dashboardAfterVerticalScroll = await metrics();
  assert(
    dashboardAfterVerticalScroll.root.scrollWidth <= dashboardAfterVerticalScroll.root.clientWidth + 1,
    "dashboard has horizontal page overflow",
  );

  await scrollDocumentBy(500, 0);
  await page.waitForTimeout(300);
  const dashboardAfterHorizontalScroll = await metrics();
  assert(
    (dashboardAfterHorizontalScroll.root.scrollTop ?? 0) >= 0,
    "dashboard horizontal gesture destabilized document scroll",
  );

  const taskCard = page.locator(`[data-handle="${handle}"]`).first();
  await taskCard.scrollIntoViewIfNeeded();
  await taskCard.locator("button").filter({ hasText: "Open" }).click();
  await page.waitForTimeout(4_000);
  await shot("02-task-open");

  const openMetrics = await metrics();
  assert(openMetrics.root.className.includes("ajax-task-open"), "task-open scroll lock class missing");
  assert(openMetrics.taskDetail, "task detail did not open");
  assert(openMetrics.terminalPanel, "terminal panel missing");
  assert(openMetrics.xterm, "xterm missing");
  assert(
    openMetrics.terminalPanel.top < 170,
    `terminal starts too low: ${openMetrics.terminalPanel.top}`,
  );
  assert(
    openMetrics.root.scrollHeight <= openMetrics.root.clientHeight + 1,
    "document should not be scrollable while task terminal is open",
  );
  assert(
    openMetrics.xterm.scrollWidth <= openMetrics.terminalHost.clientWidth + 24,
    `xterm overflows host: xterm ${openMetrics.xterm.scrollWidth}, host ${openMetrics.terminalHost.clientWidth}`,
  );

  await scrollTerminalHistoryBy(-900);
  await page.waitForTimeout(500);
  await shot("03-terminal-history-scroll");
  const historyMetrics = await metrics();
  assert((historyMetrics.root.scrollTop ?? 0) === 0, "terminal history scroll moved the page");

  const xtermBox = await page.locator(".xterm").boundingBox();
  assert(xtermBox, "xterm has no bounding box");
  await page.touchscreen.tap(xtermBox.x + xtermBox.width / 2, xtermBox.y + xtermBox.height / 2);
  await page.keyboard.type(typeText);
  if (pressEnter) await page.keyboard.press("Enter");
  await page.waitForTimeout(1_500);
  await shot("04-terminal-after-typing");

  const typedMetrics = await metrics();
  assert(typedMetrics.active.tag === "TEXTAREA", "terminal textarea did not retain focus after typing");
  assert(websocketEvents.some((event) => event.includes('"type":"input"')), "terminal input was not sent");

  for (const label of ["Esc", "Tab", "←", "↑", "↓", "→", "Ctrl"]) {
    const key = page
      .getByRole("toolbar", { name: "Terminal keys" })
      .getByRole("button", { name: label, exact: true });
    assert(await key.isVisible(), `${label} key is not visible`);
  }

  if (sendControlKeys) {
    for (const label of ["Esc", "Tab", "←", "↑", "↓", "→"]) {
      await page
        .getByRole("toolbar", { name: "Terminal keys" })
        .getByRole("button", { name: label, exact: true })
        .click();
      await page.waitForTimeout(100);
    }
  }

  for (const [label, actionId] of [
    ["Ship", "ship"],
    ["Drop", "drop"],
  ]) {
    const action = page.locator(`button.action[data-action="${actionId}"]`).first();
    assert(await action.isVisible(), `${label} action is not visible`);
    const requiresConfirmation = await action.evaluate(
      (button) =>
        button.getAttribute("data-destructive") === "true" ||
        button.textContent?.toLowerCase().includes("drop"),
    );
    if (requiresConfirmation) {
      await action.click();
      await page.waitForTimeout(250);
      assert(
        (await action.innerText()).toLowerCase().includes("confirm"),
        `${label} did not enter confirmation state`,
      );
      await page.locator(".xterm").click();
      await page.waitForTimeout(150);
    }
  }

  await page.getByRole("button", { name: /back/i }).click();
  await page.waitForTimeout(1_000);
  await shot("05-dashboard-after-back");
  const bottomNav = page.getByRole("navigation", { name: "Mobile navigation" });
  assert(
    await bottomNav.getByRole("button", { name: "New", exact: true }).isVisible(),
    "bottom New nav is not usable",
  );
  assert(
    await bottomNav.getByRole("button", { name: "Dashboard", exact: true }).isVisible(),
    "bottom Dashboard nav is not usable",
  );

  const finalMetrics = await metrics();
  assert(!finalMetrics.root.className.includes("ajax-task-open"), "scroll lock class remained after Back");
  assert(requestFailures.length === 0, `request failures: ${requestFailures.join("; ")}`);

  console.log(
    JSON.stringify(
      {
        ok: true,
        handle,
        screenshots: screenshotDir,
        metrics: {
          open: openMetrics,
          afterHistoryScroll: historyMetrics,
          afterTyping: typedMetrics,
          final: finalMetrics,
        },
        websocketEvents: websocketEvents.slice(0, 30),
        logs: logs.slice(-30),
      },
      null,
      2,
    ),
  );
} finally {
  await browser.close();
}
