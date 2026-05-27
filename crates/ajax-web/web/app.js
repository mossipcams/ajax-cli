// Ajax Cockpit — Eames/Braun PWA client.
const inbox = document.getElementById("inbox");
const repos = document.getElementById("repos");
const emptyState = document.getElementById("empty-state");
const statusLine = document.getElementById("status-line");
const offlineBanner = document.getElementById("offline-banner");
const refreshButton = document.getElementById("refresh-button");
const notifyButton = document.getElementById("notify-button");
const newTaskButton = document.getElementById("new-task-button");
const newTaskSheet = document.getElementById("new-task-sheet");
const newTaskForm = document.getElementById("new-task-form");
const newTaskRepo = document.getElementById("new-task-repo");
const newTaskTitle = document.getElementById("new-task-title-input");
const newTaskAgent = document.getElementById("new-task-agent");
const newTaskError = document.getElementById("new-task-error");
const detailContainer = document.getElementById("task-detail");

const REFRESH_INTERVAL_MS = 1000;
const DESTRUCTIVE_ACTIONS = new Set(["drop"]);
const CONFIRM_TIMEOUT_MS = 3000;

let lastCockpit = null;
let lastFingerprint = null;
let refreshInFlight = false;
let detailHandle = null;
let detailInFlight = false;
const expandedCards = new Set();
const pendingConfirms = new WeakMap();

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = text;
  return node;
}

function titleCase(value) {
  return value ? value.charAt(0).toUpperCase() + value.slice(1) : value;
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

function actionButton(action, handle, isPrimary) {
  const button = el(
    "button",
    isPrimary ? "action primary" : "action",
    titleCase(action)
  );
  button.type = "button";
  button.dataset.action = action;
  button.dataset.task = handle;
  if (DESTRUCTIVE_ACTIONS.has(action)) button.dataset.destructive = "true";
  return button;
}

function appendDetailRow(parent, label, value) {
  if (!value) return;
  const row = el("div", "detail-row");
  row.append(el("span", "detail-label", label));
  row.append(el("span", "detail-value", value));
  parent.append(row);
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

  const available = card.available_actions || [];
  const primary = card.primary_action;
  if (primary && available.includes(primary)) {
    head.append(actionButton(primary, card.qualified_handle, true));
  }
  article.append(head);

  const summary = opts.reason || card.live_summary || card.status_label || card.title;
  if (summary) article.append(el("p", "summary", summary));

  const secondary = available.filter((action) => action !== primary);
  if (secondary.length) {
    const actions = el("div", "actions");
    for (const action of secondary) {
      actions.append(actionButton(action, card.qualified_handle, false));
    }
    article.append(actions);
  }

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
    if (!card) continue;
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
    ((data.inbox && data.inbox.items) || []).map((item) => item.task_handle)
  );
  const byRepo = new Map();
  for (const card of data.cards) {
    if (inboxHandles.has(card.qualified_handle)) continue;
    const repo = repoOf(card.qualified_handle);
    if (!byRepo.has(repo)) byRepo.set(repo, []);
    byRepo.get(repo).push(card);
  }
  if (!byRepo.size) return;
  repos.append(el("div", "section-title", "Tasks"));
  for (const repo of [...byRepo.keys()].sort()) {
    const block = el("section");
    block.append(el("div", "group-title", repo));
    const cards = el("div", "cards");
    for (const card of byRepo.get(repo)) cards.append(taskCard(card));
    block.append(cards);
    repos.append(block);
  }
}

function summarize(data) {
  const total = data.cards.length;
  const attention = data.inbox && data.inbox.items ? data.inbox.items.length : 0;
  if (!total) return "All quiet";
  const taskWord = total === 1 ? "task" : "tasks";
  if (!attention) return `${total} ${taskWord}`;
  return `${total} ${taskWord} · ${attention} need attention`;
}

function fingerprint(data) {
  const cards = data.cards.map((c) => [
    c.qualified_handle,
    c.ui_state,
    c.status_label,
    c.live_summary,
    c.title,
    c.lifecycle,
    c.primary_action,
    (c.available_actions || []).join(","),
  ]);
  const items = (data.inbox && data.inbox.items) || [];
  return JSON.stringify({
    cards,
    inbox: items.map((item) => [item.task_handle, item.reason, item.severity]),
  });
}

function renderList(data) {
  const cardsByHandle = new Map(data.cards.map((card) => [card.qualified_handle, card]));
  renderInbox(data, cardsByHandle);
  renderRepos(data);
  emptyState.hidden = data.cards.length > 0;
}

function applyData(data) {
  lastCockpit = data;
  const fp = fingerprint(data);
  if (fp !== lastFingerprint) {
    renderList(data);
    lastFingerprint = fp;
  }
  statusLine.textContent = summarize(data);
  setOnline(true);
}

function setOnline(online) {
  offlineBanner.hidden = online;
  document.body.classList.toggle("is-offline", !online);
}

async function loadCockpit(options) {
  const manual = options && options.manual;
  if (refreshInFlight || document.hidden) return;
  refreshInFlight = true;
  if (manual) document.body.classList.add("is-refreshing");
  try {
    const response = await fetch("/api/cockpit", { cache: "no-store" });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const data = await response.json();
    applyData(data);
  } catch (error) {
    setOnline(false);
  } finally {
    refreshInFlight = false;
    document.body.classList.remove("is-refreshing");
  }
}

// DETAIL VIEW ---------------------------------------------------------------

function renderDetail(detail) {
  detailContainer.replaceChildren();

  const header = el("div", "detail-header");
  const back = el("button", "back", "← Back");
  back.type = "button";
  back.addEventListener("click", () => {
    window.location.hash = "#/";
  });
  header.append(back);
  header.append(el("h1", "detail-title", detail.title || detail.qualified_handle));
  detailContainer.append(header);

  // Live status
  const liveSection = el("section", "detail-section");
  liveSection.append(el("h2", null, "Live status"));
  const liveGrid = el("dl", "detail-grid");
  appendGridRow(liveGrid, "Handle", detail.qualified_handle);
  appendGridRow(liveGrid, "State", detail.ui_state || "—");
  appendGridRow(liveGrid, "Lifecycle", detail.lifecycle || "—");
  appendGridRow(liveGrid, "Status", detail.status_label || "—");
  if (detail.live_status_kind) {
    appendGridRow(liveGrid, "Live kind", detail.live_status_kind);
  }
  if (detail.live_status_summary) {
    appendGridRow(liveGrid, "Live note", detail.live_status_summary);
  }
  liveSection.append(liveGrid);
  detailContainer.append(liveSection);

  // Git / branch info
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

  // Agent
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
      const time = el("time", null, started.toLocaleString());
      li.append(time);
      list.append(li);
    }
    agentSection.append(list);
  }

  detailContainer.append(agentSection);

  // Actions
  const available = (detail.available_actions || []).filter((a) => a !== "resume" && a !== "start");
  if (available.length) {
    const actions = el("div", "detail-actions");
    for (const action of available) {
      const btn = actionButton(action, detail.qualified_handle, false);
      if (action === detail.primary_action) btn.classList.add("primary");
      actions.append(btn);
    }
    detailContainer.append(actions);
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
      statusLine.textContent = "Task no longer exists";
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

// HASH ROUTER ---------------------------------------------------------------

function applyRoute() {
  const hash = window.location.hash || "#/";
  if (hash.startsWith("#/t/")) {
    const handle = decodeURIComponent(hash.slice("#/t/".length));
    detailHandle = handle;
    document.body.classList.add("view-detail");
    loadDetail();
  } else {
    detailHandle = null;
    document.body.classList.remove("view-detail");
    loadCockpit();
  }
}

window.addEventListener("hashchange", applyRoute);

// NEW TASK SHEET ------------------------------------------------------------

function openNewTaskSheet() {
  populateRepoOptions();
  newTaskTitle.value = "";
  newTaskError.hidden = true;
  newTaskError.textContent = "";
  document.body.classList.add("sheet-open");
  setTimeout(() => newTaskTitle.focus(), 60);
}

function closeNewTaskSheet() {
  document.body.classList.remove("sheet-open");
}

function populateRepoOptions() {
  const repos = lastCockpit && lastCockpit.repos && lastCockpit.repos.repos
    ? lastCockpit.repos.repos
    : [];
  newTaskRepo.replaceChildren();
  if (!repos.length) {
    const opt = document.createElement("option");
    opt.value = "";
    opt.textContent = "No repositories configured";
    opt.disabled = true;
    newTaskRepo.append(opt);
    return;
  }
  for (const repo of repos) {
    const opt = document.createElement("option");
    opt.value = repo.name;
    opt.textContent = repo.name;
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
      return;
    }
    if (payload.cockpit) applyData(payload.cockpit);
    closeNewTaskSheet();
  } catch (error) {
    newTaskError.textContent = "Action failed — network error";
    newTaskError.hidden = false;
  } finally {
    submit.disabled = false;
  }
}

newTaskButton.addEventListener("click", openNewTaskSheet);
newTaskForm.addEventListener("submit", submitNewTask);
newTaskSheet.addEventListener("click", (event) => {
  if (event.target === newTaskSheet) closeNewTaskSheet();
});
document.querySelectorAll("[data-sheet-cancel]").forEach((btn) =>
  btn.addEventListener("click", closeNewTaskSheet),
);

// ACTIONS -------------------------------------------------------------------

function tryConfirmDestructive(button) {
  if (!button.dataset.destructive) return false;
  if (button.classList.contains("confirming")) {
    const timer = pendingConfirms.get(button);
    if (timer) clearTimeout(timer);
    pendingConfirms.delete(button);
    button.classList.remove("confirming");
    if (button.dataset.originalLabel) {
      button.textContent = button.dataset.originalLabel;
    }
    return false;
  }
  button.dataset.originalLabel = button.textContent;
  button.textContent = "Tap to confirm";
  button.classList.add("confirming");
  const timer = setTimeout(() => {
    button.classList.remove("confirming");
    if (button.dataset.originalLabel) {
      button.textContent = button.dataset.originalLabel;
    }
    pendingConfirms.delete(button);
  }, CONFIRM_TIMEOUT_MS);
  pendingConfirms.set(button, timer);
  return true;
}

async function runAction(button) {
  const cardEl = button.closest(".card");
  const peers = cardEl
    ? Array.from(cardEl.querySelectorAll("button[data-action]"))
    : [button];
  const originalLabel = button.textContent;
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
    if (payload.cockpit) {
      applyData(payload.cockpit);
    } else {
      await loadCockpit();
    }
    if (!response.ok) {
      statusLine.textContent = payload.error || `Action failed (HTTP ${response.status})`;
    } else if (detailHandle) {
      loadDetail();
    }
  } catch (error) {
    statusLine.textContent = "Action failed — network error";
  } finally {
    if (button.isConnected) {
      button.textContent = originalLabel;
      button.classList.remove("is-running");
    }
    for (const peer of peers) {
      if (peer.isConnected) peer.disabled = false;
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
  if (cardEl) {
    // If the user clicks a non-button area inside an inbox/repo card, open the
    // detail view; secondary tap on the card body still toggles inline detail
    // for users who prefer it.
    const handle = cardEl.dataset.handle;
    if (handle && event.detail >= 2) {
      window.location.hash = `#/t/${encodeURIComponent(handle)}`;
      return;
    }
    toggleCardExpansion(cardEl);
  }
});

refreshButton.addEventListener("click", () => {
  if (detailHandle) loadDetail();
  else loadCockpit({ manual: true });
});

window.addEventListener("online", () => {
  if (detailHandle) loadDetail();
  else loadCockpit();
});
window.addEventListener("offline", () => setOnline(false));
document.addEventListener("visibilitychange", () => {
  if (detailHandle) loadDetail();
  else loadCockpit();
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

async function syncNotificationUi() {
  const env = notificationEnvironment();
  notifyButton.hidden = false;
  notifyButton.removeAttribute("title");

  if (env.status === "unsupported") {
    notifyButton.disabled = true;
    notifyButton.textContent = "Alerts";
    notifyButton.dataset.state = "unsupported";
    notifyButton.title = env.reason;
    return;
  }

  if (env.status === "denied") {
    notifyButton.disabled = false;
    notifyButton.textContent = "Alerts blocked";
    notifyButton.dataset.state = "denied";
    notifyButton.title = env.reason;
    return;
  }

  try {
    const registration = await navigator.serviceWorker.ready;
    const existing = await registration.pushManager.getSubscription();
    if (existing) {
      notifyButton.disabled = true;
      notifyButton.textContent = "Alerts on";
      notifyButton.dataset.state = "enabled";
      return;
    }
    notifyButton.disabled = false;
    notifyButton.textContent = "Alerts";
    notifyButton.dataset.state = "off";
  } catch (error) {
    notifyButton.disabled = true;
    notifyButton.textContent = "Alerts…";
    notifyButton.dataset.state = "pending";
  }
}

async function enableNotifications() {
  const env = notificationEnvironment();
  if (env.status === "unsupported") {
    statusLine.textContent = env.reason;
    return;
  }
  if (env.status === "denied") {
    statusLine.textContent = env.reason;
    return;
  }

  notifyButton.disabled = true;
  try {
    const permission = await Notification.requestPermission();
    if (permission !== "granted") {
      statusLine.textContent = "Notifications were not allowed";
      await syncNotificationUi();
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
    statusLine.textContent = "Notifications enabled";
    await syncNotificationUi();
  } catch (error) {
    statusLine.textContent = "Could not enable notifications";
    await syncNotificationUi();
  }
}

notifyButton.addEventListener("click", enableNotifications);

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js")
    .then(() => syncNotificationUi())
    .catch((error) => {
      console.warn("service worker registration failed", error);
      syncNotificationUi();
    });
} else {
  syncNotificationUi();
}

if (!navigator.onLine) setOnline(false);

// Poll the active view every second.
setInterval(() => {
  if (detailHandle) loadDetail();
  else loadCockpit();
}, REFRESH_INTERVAL_MS);

applyRoute();
