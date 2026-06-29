// Operator-flow smoke suite. API responses are mocked via addInitScript
// (overrides globalThis.fetch before the app boots) so these tests run
// without a live Rust server. They verify Svelte routing, rendering,
// polling, confirmation, and connection-recovery flows in a real browser.

import { test, expect, type Page } from "@playwright/test";

// ---- fixture data --------------------------------------------------------

const COCKPIT_FIXTURE = {
  backend: { authority: "host-native", control_enabled: true, warning: null },
  repos: { repos: [{ name: "web" }, { name: "api" }] },
  cards: [
    {
      id: "web/fix-login",
      qualified_handle: "web/fix-login",
      repo: "web",
      title: "Fix login",
      status: "waiting",
      status_explanation: "Waiting for review",
      actions: [
        { action: "review", label: "Review", destructive: false, confirmation_required: false },
        { action: "drop",   label: "Drop",   destructive: true,  confirmation_required: true  },
      ],
    },
    {
      id: "api/add-auth",
      qualified_handle: "api/add-auth",
      repo: "api",
      title: "Add auth",
      status: "running",
      status_explanation: null,
      actions: [],
    },
  ],
  inbox: { items: [{ task_handle: "web/fix-login", severity: 2 }] },
};

const DETAIL_FIXTURE = {
  qualified_handle: "web/fix-login",
  repo: "web",
  title: "Fix login",
  branch: "ajax/fix-login",
  base_branch: "main",
  worktree_path: "/repo/web/ajax-fix-login",
  tmux_session: "ajax-web-fix-login",
  lifecycle: "reviewable",
  agent: "codex",
  agent_status: "idle",
  status: "waiting",
  status_explanation: "Waiting for review",
  runtime_observation_error: null,
  actions: [
    { action: "review", label: "Review", destructive: false, confirmation_required: false },
    { action: "drop",   label: "Drop",   destructive: true,  confirmation_required: true  },
  ],
  live_status_kind: null,
  live_status_summary: null,
  agent_activity: null,
  git: { unpushed_commits: 1 },
  tmux: null,
  annotations: [],
  created_unix_secs: 1700000000,
  last_activity_unix_secs: 1700001000,
  agent_attempts: [],
};

const VERSION_A = { version: "0.20.5" };
const VERSION_B = { version: "0.21.0-new" };

// ---- fetch mock helper ---------------------------------------------------

async function mockFetch(page: Page, extra: Record<string, unknown> = {}) {
  const routes: Record<string, unknown> = {
    "/api/cockpit":    COCKPIT_FIXTURE,
    "/api/version":    VERSION_A,
    "/api/health":     { status: "ok" },
    "/api/operations": { cockpit: COCKPIT_FIXTURE, output: "ok", error: null },
    "/api/server/restart": {},
    "__detail__":      DETAIL_FIXTURE,
    ...extra,
  };

  await page.addInitScript((routeMap: Record<string, unknown>) => {
    const real = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;

      if (Object.prototype.hasOwnProperty.call(routeMap, path)) {
        return new Response(JSON.stringify(routeMap[path]), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (/^\/api\/tasks\/[^/]+\/pane$/.test(path)) {
        return new Response(
          JSON.stringify({ sequence: 0, lines: [], tmux_exists: false, state: null }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (/^\/api\/tasks\/[^/]+$/.test(path)) {
        return new Response(JSON.stringify(routeMap["__detail__"]), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (path.startsWith("/api/")) {
        return new Response(JSON.stringify({ error: "not found" }), {
          status: 404,
          headers: { "content-type": "application/json" },
        });
      }
      return real(input, init);
    };
  }, routes);
}

// ---- tests ---------------------------------------------------------------

// Task list rows show `qualified_handle`, not `title`. Inbox cards also show
// `status_explanation`. Use handles as stable selectors.

test("dashboard renders tasks from cockpit fixture", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");

  // Inbox card shows the handle and status_explanation
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  // Calm group shows api/add-auth handle in a task row
  await expect(page.getByText("api/add-auth")).toBeVisible();
});

test("project filter shows only matching repo tasks", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  // Click the "web" project pill
  await page.locator("button.project-pill").filter({ hasText: "web" }).first().click();

  await expect(page.getByText("web/fix-login")).toBeVisible();
  await expect(page.getByText("api/add-auth")).not.toBeVisible();
});

test("task detail renders server status and actions", async ({ page }) => {
  await mockFetch(page);
  // Use correct task hash prefix from routes.ts: #/t/
  await page.goto("/app.html#/t/web%2Ffix-login");

  await expect(page.getByText("Waiting for review")).toBeVisible({ timeout: 10_000 });
  await expect(page.locator("[data-action='review']")).toBeVisible();
});

test("non-destructive action completes without a second tap", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-action='review']")).toBeVisible({ timeout: 10_000 });

  await page.locator("[data-action='review']").click();

  // Operation mock returns the refreshed cockpit; task outlet stays visible
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 5_000 });
});

test("destructive action requires two taps to execute", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-action='drop']")).toBeVisible({ timeout: 10_000 });

  // First tap: enters confirming state
  await page.locator("[data-action='drop']").click();
  await expect(page.locator(".action.confirming")).toBeVisible({ timeout: 3_000 });

  // Second tap: executes
  await page.locator(".action.confirming").click();
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 5_000 });
});

test("connection error shows backend unreachable state", async ({ page }) => {
  // Override to throw on cockpit — other routes still work
  await page.addInitScript(() => {
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      if (url.includes("/api/cockpit")) throw new TypeError("Failed to fetch");
      return orig(input, init);
    };
  });

  await page.goto("/app.html");
  await expect(page.locator(".connection-status")).toContainText("unreachable", { timeout: 8_000 });

  // Tap Retry — cockpit is still failing, so still unreachable
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.locator(".connection-status")).toContainText("unreachable");
});

test("settings view renders restart and diagnostics controls", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");

  await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByRole("button", { name: /Restart/i })).toBeVisible();
  await expect(
    page.locator("[data-testid='outlet-settings']").getByRole("button", { name: /Diagnostics/i }).first()
  ).toBeVisible();
});

test("update banner appears when version changes between polls", async ({ page }) => {
  await page.addInitScript((versions: { a: unknown; b: unknown; cockpit: unknown }) => {
    let count = 0;
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;
      if (path === "/api/cockpit")
        return new Response(
          JSON.stringify(versions.cockpit),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      if (path === "/api/version") {
        count++;
        return new Response(
          JSON.stringify(count === 1 ? versions.a : versions.b),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (path.startsWith("/api/")) {
        return new Response("{}", { status: 200, headers: { "content-type": "application/json" } });
      }
      return orig(input, init);
    };
  }, { a: VERSION_A, b: VERSION_B, cockpit: COCKPIT_FIXTURE });

  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  // Trigger a second version check via focus event
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));

  await expect(page.locator(".update-banner")).not.toHaveAttribute("hidden", { timeout: 10_000 });
});
