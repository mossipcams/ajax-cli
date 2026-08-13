#!/usr/bin/env node
// Assert exploratory Playwright MCP is WebKit-only and WebKit is installable.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { exploratoryDir } from "./lib.mjs";

const FORBIDDEN_BROWSERS = new Set(["chromium", "chrome", "msedge", "firefox"]);

function browserArgValue(args) {
  const idx = args.indexOf("--browser");
  if (idx === -1) return null;
  return args[idx + 1] ?? null;
}

/**
 * Validate MCP config rules without launching a browser.
 * @param {object} mcp parsed mcp.json
 */
export function assertWebkitMcpConfig(mcp) {
  const playwright = mcp?.mcpServers?.playwright;
  if (!playwright) {
    throw new Error("mcp.json missing playwright MCP server");
  }

  const args = playwright.args ?? [];
  const browser = browserArgValue(args);
  if (browser !== "webkit") {
    throw new Error(
      `exploratory MCP --browser must be exactly webkit (got ${JSON.stringify(browser)})`,
    );
  }

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (FORBIDDEN_BROWSERS.has(arg)) {
      throw new Error(`exploratory MCP must not use browser ${arg}`);
    }
    if (arg === "--browser" && FORBIDDEN_BROWSERS.has(args[i + 1])) {
      throw new Error(`exploratory MCP is not WebKit-only: --browser ${args[i + 1]}`);
    }
  }

  if (args.includes("--no-sandbox")) {
    throw new Error("exploratory MCP must not include Chromium --no-sandbox flag");
  }
}

async function loadWebkit() {
  try {
    const mod = await import("@playwright/test");
    return mod.webkit;
  } catch {
    try {
      const mod = await import("playwright");
      return mod.webkit;
    } catch {
      throw new Error("cannot import playwright to resolve WebKit executable");
    }
  }
}

export async function assertWebkitExecutable() {
  const webkit = await loadWebkit();
  const executablePath = webkit.executablePath();
  if (!existsSync(executablePath)) {
    throw new Error(
      `WebKit executable missing at ${executablePath}; run npx playwright install --with-deps webkit`,
    );
  }
  return executablePath;
}

async function main() {
  const configOnly = process.argv.includes("--config-only");
  const mcpPath = join(exploratoryDir, "mcp.json");
  const mcp = JSON.parse(readFileSync(mcpPath, "utf8"));
  assertWebkitMcpConfig(mcp);
  if (!configOnly) {
    await assertWebkitExecutable();
  }
  console.log("ok");
}

const isMain =
  process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isMain) {
  main().catch((error) => {
    console.error(error.message ?? error);
    process.exit(1);
  });
}
