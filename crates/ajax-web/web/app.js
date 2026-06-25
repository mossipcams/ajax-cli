// Ajax Cockpit mobile operator client driven by canonical task cards.
const loadedAppVersion =
  document.querySelector('meta[name="ajax-app-version"]')?.content || null;
const inbox = document.getElementById("inbox");
const repos = document.getElementById("repos");
const projectNav = document.getElementById("project-nav");
const emptyState = document.getElementById("empty-state");
const statusLine = document.getElementById("status-line");
const updateBanner = document.getElementById("update-banner");
const connectionStatus = document.getElementById("connection-status");
const connectionLabel = document.getElementById("connection-label");
const connectionRetry = document.getElementById("connection-retry");
const connectionReload = document.getElementById("connection-reload");
const connectionCopyDiagnostics = document.getElementById("connection-copy-diagnostics");
const connectionHealthLink = document.getElementById("connection-health-link");
const newTaskRow = document.getElementById("new-task-row");
const newTaskRowLabel = document.getElementById("new-task-row-label");
const newTaskSheet = document.getElementById("new-task-sheet");
const newTaskForm = document.getElementById("new-task-form");
const newTaskRepo = document.getElementById("new-task-repo");
const newTaskTitle = document.getElementById("new-task-title-input");
const newTaskAgent = document.getElementById("new-task-agent");
const newTaskError = document.getElementById("new-task-error");
const detailContainer = document.getElementById("task-detail");
const settingsView = document.getElementById("settings-view");
const settingsLink = document.getElementById("settings-link");
const settingsBack = document.getElementById("settings-back");
const restartServerButton = document.getElementById("restart-server");
const restartStatus = document.getElementById("restart-status");
const runDiagnosticsButton = document.getElementById("run-diagnostics");
const copyDiagnosticsButton = document.getElementById("copy-diagnostics");
const diagnosticsOutput = document.getElementById("diagnostics-output");
const resultPanel = document.getElementById("result-panel");
const resultMessage = document.getElementById("result-message");
const resultOutput = document.getElementById("result-output");
const resultDismiss = document.getElementById("result-dismiss");
const bottomNav = document.getElementById("bottom-nav");

const REFRESH_INTERVAL_MS = 1000;
const CONFIRM_TIMEOUT_MS = 8000;
const RESULT_AUTO_DISMISS_MS = 12000;
const VERSION_POLL_MS = 30000;
const RESTART_POLL_MS = 500;
const RESTART_TIMEOUT_MS = 30000;
const OFFLINE_STATUS = "Offline — last known state";
const PANE_INTERVAL_DEFAULT_MS = 1000;
const PANE_INTERVAL_IDLE_MS = 4000;
const PANE_INTERVAL_UNCHANGED_MS = 2500;
const MAX_LOG_ENTRIES = 24;

const CONNECTION_STATES = {
  connected: "connected",
  checking: "checking",
  reconnecting: "reconnecting",
  disconnected: "disconnected",
  backendUnreachable: "backend unreachable",
  staleSession: "stale session",
};

let lastCockpit = null;
let lastFingerprint = null;
let lastSuccessfulConnectionAt = null;
let lastFetchError = null;
let lastFetchStatus = null;
let lastHealthResult = null;
let lastVersionResult = null;
let lastCockpitResult = null;
let serverVersion = null;
let refreshInFlight = false;
let detailHandle = null;
let detailInFlight = false;
let selectedProject = null;
/** @type {Map<string, { originalLabel: string, expiresAt: number, timer: ReturnType<typeof setTimeout> }>} */
const pendingConfirmByKey = new Map();

// INTERACT PANEL STATE ------------------------------------------------------
let lastDetailData = null;
let lastPaneData = null;
let paneSequence = 0;
let paneInFlight = false;
let paneTimer = null;
let paneAvailable = false;
let lastInteractKind = null;
// The detail view polls every second and the pane on its own cadence. We only
// rebuild the DOM when the rendered content actually changes — otherwise a full
// replaceChildren() every tick restarts animations and makes the view jitter.
let lastDetailFingerprint = null;
let lastPaneFingerprint = null;
// Survive re-renders so disclosures stay open once opened.
let terminalDetailsOpen = false;
let metaDetailsOpen = false;

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = text;
  return node;
}

function titleCase(value) {
  return value ? value.charAt(0).toUpperCase() + value.slice(1) : value;
}

const ACTION_LABELS = {
  "fix-ci": "Fix CI",
  "resolve-merge-conflicts": "Resolve conflicts",
};

function actionLabel(action, state) {
  if (state && state.label) return state.label;
  return ACTION_LABELS[action] || titleCase(action);
}

function requestId() {
  if (window.crypto && window.crypto.randomUUID) return window.crypto.randomUUID();
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function repoOf(handle) {
  const slash = handle.indexOf("/");
  return slash === -1 ? handle : handle.slice(0, slash);
}

function severityBucket(value) {
  if (value <= 2) return "high";
  if (value <= 3) return "medium";
  return "low";
}

function actionsForCard(card) {
  return card.actions || [];
}

function showResult(message, output, isError) {
  resultMessage.textContent = message || "";
  if (output && output.trim()) {
    resultOutput.textContent = output.trim();
    resultOutput.hidden = false;
  } else {
    resultOutput.hidden = true;
    resultOutput.textContent = "";
  }
  resultPanel.hidden = false;
  resultPanel.classList.toggle("is-error", Boolean(isError));
  clearTimeout(showResult.timer);
  showResult.timer = setTimeout(hideResult, RESULT_AUTO_DISMISS_MS);
}

function hideResult() {
  resultPanel.hidden = true;
  resultPanel.classList.remove("is-error");
}

resultDismiss.addEventListener("click", hideResult);

function setConnectionState(state, detail) {
  const label = CONNECTION_STATES[state] || state || CONNECTION_STATES.checking;
  if (connectionStatus) connectionStatus.dataset.state = label;
  if (connectionLabel) {
    connectionLabel.textContent = detail ? `${label}: ${detail}` : label;
  }
  document.body.classList.toggle("is-offline", label !== CONNECTION_STATES.connected);
}

function recordFetchResult(path, result) {
  if (result && result.ok) {
    lastSuccessfulConnectionAt = new Date().toISOString();
  }
  lastFetchStatus = result ? result.status : null;
  lastFetchError = result ? result.error : null;
  if (path === "/api/health") lastHealthResult = result;
  if (path === "/api/version") lastVersionResult = result;
  if (path === "/api/cockpit") lastCockpitResult = result;
}

function recordFetchFailure(path, error, status) {
  recordFetchResult(path, {
    ok: false,
    status: status == null ? null : status,
    error: error && error.message ? error.message : String(error),
    body: null,
  });
}

async function checkBackendHealth() {
  try {
    const response = await fetch("/api/health", { cache: "no-store" });
    const text = await response.text();
    const result = {
      ok: response.ok,
      status: response.status,
      error: response.ok ? null : `HTTP ${response.status}`,
      body: text.slice(0, 600),
    };
    recordFetchResult("/api/health", result);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    setConnectionState("connected");
    return true;
  } catch (error) {
    recordFetchFailure("/api/health", error, null);
    setConnectionState("backendUnreachable", lastFetchError);
    return false;
  }
}

async function forceBackendHealthCheck(reason) {
  setConnectionState(lastSuccessfulConnectionAt ? "reconnecting" : "checking", reason);
  const healthy = await checkBackendHealth();
  if (healthy) {
    refreshCurrentRoute({ forceHealth: true });
  }
  return healthy;
}

// LIST VIEW -----------------------------------------------------------------

// The backend owns status derivation. The browser only renders its four-state
// contract and the accompanying explanation.
const STATUS_META = {
  running: { tone: "running", label: "Running" },
  waiting: { tone: "waiting", label: "Waiting" },
  idle: { tone: "idle", label: "Idle" },
  error: { tone: "error", label: "Error" },
};

const STATUS_ORDER = ["running", "waiting", "error", "idle"];

function statusMeta(state) {
  const key = (state || "").toLowerCase();
  return STATUS_META[key] || STATUS_META.idle;
}

function statusRank(state) {
  const index = STATUS_ORDER.indexOf((state || "").toLowerCase());
  return index === -1 ? STATUS_ORDER.length : index;
}

function statusDot(tone) {
  const dot = el("span", `status-dot tone-${tone}`);
  dot.setAttribute("aria-hidden", "true");
  return dot;
}

function statusBadge(meta) {
  return el("span", `status-badge tone-${meta.tone}`, meta.label);
}

function sectionHead(title, count, options) {
  const head = el("div", options && options.attention ? "section-head attention" : "section-head");
  head.append(el("span", "section-head-title", title));
  if (count != null) head.append(el("span", "section-head-count", String(count)));
  return head;
}

function actionButton(action, handle, isPrimary) {
  const button = el(
    "button",
    isPrimary ? "action primary" : "action",
    actionLabel(action.action, action),
  );
  if (action.action === "fix-ci" || action.action === "resolve-merge-conflicts") {
    button.classList.add("remediation-action");
  }
  button.type = "button";
  button.dataset.action = action.action;
  button.dataset.task = handle;
  if (action.destructive) button.dataset.destructive = "true";
  if (action.confirmation_required) button.dataset.confirmRequired = "true";
  applyPendingConfirm(button);
  return button;
}

function renderProjectNav(data) {
  projectNav.replaceChildren();
  const repoNames = new Set();
  for (const card of data.cards || []) repoNames.add(repoOf(card.qualified_handle));
  for (const repo of (data.repos && data.repos.repos) || []) repoNames.add(repo.name);
  const sorted = [...repoNames].sort();
  if (!sorted.length) {
    projectNav.hidden = true;
    return;
  }
  projectNav.hidden = false;
  projectNav.append(el("span", "project-nav-label", "Projects"));

  const all = el("button", selectedProject ? "project-pill" : "project-pill is-active", "All");
  all.type = "button";
  all.dataset.project = "";
  projectNav.append(all);

  for (const name of sorted) {
    const pill = el(
      "button",
      selectedProject === name ? "project-pill is-active" : "project-pill",
      name,
    );
    pill.type = "button";
    pill.dataset.project = name;
    projectNav.append(pill);
  }
}

function cardMatchesProject(card) {
  if (!selectedProject) return true;
  return repoOf(card.qualified_handle) === selectedProject;
}

// Inbox cards carry the full weight: status, reason, and inline actions so the
// operator can clear the blocker in one tap. Tapping the card body (not a
// button) opens the detail view — "tap to learn more".
function inboxActionRow(card) {
  const actions = actionsForCard(card);
  const primary = actions[0];

  const row = el("div", "inbox-card-actions");
  if (primary) {
    row.append(actionButton(primary, card.qualified_handle, true));
  }
  const open = el("button", "action", "Open");
  open.type = "button";
  open.setAttribute("data-open-task", card.qualified_handle);
  row.append(open);
  for (const action of actions.slice(1)) {
    row.append(actionButton(action, card.qualified_handle, false));
  }
  return row;
}

function inboxCard(card, item) {
  const meta = statusMeta(card.status);
  const article = el("article", `inbox-card tone-${meta.tone}`);
  article.dataset.handle = card.qualified_handle;
  article.dataset.severity = severityBucket(item.severity || 999);

  const head = el("div", "inbox-card-head");
  head.append(statusDot(meta.tone));
  head.append(el("span", "inbox-card-handle", card.qualified_handle));
  head.append(statusBadge(meta));
  article.append(head);

  const reason = card.status_explanation;
  if (reason) article.append(el("p", "inbox-card-reason", reason));

  article.append(inboxActionRow(card));
  return article;
}

// Task rows are deliberately light: a status dot, the handle, an optional live
// sub-line, and the status label. The whole row is the open-detail target.
function taskRow(card) {
  const meta = statusMeta(card.status);
  const row = el("button", `task-row tone-${meta.tone}`);
  row.type = "button";
  row.dataset.handle = card.qualified_handle;
  row.setAttribute("data-open-task", card.qualified_handle);

  row.append(statusDot(meta.tone));

  const main = el("div", "task-row-main");
  main.append(el("span", "task-row-handle", card.qualified_handle));
  const sub = card.status_explanation;
  if (sub && sub.toLowerCase() !== meta.label.toLowerCase()) {
    main.append(el("span", "task-row-sub", sub));
  }
  row.append(main);

  row.append(el("span", "task-row-status", meta.label));
  row.append(el("span", "task-row-chevron", "›"));
  return row;
}

function renderInbox(data, cardsByHandle) {
  inbox.replaceChildren();
  const items = ((data.inbox && data.inbox.items) || [])
    .slice()
    .sort((a, b) => (a.severity || 999) - (b.severity || 999))
    .filter((item) => {
      const card = cardsByHandle.get(item.task_handle);
      return card && cardMatchesProject(card);
    });
  if (!items.length) return;
  inbox.append(sectionHead("Needs you", items.length, { attention: true }));
  const list = el("div", "inbox-list");
  for (const item of items) {
    list.append(inboxCard(cardsByHandle.get(item.task_handle), item));
  }
  inbox.append(list);
}

function renderTasks(data) {
  repos.replaceChildren();
  const inboxHandles = new Set(
    ((data.inbox && data.inbox.items) || []).map((item) => item.task_handle),
  );
  const visible = data.cards.filter(
    (card) => cardMatchesProject(card) && !inboxHandles.has(card.qualified_handle),
  );
  if (!visible.length) return;

  repos.append(sectionHead(selectedProject ? selectedProject : "Tasks", visible.length));

  const byRepo = new Map();
  for (const card of visible) {
    const repo = repoOf(card.qualified_handle);
    if (!byRepo.has(repo)) byRepo.set(repo, []);
    byRepo.get(repo).push(card);
  }

  const sortCards = (cards) =>
    cards
      .slice()
      .sort(
        (a, b) =>
          statusRank(a.status) - statusRank(b.status) ||
          a.qualified_handle.localeCompare(b.qualified_handle),
      );

  for (const repo of [...byRepo.keys()].sort()) {
    const block = el("section", "task-group");
    if (!selectedProject && byRepo.size > 1) {
      block.append(el("div", "task-group-title", repo));
    }
    const list = el("div", "task-list");
    for (const card of sortCards(byRepo.get(repo))) list.append(taskRow(card));
    block.append(list);
    repos.append(block);
  }
}

function summarize(data) {
  const visible = data.cards.filter((card) => cardMatchesProject(card));
  const total = visible.length;
  const attention = ((data.inbox && data.inbox.items) || []).filter((item) => {
    const card = data.cards.find((c) => c.qualified_handle === item.task_handle);
    return card && cardMatchesProject(card);
  }).length;
  if (!total) return selectedProject ? `${selectedProject}: all quiet` : "All quiet";
  const taskWord = total === 1 ? "task" : "tasks";
  if (!attention) {
    return selectedProject
      ? `${selectedProject}: ${total} ${taskWord}`
      : `${total} ${taskWord}`;
  }
  return selectedProject
    ? `${selectedProject}: ${attention} need attention`
    : `${total} ${taskWord} · ${attention} need attention`;
}

function actionStructureSignature(card) {
  return (card.actions || []).map((action) => [
    action.action,
    action.label,
    action.destructive,
    action.confirmation_required,
  ]);
}

function structureFingerprint(data) {
  const cards = data.cards.map((c) => [
    c.qualified_handle,
    JSON.stringify(actionStructureSignature(c)),
  ]);
  const items = (data.inbox && data.inbox.items) || [];
  return JSON.stringify({
    project: selectedProject,
    cards,
    inbox: items.map((item) => [item.task_handle, item.severity]),
  });
}

function cardSummaryText(card, inboxItem) {
  return card.status_explanation || card.title || "";
}

function updateLiveSummaries(data, cardsByHandle) {
  const inboxByHandle = new Map(
    ((data.inbox && data.inbox.items) || []).map((item) => [item.task_handle, item]),
  );
  for (const article of document.querySelectorAll(".inbox-card[data-handle]")) {
    const card = cardsByHandle.get(article.dataset.handle);
    if (!card) continue;
    const inboxItem = inboxByHandle.get(card.qualified_handle);
    const meta = statusMeta(card.status);
    article.className = `inbox-card tone-${meta.tone}`;
    if (inboxItem) article.dataset.severity = severityBucket(inboxItem.severity || 999);
    const dot = article.querySelector(".status-dot");
    if (dot) dot.className = `status-dot tone-${meta.tone}`;
    const badge = article.querySelector(".status-badge");
    if (badge) {
      badge.className = `status-badge tone-${meta.tone}`;
      badge.textContent = meta.label;
    }
    const reason = article.querySelector(".inbox-card-reason");
    if (reason) {
      const text = cardSummaryText(card, inboxItem);
      if (text) {
        reason.textContent = text;
        reason.hidden = false;
      } else {
        reason.hidden = true;
      }
    }
  }
  for (const row of document.querySelectorAll(".task-row[data-handle]")) {
    const card = cardsByHandle.get(row.dataset.handle);
    if (!card) continue;
    const meta = statusMeta(card.status);
    row.className = `task-row tone-${meta.tone}`;
    const dot = row.querySelector(".status-dot");
    if (dot) dot.className = `status-dot tone-${meta.tone}`;
    const status = row.querySelector(".task-row-status");
    if (status) status.textContent = meta.label;
    const sub = row.querySelector(".task-row-sub");
    if (sub) {
      const subText = card.status_explanation;
      if (subText && subText.toLowerCase() !== meta.label.toLowerCase()) {
        sub.textContent = subText;
        sub.hidden = false;
      } else {
        sub.hidden = true;
      }
    }
  }
}

function renderList(data) {
  renderProjectNav(data);
  const cardsByHandle = new Map(data.cards.map((card) => [card.qualified_handle, card]));
  renderInbox(data, cardsByHandle);
  renderTasks(data);
  const visibleCount = data.cards.filter((card) => cardMatchesProject(card)).length;
  emptyState.hidden = visibleCount > 0;
  emptyState.textContent = selectedProject
    ? `No tasks in ${selectedProject}`
    : "All quiet";
}

function applyData(data) {
  lastCockpit = data;
  const fp = structureFingerprint(data);
  const cardsByHandle = new Map(data.cards.map((card) => [card.qualified_handle, card]));
  if (fp !== lastFingerprint) {
    renderList(data);
    lastFingerprint = fp;
    document.body.classList.add("is-hydrated");
  } else {
    updateLiveSummaries(data, cardsByHandle);
  }
  statusLine.textContent = summarize(data);
  setOnline(true);
}

function setOnline(online) {
  setConnectionState(online ? "connected" : "disconnected");
  if (!online) {
    statusLine.textContent = OFFLINE_STATUS;
  }
}

async function loadCockpit() {
  if (refreshInFlight || document.hidden) return;
  refreshInFlight = true;
  try {
    const response = await fetch("/api/cockpit", { cache: "no-store" });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    recordFetchResult("/api/cockpit", {
      ok: true,
      status: response.status,
      error: null,
      body: "ok",
    });
    const data = await response.json();
    applyData(data);
  } catch (error) {
    recordFetchFailure("/api/cockpit", error, null);
    setOnline(false);
  } finally {
    refreshInFlight = false;
  }
}

// DETAIL VIEW ---------------------------------------------------------------

function renderDetail(detail) {
  lastDetailData = detail;
  // The panel below is rendered from the current pane snapshot; keep the pane
  // fingerprint in sync so the next pane tick doesn't redundantly re-render it.
  lastPaneFingerprint = paneFingerprint(lastPaneData);
  detailContainer.replaceChildren();

  const header = el("div", "detail-header");
  const back = el("button", "back", "← Back");
  back.type = "button";
  back.addEventListener("click", () => {
    window.location.hash = selectedProject ? `#/p/${encodeURIComponent(selectedProject)}` : "#/";
  });
  header.append(back);
  header.append(el("h1", "detail-title", detail.title || detail.qualified_handle));
  detailContainer.append(header);

  detailContainer.append(renderInteractPanel(detail, lastPaneData));

  const meta = document.createElement("details");
  meta.className = "meta-details";
  meta.open = metaDetailsOpen;
  meta.addEventListener("toggle", () => {
    metaDetailsOpen = meta.open;
  });
  const metaSummary = document.createElement("summary");
  metaSummary.textContent = "Task details";
  meta.append(metaSummary);

  meta.append(el("div", "meta-group-label", "Branch"));
  const gitGrid = el("dl", "detail-grid");
  appendCopyRow(gitGrid, "Branch", detail.branch);
  appendGridRow(gitGrid, "Base", detail.base_branch);
  appendCopyRow(gitGrid, "Worktree", detail.worktree_path);
  if (detail.git && detail.git.unpushed_commits) {
    appendGridRow(gitGrid, "Unpushed", String(detail.git.unpushed_commits));
  }
  meta.append(gitGrid);

  meta.append(el("div", "meta-group-label", "Agent"));
  const agentGrid = el("dl", "detail-grid");
  appendGridRow(agentGrid, "Client", detail.agent);
  appendGridRow(agentGrid, "Runtime", detail.agent_status);
  appendGridRow(agentGrid, "Tmux", detail.tmux_session);
  meta.append(agentGrid);

  detailContainer.append(meta);
}

function appendGridRow(grid, label, value) {
  if (value == null || value === "") return;
  grid.append(el("dt", null, label));
  grid.append(el("dd", null, String(value)));
}

function appendCopyRow(grid, label, value) {
  if (value == null || value === "") return;
  grid.append(el("dt", null, label));
  const dd = el("dd", "meta-copy-cell");
  dd.append(el("span", "meta-copy-value", String(value)));
  const copy = el("button", "meta-copy", "Copy");
  copy.type = "button";
  copy.dataset.copyValue = String(value);
  copy.setAttribute("aria-label", `Copy ${label.toLowerCase()}`);
  dd.append(copy);
  grid.append(dd);
}

async function loadDetail() {
  if (!detailHandle || detailInFlight || document.hidden) return;
  detailInFlight = true;
  try {
    const response = await fetch(`/api/tasks/${detailHandle}`, { cache: "no-store" });
    if (response.status === 404) {
      showResult("Task no longer exists", null, true);
      window.location.hash = "#/";
      return;
    }
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    recordFetchResult(`/api/tasks/${detailHandle}`, {
      ok: true,
      status: response.status,
      error: null,
      body: "ok",
    });
    const detail = await response.json();
    // Keep the cached copy fresh so the pane tick renders against current data,
    // but only rebuild the DOM when the structural fingerprint actually changes.
    lastDetailData = detail;
    const fp = detailFingerprint(detail);
    if (fp !== lastDetailFingerprint) {
      renderDetail(detail);
      lastDetailFingerprint = fp;
    } else {
      updateDetailLiveSummaries(detail);
    }
    setOnline(true);
  } catch (error) {
    recordFetchFailure(`/api/tasks/${detailHandle}`, error, null);
    setOnline(false);
  } finally {
    detailInFlight = false;
  }
}

// The skeleton of the detail view — header, the blocks the interact panel
// renders, and the meta disclosure. Live status text/tone are excluded; they
// update in place via updateDetailLiveSummaries so the view stays still.
function detailFingerprint(detail) {
  return JSON.stringify({
    title: detail.title,
    handle: detail.qualified_handle,
    branch: detail.branch,
    base: detail.base_branch,
    worktree: detail.worktree_path,
    unpushed: detail.git && detail.git.unpushed_commits,
    agent: detail.agent,
    agentStatus: detail.agent_status,
    tmux: detail.tmux_session,
    kind: detail.live_status_kind,
    actions: actionStructureSignature(detail),
  });
}

function updateDetailLiveSummaries(detail) {
  const meta = statusMeta(detail.status);
  const pill = detailContainer.querySelector(".interact-state.is-hero .interact-pill");
  if (pill) {
    const cls = `interact-pill tone-${meta.tone}`;
    if (pill.className !== cls) pill.className = cls;
    if (pill.textContent !== meta.label) pill.textContent = meta.label;
  }
  const summary = detailContainer.querySelector(".interact-state.is-hero .interact-summary");
  if (summary && detail.status_explanation && summary.textContent !== detail.status_explanation) {
    summary.textContent = detail.status_explanation;
  }
}

// SETTINGS ------------------------------------------------------------------

function isSettingsRoute() {
  return (window.location.hash || "#/") === "#/settings";
}

function showSettingsView() {
  settingsView.hidden = false;
}

function hideSettingsView() {
  settingsView.hidden = true;
  restartStatus.hidden = true;
  restartStatus.textContent = "";
}

async function waitForServerOnline() {
  const deadline = Date.now() + RESTART_TIMEOUT_MS;
  while (Date.now() < deadline) {
    try {
      const response = await fetch("/api/health", { cache: "no-store" });
      if (response.ok) return true;
    } catch (error) {
      // expected while the server is down
    }
    await new Promise((resolve) => setTimeout(resolve, RESTART_POLL_MS));
  }
  return false;
}

async function restartServer() {
  if (tryConfirmDestructive(restartServerButton)) return;

  restartServerButton.disabled = true;
  restartStatus.textContent = "Restarting…";
  restartStatus.hidden = false;
  try {
    const response = await fetch("/api/server/restart", {
      method: "POST",
      cache: "no-store",
    });
    const payload = await response.json().catch(() => ({}));
    if (!response.ok) {
      showResult(payload.error || "Restart failed", null, true);
      return;
    }
  } catch (error) {
    // connection drop during restart is expected
  }

  const online = await waitForServerOnline();
  if (online) {
    showResult("Server restarted", null, false);
    window.location.hash = "#/";
    loadCockpit();
  } else {
    showResult("Server did not come back in time", null, true);
  }
}

settingsLink.addEventListener("click", () => {
  window.location.hash = "#/settings";
});

settingsBack.addEventListener("click", () => {
  window.location.hash = "#/";
});

restartServerButton.addEventListener("click", () => {
  restartServer().finally(() => {
    restartServerButton.disabled = false;
    if (!isSettingsRoute()) restartStatus.hidden = true;
  });
});

async function diagnosticFetch(path) {
  try {
    const response = await fetch(path, { cache: "no-store" });
    const text = await response.text();
    let body = text.slice(0, 600);
    try {
      const parsed = JSON.parse(text);
      body = JSON.stringify(parsed, null, 2).slice(0, 600);
    } catch (error) {
      // Plain-text responses are still useful diagnostics.
    }
    const result = {
      ok: response.ok,
      status: response.status,
      error: null,
      body,
    };
    recordFetchResult(path, result);
    return result;
  } catch (error) {
    const result = {
      ok: false,
      status: null,
      error: error && error.message ? error.message : String(error),
      body: null,
    };
    recordFetchResult(path, result);
    return result;
  }
}

async function buildDiagnosticsReport() {
  const checks = {
    health: await diagnosticFetch("/api/health"),
    version: await diagnosticFetch("/api/version"),
    cockpit: await diagnosticFetch("/api/cockpit"),
  };
  if (detailHandle) {
    checks.task = await diagnosticFetch(`/api/tasks/${encodeURIComponent(detailHandle)}`);
  }

  serverVersion = serverVersionFromResult(checks.version);

  return {
    browser_mode: isStandalonePwa() ? "standalone" : "Safari/browser",
    backend_url: window.location.origin,
    navigator_onLine: navigator.onLine,
    app_version: loadedAppVersion,
    server_version: serverVersion,
    service_worker_controller: Boolean(
      navigator.serviceWorker && navigator.serviceWorker.controller,
    ),
    loaded_app_version: loadedAppVersion,
    location: window.location.href,
    last_successful_connection_at: lastSuccessfulConnectionAt,
    last_fetch_error: lastFetchError,
    last_fetch_status: lastFetchStatus,
    cached_results: {
      health: lastHealthResult,
      version: lastVersionResult,
      cockpit: lastCockpitResult,
    },
    checks,
  };
}

function serverVersionFromResult(result) {
  if (!result || !result.body) return null;
  try {
    const parsed = JSON.parse(result.body);
    return parsed.version || null;
  } catch (error) {
    return null;
  }
}

async function runDiagnostics() {
  runDiagnosticsButton.disabled = true;
  diagnosticsOutput.hidden = false;
  diagnosticsOutput.textContent = "Running diagnostics...";

  const report = await buildDiagnosticsReport();
  diagnosticsOutput.textContent = JSON.stringify(report, null, 2);
  runDiagnosticsButton.disabled = false;
  return report;
}

runDiagnosticsButton.addEventListener("click", runDiagnostics);

async function copyDiagnostics() {
  const report = await buildDiagnosticsReport();
  const text = JSON.stringify(report, null, 2);
  diagnosticsOutput.hidden = false;
  diagnosticsOutput.textContent = text;
  if (navigator.clipboard && navigator.clipboard.writeText) {
    await navigator.clipboard.writeText(text);
    showResult("Diagnostics copied", null, false);
  } else {
    showResult("Diagnostics ready to copy", null, false);
  }
}

copyDiagnosticsButton.addEventListener("click", () => {
  copyDiagnostics().catch(() => showResult("Could not copy diagnostics", null, true));
});

// INTERACT PANEL ------------------------------------------------------------

function interactCommand(detail, pane) {
  if (pane && pane.state && pane.state.command) return pane.state.command;
  if (detail && detail.live_status_kind === "WaitingForApproval" && detail.live_status_summary) {
    return detail.live_status_summary;
  }
  return null;
}

function interactPrompt(detail, pane) {
  if (pane && pane.state && pane.state.prompt) return pane.state.prompt;
  if (detail && detail.live_status_kind === "WaitingForInput" && detail.live_status_summary) {
    return detail.live_status_summary;
  }
  return null;
}

function renderInteractPanel(detail, pane) {
  const panel = el("section", "interact-panel");
  const tmuxMissing = pane && pane.tmux_exists === false;
  const kind = detail.live_status_kind || "Unknown";
  const meta = statusMeta(detail.status);
  lastInteractKind = kind;

  // Status hero: the one-line answer to "what is this task doing right now".
  const stateRow = el("div", "interact-state is-hero");
  const pill = el("span", `interact-pill tone-${meta.tone}`, meta.label);
  stateRow.append(pill);
  if (detail.status_explanation) {
    stateRow.append(el("span", "interact-summary", detail.status_explanation));
  }
  panel.append(stateRow);

  // Only render the decision surface when the task is actually blocked on you.
  const needs = renderNeedsFromYou(detail, pane, tmuxMissing);
  if (needs) panel.append(needs);

  const band = renderNextActionBand(detail);
  if (band) panel.append(band);

  // The escape hatch: this view is a glance, not a terminal — hand the operator
  // a real way into the tmux session for anything the web can't do.
  panel.append(renderEscapeHatch(detail, pane, tmuxMissing));

  panel.append(renderTerminalDetails(detail, pane, tmuxMissing));

  return panel;
}

function renderNeedsFromYou(detail, pane, tmuxMissing) {
  const kind = detail.live_status_kind;

  if (tmuxMissing) {
    const wrap = el("section", "needs-block");
    wrap.append(el("div", "interact-card-label", "Needs from you"));
    wrap.append(el("p", "interact-card-body", "Tmux session is missing. Sync the task to recover."));
    return wrap;
  }

  const command = interactCommand(detail, pane);
  if (kind === "WaitingForApproval" && command) {
    const wrap = el("section", "needs-block");
    wrap.append(el("div", "interact-card-label", "Needs from you"));
    wrap.append(el("p", "interact-card-body", "The agent is blocked on an approval decision."));
    wrap.append(el("code", "interact-card-body", command));
    if (pane && pane.state && pane.state.answerable && pane.state.fingerprint) {
      const actions = el("div", "interact-card-actions");
      const approve = el("button", "pill is-primary", "Approve");
      approve.type = "button";
      approve.addEventListener("click", () => sendAnswer("approve", pane.state.fingerprint));
      const deny = el("button", "pill is-danger", "Deny");
      deny.type = "button";
      deny.addEventListener("click", () => sendAnswer("deny", pane.state.fingerprint));
      actions.append(approve);
      actions.append(deny);
      wrap.append(actions);
    } else {
      wrap.append(el("p", "interact-hint", "Open the terminal below for this approval."));
    }
    return wrap;
  }

  const prompt = interactPrompt(detail, pane);
  if (kind === "WaitingForInput") {
    const wrap = el("section", "needs-block");
    wrap.append(el("div", "interact-card-label", "Needs from you"));
    if (prompt) wrap.append(el("p", "interact-card-body", prompt));
    wrap.append(el("p", "interact-hint", "Open the terminal below for free-form replies."));
    return wrap;
  }

  // Nothing is blocking — render nothing rather than a filler card.
  return null;
}

function renderEscapeHatch(detail, pane, tmuxMissing) {
  const block = el("section", "escape-hatch");
  block.append(el("div", "interact-card-label", "Open in terminal"));

  const session = detail.tmux_session;
  if (tmuxMissing || !session) {
    block.append(
      el("p", "interact-card-body", "Terminal session unavailable — sync the task to recover."),
    );
    return block;
  }

  block.append(
    el("p", "escape-hatch-hint", "Continue this task in your SSH session — tap to copy the command."),
  );

  const command = `tmux attach -t ${session}`;
  const row = el("div", "escape-hatch-row");

  const open = el("button", "pill is-primary", "Open in tmux");
  open.type = "button";
  open.dataset.copyValue = command;
  open.setAttribute("aria-label", "Copy tmux attach command");
  row.append(open);

  const copyOut = el("button", "pill", "Copy output");
  copyOut.type = "button";
  copyOut.dataset.terminalShortcut = "Copy visible output";
  row.append(copyOut);

  const copyErr = el("button", "pill", "Copy last error");
  copyErr.type = "button";
  copyErr.dataset.terminalShortcut = "Copy last error";
  row.append(copyErr);

  block.append(row);
  block.append(el("code", "escape-hatch-cmd", command));
  return block;
}

function renderNextActionBand(detail) {
  const actions = actionsForCard(detail);
  if (!actions.length) return null;

  const band = el("section", "next-action");
  band.append(el("div", "interact-card-label", "Next action"));

  const primary = actions[0];
  band.append(el("p", "next-action-hint", nextStepMessage(detail, primary)));

  const row = el("div", "next-action-row");
  for (const [index, action] of actions.entries()) {
    row.append(actionButton(action, detail.qualified_handle, index === 0));
  }
  band.append(row);

  return band;
}

function nextStepMessage(detail, primary) {
  switch (detail.live_status_kind) {
    case "WaitingForApproval":
      return "Clear the approval request, then let the task continue.";
    case "WaitingForInput":
      return "Open the terminal to reply directly to the agent.";
    case "CiFailed":
      return "Inspect the failing check and run Fix CI if you want Ajax to remediate it.";
    case "MergeConflict":
      return "Run Resolve conflicts to repair the branch before reviewing or shipping.";
    default:
      if (primary && primary.status === "supported") {
        return `Use ${actionLabel(primary.action, primary)} when you're ready to move this task forward.`;
      }
      return "Monitor the task health and use the action drawer when intervention is needed.";
  }
}

function renderTerminalDetails(detail, pane, tmuxMissing) {
  const details = document.createElement("details");
  details.className = "terminal-details";
  details.open = terminalDetailsOpen;
  details.addEventListener("toggle", () => {
    terminalDetailsOpen = details.open;
  });
  const summary = document.createElement("summary");
  summary.textContent = "View terminal output";
  details.append(summary);

  if (tmuxMissing) {
    details.append(el("p", "interact-hint", "Terminal session is unavailable for this task."));
    return details;
  }

  const pre = el("pre", "activity-excerpt");
  if (pane && Array.isArray(pane.lines) && pane.lines.length) {
    pre.textContent = pane.lines.join("\n");
  } else if (detail.agent_activity) {
    pre.textContent = detail.agent_activity;
  } else {
    pre.textContent = "No live pane snapshot available.";
  }
  details.append(pre);
  return details;
}

function visiblePaneOutput() {
  if (lastPaneData && Array.isArray(lastPaneData.lines)) {
    return lastPaneData.lines.join("\n");
  }
  return lastDetailData && lastDetailData.agent_activity ? lastDetailData.agent_activity : "";
}

function lastErrorOutput() {
  return visiblePaneOutput()
    .split("\n")
    .reverse()
    .find((line) => /error|failed|panic|conflict/i.test(line)) || "";
}

async function copyTextResult(text, success, empty) {
  if (!text) {
    showResult(empty, null, true);
    return;
  }
  if (navigator.clipboard && navigator.clipboard.writeText) {
    await navigator.clipboard.writeText(text);
    showResult(success, null, false);
  } else {
    showResult(success, text, false);
  }
}

async function runTerminalShortcut(label) {
  switch (label) {
    case "Copy visible output":
      await copyTextResult(visiblePaneOutput(), "Visible output copied", "No visible output");
      return;
    case "Copy last error":
      await copyTextResult(lastErrorOutput(), "Last error copied", "No visible error found");
      return;
    default:
      showResult(`${label} is unavailable here — open the terminal`, null, true);
  }
}

// Everything in the interact panel that the pane data can change. When this is
// unchanged between polls we skip the re-render so the panel doesn't flicker.
function paneFingerprint(pane) {
  if (!pane) return "null";
  const state = pane.state || {};
  return JSON.stringify({
    tmux: pane.tmux_exists,
    lines: Array.isArray(pane.lines) ? pane.lines : [],
    kind: state.kind,
    command: state.command,
    prompt: state.prompt,
    answerable: state.answerable,
    fingerprint: state.fingerprint,
  });
}

function refreshInteractPanelFromPane() {
  if (!lastDetailData) return;
  const fp = paneFingerprint(lastPaneData);
  if (fp === lastPaneFingerprint) return;
  lastPaneFingerprint = fp;
  renderInteractPanelInto(lastDetailData, lastPaneData);
}

function renderInteractPanelInto(detail, pane) {
  const existing = detailContainer.querySelector(".interact-panel");
  if (!existing) return;
  existing.replaceWith(renderInteractPanel(detail, pane));
}

async function sendAnswer(answer, fingerprint) {
  if (!detailHandle) return;
  if (!fingerprint) {
    showResult("This approval is no longer current — refresh the task", null, true);
    return;
  }
  try {
    const response = await fetch(`/api/tasks/${encodeURIComponent(detailHandle)}/answer`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ answer, fingerprint, request_id: requestId() }),
    });
    if (response.status === 429) {
      showResult("Slow down — too many actions in a short window", null, true);
      return;
    }
    if (response.status === 409) {
      showResult("The agent moved on before this approval was sent", null, true);
      schedulePaneTick(true);
      return;
    }
    if (response.status === 422) {
      showResult("This prompt needs the terminal instead of the dashboard", null, true);
      return;
    }
    if (!response.ok) {
      showResult(`Could not send answer (HTTP ${response.status})`, null, true);
      return;
    }
    schedulePaneTick(true);
  } catch (error) {
    showResult("Could not send answer — network error", null, true);
  }
}

function paneInterval() {
  if (document.hidden) return PANE_INTERVAL_IDLE_MS;
  const stateKind = lastPaneData?.state?.kind;
  if (!stateKind) return PANE_INTERVAL_DEFAULT_MS;
  if (
    stateKind === "WaitingForApproval" ||
    stateKind === "WaitingForInput" ||
    stateKind === "AgentRunning"
  ) {
    return PANE_INTERVAL_DEFAULT_MS;
  }
  if (stateKind === "Done" || stateKind === "Idle") {
    return PANE_INTERVAL_IDLE_MS;
  }
  return PANE_INTERVAL_UNCHANGED_MS;
}

function clearPaneTimer() {
  if (paneTimer) {
    clearTimeout(paneTimer);
    paneTimer = null;
  }
}

function schedulePaneTick(immediate) {
  clearPaneTimer();
  if (!detailHandle || document.hidden) return;
  if (immediate) {
    paneTimer = setTimeout(loadPane, 0);
  } else {
    paneTimer = setTimeout(loadPane, paneInterval());
  }
}

async function loadPane() {
  paneTimer = null;
  if (!detailHandle || paneInFlight || document.hidden) {
    schedulePaneTick(false);
    return;
  }
  paneInFlight = true;
  try {
    const url = `/api/tasks/${encodeURIComponent(detailHandle)}/pane?since=${paneSequence}`;
    const response = await fetch(url, { cache: "no-store" });
    if (response.status === 404) {
      // Endpoint not yet wired (backend pending) or task not found — degrade silently
      paneAvailable = false;
      lastPaneData = null;
      return;
    }
    if (response.status === 409) {
      const data = await response.json().catch(() => ({}));
      paneAvailable = true;
      lastPaneData = { sequence: paneSequence, lines: [], tmux_exists: false, state: null, ...data };
      refreshInteractPanelFromPane();
      setOnline(true);
      return;
    }
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const data = await response.json();
    const incomingSeq = typeof data.sequence === "number" ? data.sequence : paneSequence;
    const hasNewLines = Array.isArray(data.lines) && data.lines.length > 0;
    if (incomingSeq > paneSequence && hasNewLines) {
      // merge new lines into the cached buffer up to MAX_LOG_ENTRIES
      const previous = lastPaneData && Array.isArray(lastPaneData.lines) ? lastPaneData.lines : [];
      const merged = previous.concat(data.lines).slice(-MAX_LOG_ENTRIES);
      lastPaneData = { ...data, lines: merged };
      paneSequence = incomingSeq;
    } else if (incomingSeq >= paneSequence) {
      // unchanged delta — keep existing buffer but refresh state hints
      lastPaneData = lastPaneData
        ? { ...lastPaneData, ...data, lines: lastPaneData.lines }
        : { ...data, lines: Array.isArray(data.lines) ? data.lines : [] };
      paneSequence = incomingSeq;
    }
    paneAvailable = true;
    refreshInteractPanelFromPane();
    setOnline(true);
  } catch (error) {
    setOnline(false);
  } finally {
    paneInFlight = false;
    schedulePaneTick(false);
  }
}

function resetInteractState() {
  clearPaneTimer();
  lastPaneData = null;
  paneSequence = 0;
  paneAvailable = false;
  lastInteractKind = null;
  lastDetailData = null;
  lastDetailFingerprint = null;
  lastPaneFingerprint = null;
}

// HASH ROUTER ---------------------------------------------------------------

function updateNewTaskRowLabel() {
  newTaskRowLabel.textContent = selectedProject
    ? `New task in ${selectedProject}`
    : "New task";
}

function applyRoute() {
  const hash = window.location.hash || "#/";
  document.body.classList.remove("view-detail", "view-settings");
  hideSettingsView();

  if (hash === "#/settings") {
    if (detailHandle) resetInteractState();
    detailHandle = null;
    document.body.classList.add("view-settings");
    showSettingsView();
    return;
  }
  if (hash.startsWith("#/t/")) {
    const incoming = decodeURIComponent(hash.slice("#/t/".length));
    if (incoming !== detailHandle) resetInteractState();
    detailHandle = incoming;
    document.body.classList.add("view-detail");
    loadDetail();
    schedulePaneTick(true);
    return;
  }
  if (hash.startsWith("#/p/")) {
    selectedProject = decodeURIComponent(hash.slice("#/p/".length)) || null;
    if (detailHandle) resetInteractState();
    detailHandle = null;
    lastFingerprint = null;
    updateNewTaskRowLabel();
    if (lastCockpit) applyData(lastCockpit);
    else loadCockpit();
    return;
  }
  if (detailHandle) resetInteractState();
  selectedProject = null;
  detailHandle = null;
  updateNewTaskRowLabel();
  loadCockpit();
}

window.addEventListener("hashchange", applyRoute);

projectNav.addEventListener("click", (event) => {
  const pill = event.target.closest(".project-pill");
  if (!pill) return;
  const project = pill.dataset.project || "";
  window.location.hash = project ? `#/p/${encodeURIComponent(project)}` : "#/";
});

// SHEETS --------------------------------------------------------------------

function openNewTaskSheet() {
  populateRepoOptions();
  newTaskTitle.value = "";
  newTaskError.hidden = true;
  newTaskError.textContent = "";
  document.body.classList.add("sheet-open");
  setTimeout(() => newTaskTitle.focus(), 60);
}

function closeSheets() {
  document.body.classList.remove("sheet-open");
}

function populateRepoOptions() {
  const repoList =
    lastCockpit && lastCockpit.repos && lastCockpit.repos.repos
      ? lastCockpit.repos.repos
      : [];
  newTaskRepo.replaceChildren();
  if (!repoList.length) {
    const opt = document.createElement("option");
    opt.value = "";
    opt.textContent = "No repositories configured";
    opt.disabled = true;
    newTaskRepo.append(opt);
    return;
  }
  for (const repo of repoList) {
    const opt = document.createElement("option");
    opt.value = repo.name;
    opt.textContent = repo.name;
    if (selectedProject && repo.name === selectedProject) opt.selected = true;
    newTaskRepo.append(opt);
  }
}

async function submitNewTask(event) {
  event.preventDefault();
  const repo = newTaskRepo.value;
  const title = newTaskTitle.value.trim();
  const agent = newTaskAgent.value;
  if (!repo) {
    newTaskError.textContent = "Pick a repository first";
    newTaskError.hidden = false;
    return;
  }
  if (!title) {
    newTaskError.textContent = "Add a title";
    newTaskError.hidden = false;
    return;
  }
  const submit = newTaskForm.querySelector('button[type="submit"]');
  submit.disabled = true;
  try {
    const response = await fetch("/api/tasks", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ repo, title, agent, request_id: requestId() }),
    });
    const payload = await response.json().catch(() => ({}));
    if (!response.ok) {
      newTaskError.textContent = payload.error || "Action failed";
      newTaskError.hidden = false;
      if (payload.cockpit) applyData(payload.cockpit);
      showResult(payload.error || "Could not start task", payload.output, true);
      return;
    }
    if (payload.cockpit) applyData(payload.cockpit);
    showResult("Task started", payload.output, false);
    closeSheets();
  } catch (error) {
    newTaskError.textContent = "Action failed — network error";
    newTaskError.hidden = false;
    showResult("Could not start task", null, true);
  } finally {
    submit.disabled = false;
  }
}

newTaskRow.addEventListener("click", openNewTaskSheet);
newTaskForm.addEventListener("submit", submitNewTask);
newTaskSheet.addEventListener("click", (event) => {
  if (event.target === newTaskSheet) closeSheets();
});
document.querySelectorAll("[data-sheet-cancel]").forEach((btn) => {
  btn.addEventListener("click", closeSheets);
});

// ACTIONS -------------------------------------------------------------------

function confirmKey(handle, action) {
  return `${handle}:${action}`;
}

function clearPendingConfirm(key) {
  const entry = pendingConfirmByKey.get(key);
  if (entry?.timer) clearTimeout(entry.timer);
  pendingConfirmByKey.delete(key);
}

function resetConfirmButton(button) {
  const key = confirmKey(button.dataset.task, button.dataset.action);
  clearPendingConfirm(key);
  button.classList.remove("confirming");
  if (button.dataset.originalLabel) button.textContent = button.dataset.originalLabel;
}

function applyPendingConfirm(button) {
  if (!button.dataset.destructive && !button.dataset.confirmRequired) return;
  const key = confirmKey(button.dataset.task, button.dataset.action);
  const entry = pendingConfirmByKey.get(key);
  if (!entry || Date.now() > entry.expiresAt) {
    if (entry) clearPendingConfirm(key);
    return;
  }
  button.dataset.originalLabel = entry.originalLabel;
  button.textContent = "Tap to confirm";
  button.classList.add("confirming");
}

function beginPendingConfirm(button) {
  const key = confirmKey(button.dataset.task, button.dataset.action);
  const originalLabel = button.textContent;
  clearPendingConfirm(key);
  const expiresAt = Date.now() + CONFIRM_TIMEOUT_MS;
  const timer = setTimeout(() => {
    clearPendingConfirm(key);
    if (button.isConnected) resetConfirmButton(button);
  }, CONFIRM_TIMEOUT_MS);
  pendingConfirmByKey.set(key, { originalLabel, expiresAt, timer });
  button.dataset.originalLabel = originalLabel;
  button.textContent = "Tap to confirm";
  button.classList.add("confirming");
}

function tryConfirmDestructive(button) {
  if (!button.dataset.destructive && !button.dataset.confirmRequired) return false;
  const key = confirmKey(button.dataset.task, button.dataset.action);
  if (button.classList.contains("confirming")) {
    clearPendingConfirm(key);
    button.classList.remove("confirming");
    if (button.dataset.originalLabel) button.textContent = button.dataset.originalLabel;
    return false;
  }
  beginPendingConfirm(button);
  return true;
}

async function runAction(button) {
  resetConfirmButton(button);
  const cardEl = button.closest(".inbox-card, #task-detail");
  const peers = cardEl
    ? Array.from(cardEl.querySelectorAll("button[data-action]:not([disabled])"))
    : [button];
  const originalLabel = button.dataset.originalLabel || button.textContent;
  button.textContent = `${originalLabel} …`;
  button.classList.add("is-running");
  for (const peer of peers) peer.disabled = true;
  try {
    const response = await fetch("/api/operations", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        task_handle: button.dataset.task,
        action: button.dataset.action,
        request_id: requestId(),
      }),
    });
    const payload = await response.json().catch(() => ({}));
    if (payload.cockpit) applyData(payload.cockpit);
    else await loadCockpit();
    if (!response.ok) {
      showResult(payload.error || `Action failed (HTTP ${response.status})`, payload.output, true);
    } else {
      const label = titleCase(button.dataset.action);
      showResult(`${label} completed`, payload.output, false);
      if (detailHandle) loadDetail();
    }
  } catch (error) {
    showResult("Action failed — network error", null, true);
  } finally {
    if (button.isConnected) {
      button.textContent = originalLabel;
      button.classList.remove("is-running");
    }
    for (const peer of peers) {
      if (peer.isConnected && !peer.classList.contains("is-disabled")) peer.disabled = false;
    }
  }
}

document.addEventListener("click", (event) => {
  const shortcut = event.target.closest("[data-terminal-shortcut]");
  if (shortcut) {
    runTerminalShortcut(shortcut.dataset.terminalShortcut);
    return;
  }
  const copyButton = event.target.closest("[data-copy-value]");
  if (copyButton) {
    copyTextResult(copyButton.dataset.copyValue, "Copied", "Nothing to copy");
    return;
  }
  const openTask = event.target.closest("[data-open-task]");
  if (openTask) {
    window.location.hash = `#/t/${encodeURIComponent(openTask.getAttribute("data-open-task"))}`;
    return;
  }
  const button = event.target.closest("button[data-action]");
  if (button) {
    if (button.disabled) return;
    if (tryConfirmDestructive(button)) return;
    runAction(button);
    return;
  }
  const inboxCardEl = event.target.closest(".inbox-card");
  if (inboxCardEl && !event.target.closest("button")) {
    const handle = inboxCardEl.dataset.handle;
    if (handle) {
      window.location.hash = `#/t/${encodeURIComponent(handle)}`;
    }
  }
});

bottomNav.addEventListener("click", (event) => {
  const routeButton = event.target.closest("[data-bottom-route]");
  if (routeButton) {
    window.location.hash = routeButton.getAttribute("data-bottom-route");
    return;
  }
  const actionButton = event.target.closest("[data-bottom-action=\"new-task\"]");
  if (actionButton) openNewTaskSheet();
});

function refreshCurrentRoute(options) {
  if (isSettingsRoute()) return;
  const forceHealth = options && options.forceHealth;
  if (forceHealth) setConnectionState("checking", "refreshing cockpit");
  if (detailHandle) {
    loadDetail();
    schedulePaneTick(true);
  } else {
    loadCockpit();
  }
}

function refreshAfterResume(reason) {
  checkForUpdate(true);
  forceBackendHealthCheck(reason);
}

window.addEventListener("online", () => forceBackendHealthCheck("online"));
window.addEventListener("offline", () => setOnline(false));
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    if (!isSettingsRoute()) clearPaneTimer();
    return;
  }
  checkForUpdate(true);
  forceBackendHealthCheck("visibilitychange");
});
window.addEventListener("pageshow", () => {
  checkForUpdate(true);
  forceBackendHealthCheck("pageshow");
});
window.addEventListener("focus", () => {
  checkForUpdate(true);
  forceBackendHealthCheck("focus");
});

// BROWSER MODE --------------------------------------------------------------

function isStandalonePwa() {
  return (
    window.matchMedia("(display-mode: standalone)").matches ||
    window.navigator.standalone === true
  );
}

// SHELL UPDATES -------------------------------------------------------------
// An installed iOS standalone PWA resumes a frozen snapshot and has no reload
// affordance, so a redeployed shell would run stale forever (the symptom that
// looks like "updates need re-adding to the Home Screen"). We pin the version
// the app booted with and, whenever the server reports a different one, surface
// a tap-to-reload banner. The foreground check is the load-bearing path: it
// runs the moment the app is brought back to the front, which is exactly when a
// frozen snapshot would otherwise show old code.
let loadedVersion = loadedAppVersion;
let lastVersionCheck = 0;

async function checkForUpdate(force) {
  const now = Date.now();
  if (!force && now - lastVersionCheck < VERSION_POLL_MS) return;
  lastVersionCheck = now;
  try {
    const response = await fetch("/api/version", { cache: "no-store" });
    if (!response.ok) return;
    const data = await response.json();
    const version = data && data.version;
    if (!version) return;
    serverVersion = version;
    recordFetchResult("/api/version", {
      ok: true,
      status: response.status,
      error: null,
      body: JSON.stringify(data),
    });
    if (loadedVersion === null) {
      loadedVersion = version;
    } else if (version !== loadedVersion) {
      updateBanner.hidden = false;
    }
  } catch (error) {
    recordFetchFailure("/api/version", error, null);
    // Offline or unreachable: leave the current version pinned and retry later.
  }
}

if (updateBanner) {
  updateBanner.addEventListener("click", () => {
    window.location.reload();
  });
}

connectionRetry.addEventListener("click", () => {
  forceBackendHealthCheck("manual retry");
});

connectionReload.addEventListener("click", () => {
  window.location.reload();
});

connectionCopyDiagnostics.addEventListener("click", () => {
  copyDiagnostics().catch(() => showResult("Could not copy diagnostics", null, true));
});

if (connectionHealthLink) connectionHealthLink.href = "/api/health";

function unregisterExistingServiceWorkers() {
  if (!("serviceWorker" in navigator)) return;
  navigator.serviceWorker
    .getRegistrations()
    .then((registrations) =>
      Promise.all(registrations.map((registration) => registration.unregister())),
    )
    .catch((error) => {
      console.warn("service worker cleanup failed", error);
    });
}

unregisterExistingServiceWorkers();

checkForUpdate(true);

if (!navigator.onLine) setOnline(false);

setInterval(() => {
  checkForUpdate(false);
  if (isSettingsRoute()) return;
  if (detailHandle) loadDetail();
  else loadCockpit();
}, REFRESH_INTERVAL_MS);

applyRoute();
forceBackendHealthCheck("initial");
