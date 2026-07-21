// Mobile WebKit smoke against the release-built Rust HTTPS server. API and
// terminal transport are mocked in-page; HTML/JS/CSS load from the real server.

import { test, expect } from "@playwright/test";
import { mockFetch, mockTerminalWebSocket, terminalSurface } from "./fixtures";

type AssetTrace = {
  url: string;
  pathname: string;
  search: string;
  cacheControl: string | null;
};

test("release rust server serves bare shell assets on task route", async ({ page, baseURL }, testInfo) => {
  test.skip(baseURL !== "https://127.0.0.1:18789", "requires the real Rust HTTPS server");
  const traces: AssetTrace[] = [];
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];

  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });
  page.on("response", async (response) => {
    const url = new URL(response.url());
    if (!["/app.js", "/app.css", "/terminal.js"].includes(url.pathname)) return;
    traces.push({
      url: response.url(),
      pathname: url.pathname,
      search: url.search,
      cacheControl: await response.headerValue("cache-control"),
    });
  });

  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/#/t/web%2Ffix-login");

  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 10_000 });
  await expect(terminalSurface(page)).toBeVisible({ timeout: 10_000 });

  await testInfo.attach("asset-requests", {
    body: JSON.stringify(traces, null, 2),
    contentType: "application/json",
  });

  const appJs = traces.filter((trace) => trace.pathname === "/app.js");
  const terminalJs = traces.filter((trace) => trace.pathname === "/terminal.js");
  const appCss = traces.filter((trace) => trace.pathname === "/app.css");

  expect(appJs).toHaveLength(1);
  expect(terminalJs).toHaveLength(1);
  expect(appJs[0]?.search).toBe("");
  expect(terminalJs[0]?.search).toBe("");
  expect(appCss.some((trace) => trace.search === "")).toBe(true);

  for (const trace of traces) {
    expect(trace.cacheControl).toBe("no-store");
    expect(trace.cacheControl ?? "").not.toContain("immutable");
  }

  for (const error of [...consoleErrors, ...pageErrors]) {
    expect(error).not.toMatch(/#321/);
    expect(error).not.toMatch(/NotFoundError/);
    expect(error).not.toMatch(/Incompatible server response/);
  }
});
