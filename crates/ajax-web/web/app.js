// Ajax Cockpit — mobile operator client driven by server action_states.
const inbox = document.getElementById("inbox");
const repos = document.getElementById("repos");
const projectNav = document.getElementById("project-nav");
const emptyState = document.getElementById("empty-state");
const statusLine = document.getElementById("status-line");
const alertsBanner = document.getElementById("alerts-banner");
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
const resultPanel = document.getElementById("result-panel");
const resultMessage = document.getElementById("result-message");
const resultOutput = document.getElementById("result-output");
const resultDismiss = document.getElementById("result-dismiss");

const REFRESH_INTERVAL_MS = 1000;
const CONFIRM_TIMEOUT_MS = 8000;
const RESULT_AUTO_DISMISS_MS = 12000;
const RESTART_POLL_MS = 500;
const RESTART_TIMEOUT_MS = 30000;
const OFFLINE_STATUS = "Offline — last known state";
const PANE_INTERVAL_DEFAULT_MS = 1000;
const MAX_LOG_ENTRIES = 24;

let lastCockpit = null;
let lastFingerprint = null;
let refreshInFlight = false;
let detailHandle = null;
let detailInFlight = false;
let selectedProject = null;
const expandedCards = new Set();
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

function supportedActionStates(card) {
  return (card.action_states || []).filter((state) => state.status === "supported");
}

function actionStatesForCard(card) {
  if (card.action_states && card.action_states.length) return card.action_states;
  return (card.available_actions || []).map((action) => ({
    action,
    status: "supported",
    reason: null,
    destructive: action === "drop",
    confirmation_required: action === "drop",
  }));
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

// LIST VIEW -----------------------------------------------------------------

function stateIndicator(state) {
  switch ((state || "").toLowerCase()) {
    case "running":
      return "is-running";
    case "review ready":
    case "safe merge":
      return "is-attention";
    case "needs input":
    case "blocked":
    case "failed":
      return "is-danger";
    case "cleanable":
      return "is-success";
    case "idle":
    case "archived":
    default:
      return "is-muted";
  }
}

function actionButtonFromState(state, handle, isPrimary) {
  const supported = state.status === "supported";
  const button = el(
    "button",
    isPrimary ? "action primary" : "action",
    actionLabel(state.action, state),
  );
  if (state.action === "fix-ci" || state.action === "resolve-merge-conflicts") {
    button.classList.add("remediation-action");
  }
  button.type = "button";
  button.dataset.action = state.action;
  button.dataset.task = handle;
  if (state.destructive) button.dataset.destructive = "true";
  if (state.confirmation_required) button.dataset.confirmRequired = "true";
  if (!supported) {
    button.disabled = true;
    button.classList.add("is-disabled");
    if (state.reason) button.title = state.reason;
  }
  applyPendingConfirm(button);
  return button;
}

function appendDetailRow(parent, label, value) {
  if (!value) return;
  const row = el("div", "detail-row");
  row.append(el("span", "detail-label", label));
  row.append(el("span", "detail-value", value));
  parent.append(row);
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

function taskCard(card, options) {
  const opts = options || {};
  const article = el("article", opts.attention ? "card attention" : "card");
  article.dataset.state = card.ui_state;
  article.dataset.handle = card.qualified_handle;
  if (expandedCards.has(card.qualified_handle)) article.classList.add("expanded");

  const head = el("div", "card-head");
  const indicator = el("span", `indicator ${stateIndicator(card.ui_state)}`.trim());
  indicator.setAttribute("aria-hidden", "true");
  head.append(indicator);
  head.append(el("span", "handle", card.qualified_handle));
  head.append(el("span", "badge", card.status_label || card.ui_state));

  const states = actionStatesForCard(card);
  const supported = states.filter((state) => state.status === "supported");
  const primaryName = card.primary_action;
  const primaryState = supported.find((state) => state.action === primaryName) || supported[0];
  if (primaryState) {
    head.append(actionButtonFromState(primaryState, card.qualified_handle, true));
  }
  article.append(head);

  const summary = opts.reason || card.live_summary || card.status_label || card.title;
  if (summary) article.append(el("p", "summary", summary));

  const drawer = el("div", "action-drawer");
  const drawerTitle = el("div", "drawer-title", "Actions");
  drawer.append(drawerTitle);
  const drawerActions = el("div", "actions");
  for (const state of states) {
    if (primaryState && state.action === primaryState.action) continue;
    drawerActions.append(actionButtonFromState(state, card.qualified_handle, false));
  }
  if (drawerActions.childElementCount) drawer.append(drawerActions);
  article.append(drawer);

  const details = el("div", "card-details");
  const titleText =
    card.title && card.title !== card.qualified_handle ? card.title : null;
  appendDetailRow(details, "Title", titleText);
  appendDetailRow(details, "Lifecycle", titleCase(card.lifecycle));
  appendDetailRow(details, "State", titleCase(card.ui_state));
  if (details.childElementCount) {
    article.append(details);
    article.classList.add("has-details");
  }

  return article;
}

function renderInbox(data, cardsByHandle) {
  inbox.replaceChildren();
  const items = ((data.inbox && data.inbox.items) || [])
    .slice()
    .sort((a, b) => (a.severity || 999) - (b.severity || 999));
  if (!items.length) return;
  const cards = el("div", "cards");
  for (const item of items) {
    const card = cardsByHandle.get(item.task_handle);
    if (!card || !cardMatchesProject(card)) continue;
    const article = taskCard(card, { attention: true, reason: item.reason });
    article.dataset.severity = severityBucket(item.severity || 999);
    cards.append(article);
  }
  if (!cards.childElementCount) return;
  inbox.append(el("div", "section-title attention", "Inbox"));
  inbox.append(cards);
}

function renderRepos(data) {
  repos.replaceChildren();
  const inboxHandles = new Set(
    ((data.inbox && data.inbox.items) || []).map((item) => item.task_handle),
  );
  const byRepo = new Map();
  for (const card of data.cards) {
    if (inboxHandles.has(card.qualified_handle)) continue;
    if (!cardMatchesProject(card)) continue;
    const repo = repoOf(card.qualified_handle);
    if (!byRepo.has(repo)) byRepo.set(repo, []);
    byRepo.get(repo).push(card);
  }
  if (!byRepo.size) return;
  const title = selectedProject ? selectedProject : "Tasks";
  repos.append(el("div", "section-title", title));
  for (const repo of [...byRepo.keys()].sort()) {
    const block = el("section");
    if (!selectedProject) block.append(el("div", "group-title", repo));
    const cards = el("div", "cards");
    for (const card of byRepo.get(repo)) cards.append(taskCard(card));
    block.append(cards);
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
  const states = card.action_states || card.available_actions || [];
  return states.map((state) => {
    if (typeof state === "string") return [state, "supported"];
    return [state.action, state.status];
  });
}

function structureFingerprint(data) {
  const cards = data.cards.map((c) => [
    c.qualified_handle,
    c.primary_action,
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
  if (inboxItem && inboxItem.reason) return inboxItem.reason;
  return card.live_summary || card.status_label || card.title || "";
}

function updateLiveSummaries(data, cardsByHandle) {
  const inboxByHandle = new Map(
    ((data.inbox && data.inbox.items) || []).map((item) => [item.task_handle, item]),
  );
  for (const article of document.querySelectorAll(".card[data-handle]")) {
    const card = cardsByHandle.get(article.dataset.handle);
    if (!card) continue;
    const inboxItem = inboxByHandle.get(card.qualified_handle);
    const summary = article.querySelector(".summary");
    const text = cardSummaryText(card, inboxItem);
    if (summary) {
      if (text) {
        summary.textContent = text;
        summary.hidden = false;
      } else {
        summary.hidden = true;
      }
    }
    const badge = article.querySelector(".badge");
    if (badge) badge.textContent = card.status_label || card.ui_state;
    article.dataset.state = card.ui_state;
    if (inboxItem) {
      article.dataset.severity = severityBucket(inboxItem.severity || 999);
    }
    const indicator = article.querySelector(".indicator");
    if (indicator) {
      indicator.className = `indicator ${stateIndicator(card.ui_state)}`.trim();
    }
  }
}

function renderList(data) {
  renderProjectNav(data);
  const cardsByHandle = new Map(data.cards.map((card) => [card.qualified_handle, card]));
  renderInbox(data, cardsByHandle);
  renderRepos(data);
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
  document.body.classList.toggle("is-offline", !online);
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
    const data = await response.json();
    applyData(data);
  } catch (error) {
    setOnline(false);
  } finally {
    refreshInFlight = false;
  }
}

// DETAIL VIEW ---------------------------------------------------------------

function renderDetail(detail) {
  lastDetailData = detail;
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

  const liveSection = el("section", "detail-section");
  liveSection.append(el("h2", null, "Live status"));
  const liveGrid = el("dl", "detail-grid");
  appendGridRow(liveGrid, "Handle", detail.qualified_handle);
  appendGridRow(liveGrid, "State", detail.ui_state || "—");
  appendGridRow(liveGrid, "Lifecycle", detail.lifecycle || "—");
  appendGridRow(liveGrid, "Status", detail.status_label || "—");
  if (detail.live_status_kind) appendGridRow(liveGrid, "Live kind", detail.live_status_kind);
  if (detail.live_status_summary) appendGridRow(liveGrid, "Live note", detail.live_status_summary);
  liveSection.append(liveGrid);
  detailContainer.append(liveSection);

  const gitSection = el("section", "detail-section");
  gitSection.append(el("h2", null, "Branch"));
  const gitGrid = el("dl", "detail-grid");
  appendGridRow(gitGrid, "Branch", detail.branch);
  appendGridRow(gitGrid, "Base", detail.base_branch);
  appendGridRow(gitGrid, "Worktree", detail.worktree_path);
  if (detail.git) {
    const ahead = detail.git.ahead || 0;
    const behind = detail.git.behind || 0;
    const dirty = detail.git.dirty ? "dirty" : "clean";
    appendGridRow(gitGrid, "Diff", `${ahead} ahead · ${behind} behind · ${dirty}`);
    if (detail.git.unpushed_commits) {
      appendGridRow(gitGrid, "Unpushed", String(detail.git.unpushed_commits));
    }
  }
  gitSection.append(gitGrid);
  detailContainer.append(gitSection);

  const agentSection = el("section", "detail-section");
  agentSection.append(el("h2", null, "Agent"));
  const agentGrid = el("dl", "detail-grid");
  appendGridRow(agentGrid, "Client", detail.agent);
  appendGridRow(agentGrid, "Runtime", detail.agent_status);
  appendGridRow(agentGrid, "Tmux", detail.tmux_session);
  agentSection.append(agentGrid);

  if (detail.agent_attempts && detail.agent_attempts.length) {
    const attemptsHeading = el("h2", null, "Recent attempts");
    attemptsHeading.style.marginTop = "16px";
    agentSection.append(attemptsHeading);
    const list = el("ul", "attempt-list");
    for (const attempt of detail.agent_attempts.slice(-5).reverse()) {
      const li = el("li", "attempt");
      li.append(el("span", null, attempt.outcome));
      const started = new Date(attempt.started_unix_secs * 1000);
      li.append(el("time", null, started.toLocaleString()));
      list.append(li);
    }
    agentSection.append(list);
  }
  detailContainer.append(agentSection);

  const states = actionStatesForCard(detail);
  const supported = states.filter((state) => state.status === "supported");
  if (states.length) {
    const actions = el("div", "detail-actions");
    for (const state of states) {
      const btn = actionButtonFromState(state, detail.qualified_handle, false);
      if (state.action === detail.primary_action && state.status === "supported") {
        btn.classList.add("primary");
      }
      actions.append(btn);
    }
    detailContainer.append(actions);

    const disabled = states.filter((state) => state.status !== "supported");
    if (disabled.length) {
      const notes = el("div", "action-notes");
      for (const state of disabled) {
        if (!state.reason) continue;
        const note = el("p", "action-note");
        note.textContent = `${titleCase(state.action)}: ${state.reason}`;
        notes.append(note);
      }
      if (notes.childElementCount) detailContainer.append(notes);
    }
  }
}

function appendGridRow(grid, label, value) {
  if (value == null || value === "") return;
  grid.append(el("dt", null, label));
  grid.append(el("dd", null, String(value)));
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
    const detail = await response.json();
    renderDetail(detail);
    setOnline(true);
  } catch (error) {
    setOnline(false);
  } finally {
    detailInFlight = false;
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

// INTERACT PANEL ------------------------------------------------------------

const INTERACT_STATE_COPY = {
  WaitingForApproval: { label: "Needs your approval", tone: "attention" },
  WaitingForInput: { label: "Asking you", tone: "attention" },
  AgentRunning: { label: "Working", tone: "running" },
  CommandRunning: { label: "Running command", tone: "running" },
  TestsRunning: { label: "Running tests", tone: "running" },
  Thinking: { label: "Thinking", tone: "running" },
  Done: { label: "Idle — done", tone: "success" },
  CommandFailed: { label: "Command failed", tone: "danger" },
  Blocked: { label: "Blocked", tone: "danger" },
  AuthRequired: { label: "Needs sign-in", tone: "danger" },
  RateLimited: { label: "Rate limited", tone: "danger" },
  MergeConflict: { label: "Merge conflict", tone: "danger" },
  CiFailed: { label: "CI failed", tone: "danger" },
  Unknown: { label: "Status unknown", tone: "muted" },
};

function interactStateCopy(kind) {
  return INTERACT_STATE_COPY[kind] || { label: kind || "—", tone: "muted" };
}

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
  const copy = interactStateCopy(kind);
  lastInteractKind = kind;

  const stateRow = el("div", "interact-state");
  const pill = el("span", `interact-pill tone-${copy.tone}`, copy.label);
  stateRow.append(pill);
  if (detail.live_status_summary) {
    stateRow.append(el("span", "interact-summary", detail.live_status_summary));
  }
  panel.append(stateRow);

  const cards = el("div", "dashboard-card-grid");
  cards.append(renderDashboardCard("Current status", renderCurrentStatus(detail, pane)));
  cards.append(renderDashboardCard("Needs from you", renderNeedsFromYou(detail, pane, tmuxMissing)));
  cards.append(renderDashboardCard("Best next step", renderBestNextStep(detail)));
  cards.append(renderDashboardCard("Recent milestones", renderMilestones(detail, pane)));
  panel.append(cards);

  panel.append(renderTerminalDetails(detail, pane, tmuxMissing));

  return panel;
}

function renderDashboardCard(title, body) {
  const card = el("section", "interact-card dashboard-card");
  card.append(el("div", "interact-card-label", title));
  card.append(body);
  return card;
}

function renderCurrentStatus(detail, pane) {
  const wrap = el("div", "dashboard-card-body");
  wrap.append(el("p", "interact-card-body", detail.live_status_summary || detail.status_label || "No live summary yet."));
  const meta = el("dl", "dashboard-meta");
  appendGridRow(meta, "Task", detail.qualified_handle);
  appendGridRow(meta, "Lifecycle", detail.lifecycle || "—");
  appendGridRow(meta, "State", detail.ui_state || "—");
  if (paneAvailable && pane && pane.truncated) {
    appendGridRow(meta, "Terminal", "Live snapshot available");
  }
  wrap.append(meta);
  return wrap;
}

function renderNeedsFromYou(detail, pane, tmuxMissing) {
  const wrap = el("div", "dashboard-card-body");
  if (tmuxMissing) {
    wrap.append(el("p", "interact-card-body", "Tmux session is missing. Sync the task to recover."));
    return wrap;
  }

  const kind = detail.live_status_kind;
  const command = interactCommand(detail, pane);
  if (kind === "WaitingForApproval" && command) {
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
      wrap.append(el("p", "interact-hint", "Open the terminal for this approval."));
    }
    return wrap;
  }

  const prompt = interactPrompt(detail, pane);
  if (kind === "WaitingForInput") {
    if (prompt) wrap.append(el("p", "interact-card-body", prompt));
    wrap.append(el("p", "interact-hint", "Open the terminal for free-form replies."));
    return wrap;
  }

  wrap.append(el("p", "interact-card-body", "No immediate operator decision is blocking this task."));
  return wrap;
}

function renderBestNextStep(detail) {
  const wrap = el("div", "dashboard-card-body");
  const primary = actionStatesForCard(detail).find((state) => state.action === detail.primary_action);
  const message = nextStepMessage(detail, primary);
  wrap.append(el("p", "interact-card-body", message));
  if (primary && primary.status === "supported") {
    wrap.append(el("span", "dashboard-chip", actionLabel(primary.action, primary)));
  }
  return wrap;
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

function renderMilestones(detail, pane) {
  const list = el("ul", "milestone-list");
  for (const entry of milestoneEntries(detail, pane)) {
    const item = el("li", "milestone-entry");
    item.append(el("span", "milestone-dot"));
    item.append(el("span", "milestone-text", entry));
    list.append(item);
  }
  return list;
}

function milestoneEntries(detail, pane) {
  const entries = [];
  entries.push(detail.live_status_summary || detail.status_label || "Task opened in Cockpit.");
  if (detail.git) {
    entries.push(
      `${detail.git.ahead || 0} ahead · ${detail.git.behind || 0} behind · ${detail.git.dirty ? "dirty worktree" : "clean worktree"}`,
    );
  }
  if (detail.agent_attempts && detail.agent_attempts.length) {
    for (const attempt of detail.agent_attempts.slice(-3).reverse()) {
      const started = new Date(attempt.started_unix_secs * 1000);
      entries.push(`${titleCase(attempt.outcome)} at ${started.toLocaleString()}`);
    }
  } else if (detail.agent_activity) {
    entries.push(detail.agent_activity);
  }
  if (pane && pane.state && pane.state.command) {
    entries.push(`Pending command: ${pane.state.command}`);
  }
  return entries.slice(0, 4);
}

function renderTerminalDetails(detail, pane, tmuxMissing) {
  const details = document.createElement("details");
  details.className = "terminal-details";
  const summary = document.createElement("summary");
  summary.textContent = "View terminal details";
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
  return PANE_INTERVAL_DEFAULT_MS;
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
      if (lastDetailData) renderInteractPanelInto(lastDetailData, lastPaneData);
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
    if (lastDetailData) renderInteractPanelInto(lastDetailData, lastPaneData);
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
  const cardEl = button.closest(".card, #task-detail");
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

function toggleCardExpansion(cardEl) {
  const handle = cardEl.dataset.handle;
  if (!handle) return;
  if (cardEl.classList.contains("expanded")) {
    cardEl.classList.remove("expanded");
    expandedCards.delete(handle);
  } else {
    cardEl.classList.add("expanded");
    expandedCards.add(handle);
  }
}

document.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-action]");
  if (button) {
    if (button.disabled) return;
    if (tryConfirmDestructive(button)) return;
    runAction(button);
    return;
  }
  const cardEl = event.target.closest(".card.has-details");
  if (cardEl && !event.target.closest(".action-drawer")) {
    const handle = cardEl.dataset.handle;
    if (handle) {
      window.location.hash = `#/t/${encodeURIComponent(handle)}`;
    }
  }
});

window.addEventListener("online", () => {
  if (isSettingsRoute()) return;
  if (detailHandle) {
    loadDetail();
    schedulePaneTick(true);
  } else {
    loadCockpit();
  }
});
window.addEventListener("offline", () => setOnline(false));
document.addEventListener("visibilitychange", () => {
  if (isSettingsRoute()) return;
  if (document.hidden) {
    clearPaneTimer();
    return;
  }
  if (detailHandle) {
    loadDetail();
    schedulePaneTick(true);
  } else {
    loadCockpit();
  }
});

// PUSH NOTIFICATIONS --------------------------------------------------------

function isStandalonePwa() {
  return (
    window.matchMedia("(display-mode: standalone)").matches ||
    window.navigator.standalone === true
  );
}

function isIosBrowser() {
  return /iPad|iPhone|iPod/.test(window.navigator.userAgent);
}

function notificationEnvironment() {
  if (!("serviceWorker" in navigator) || !("Notification" in window)) {
    return {
      status: "unsupported",
      reason: "This browser cannot receive alerts.",
    };
  }
  if (!("PushManager" in window)) {
    if (isIosBrowser() && !isStandalonePwa()) {
      return {
        status: "unsupported",
        reason: "Add Ajax to your Home Screen to enable alerts.",
      };
    }
    return {
      status: "unsupported",
      reason: "Alerts are not available in this browser.",
    };
  }
  if (Notification.permission === "denied") {
    return {
      status: "denied",
      reason: "Notifications blocked — enable them in browser settings.",
    };
  }
  return { status: "available", reason: null };
}

function showAlertsBanner(text, actionable) {
  alertsBanner.textContent = text;
  alertsBanner.disabled = !actionable;
  alertsBanner.hidden = false;
}

function hideAlertsBanner() {
  alertsBanner.hidden = true;
  alertsBanner.disabled = false;
  alertsBanner.textContent = "";
}

async function syncAlertsBanner() {
  const env = notificationEnvironment();

  if (env.status === "unsupported") {
    if (isIosBrowser() && !isStandalonePwa()) {
      showAlertsBanner("Add Ajax to your Home Screen to enable alerts", false);
    } else {
      hideAlertsBanner();
    }
    return;
  }

  if (env.status === "denied") {
    showAlertsBanner("Alerts blocked — enable in browser settings", false);
    return;
  }

  try {
    const registration = await navigator.serviceWorker.ready;
    const existing = await registration.pushManager.getSubscription();
    if (existing) {
      hideAlertsBanner();
      return;
    }
    showAlertsBanner("Turn on alerts", true);
  } catch (error) {
    hideAlertsBanner();
  }
}

async function enableNotifications() {
  const env = notificationEnvironment();
  if (env.status !== "available") {
    await syncAlertsBanner();
    return;
  }

  alertsBanner.disabled = true;
  try {
    const permission = await Notification.requestPermission();
    if (permission !== "granted") {
      showResult("Notifications were not allowed", null, true);
      await syncAlertsBanner();
      return;
    }
    const registration = await navigator.serviceWorker.ready;
    const config = await (await fetch("/api/push/config")).json();
    const subscription = await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: new Uint8Array(config.public_key),
    });
    const response = await fetch("/api/push/subscribe", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(subscription),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    showResult("Notifications enabled", null, false);
    await syncAlertsBanner();
  } catch (error) {
    showResult("Could not enable notifications", null, true);
    await syncAlertsBanner();
  }
}

alertsBanner.addEventListener("click", () => {
  if (alertsBanner.disabled) return;
  enableNotifications();
});

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js")
    .then(() => syncAlertsBanner())
    .catch((error) => {
      console.warn("service worker registration failed", error);
      syncAlertsBanner();
    });
} else {
  syncAlertsBanner();
}

if (!navigator.onLine) setOnline(false);

setInterval(() => {
  if (isSettingsRoute()) return;
  if (detailHandle) loadDetail();
  else loadCockpit();
}, REFRESH_INTERVAL_MS);

applyRoute();
