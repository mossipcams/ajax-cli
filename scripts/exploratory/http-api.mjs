// Minimal loopback HTTP helpers for exploratory seeding and preflight.

import { execFileSync } from "node:child_process";
import { BASE_URL } from "./lib.mjs";

export function bootstrapBrowserSession(baseUrl = BASE_URL) {
  const headers = execFileSync(
    "curl",
    ["-sk", "-D", "-", "-o", "/dev/null", "-X", "POST", `${baseUrl}/api/session`],
    { encoding: "utf8", maxBuffer: 1024 * 1024 },
  );
  const match = headers.match(/set-cookie:\s*(ajax_browser_session=[^;\r\n]+)/i);
  if (!match) {
    throw new Error("failed to bootstrap browser session cookie from POST /api/session");
  }
  return match[1];
}

export function curlJson(method, path, { baseUrl = BASE_URL, cookie, body } = {}) {
  const args = ["-sk", "-w", "\n%{http_code}", "-X", method];
  if (cookie) args.push("-H", `Cookie: ${cookie}`);
  if (body !== undefined) {
    args.push("-H", "Content-Type: application/json", "-d", JSON.stringify(body));
  }
  args.push(`${baseUrl}${path}`);
  const out = execFileSync("curl", args, { encoding: "utf8", maxBuffer: 4 * 1024 * 1024 });
  const newline = out.lastIndexOf("\n");
  const status = Number(out.slice(newline + 1));
  const raw = out.slice(0, newline).trim();
  let json = {};
  if (raw) {
    try {
      json = JSON.parse(raw);
    } catch (error) {
      throw new Error(`invalid JSON from ${method} ${path}: ${error.message}`);
    }
  }
  return { status, json };
}

export function isApiSuccess({ status, json }) {
  return status >= 200 && status < 300 && json?.ok !== false;
}
