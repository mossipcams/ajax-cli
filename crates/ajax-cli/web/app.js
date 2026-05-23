// Ajax Mobile Cockpit — client for the companion PWA.
const inbox = document.getElementById("inbox");
const repos = document.getElementById("repos");
const statusLine = document.getElementById("status-line");
const offlineBanner = document.getElementById("offline-banner");
const emptyState = document.getElementById("empty-state");
const refreshButton = document.getElementById("refresh-button");
const notifyButton = document.getElementById("notify-button");

const REFRESH_INTERVAL_MS = 1000;
// Actions that should require a two-tap confirm before firing.
const DESTRUCTIVE_ACTIONS = new Set(["drop"]);
const CONFIRM_TIMEOUT_MS = 3000;

let lastFingerprint = null;
let refreshInFlight = false;
// Card handles whose detail panel should stay expanded across re-renders.
const expandedCards = new Set();
// Timers for buttons currently staged in their "tap to confirm" state.
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

function repoOf(handle) {
  const slash = handle.indexOf("/");
  return slash === -1 ? handle : handle.slice(0, slash);
}

// Severity is 1..4 in the backend, lower = more urgent.
function severityBucket(value) {
  if (value <= 2) return "high";
  if (value <= 3) return "medium";
  return "low";
}

function actionButton(action, handle, isPrimary) {
  const button = el("button", isPrimary ? "action primary" : "action", titleCase(action));
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
  if (expandedCards.has(card.qualified_handle)) {
    article.classList.add("expanded");
  }

  const head = el("div", "card-head");
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

  // Detail panel revealed by tapping the card body.
  const details = el("div", "card-details");
  const titleText = card.title && card.title !== card.qualified_handle ? card.title : null;
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
    ((data.inbox && data.inbox.items) || []).map((item) => item.task_handle),
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
  return `${total} ${taskWord} · ${attention} needs attention`;
}

// Cheap stable signature of just the parts we render — used to skip DOM
// rebuilds when polled data is unchanged so the cards never flash.
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

function render(data) {
  const cardsByHandle = new Map(data.cards.map((card) => [card.qualified_handle, card]));
  renderInbox(data, cardsByHandle);
  renderRepos(data);
  emptyState.hidden = data.cards.length > 0;
}

function applyData(data) {
  const fp = fingerprint(data);
  if (fp !== lastFingerprint) {
    render(data);
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

// First tap on a destructive action arms the confirm; second tap within the
// window proceeds. Returns true when the tap was *consumed* by staging.
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
    const response = await fetch("/api/actions", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ task_handle: button.dataset.task, action: button.dataset.action }),
    });
    const payload = await response.json().catch(() => ({}));
    if (payload.cockpit) {
      applyData(payload.cockpit);
    } else {
      await loadCockpit();
    }
    if (!response.ok) {
      statusLine.textContent = payload.error || `Action failed (HTTP ${response.status})`;
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
  if (cardEl) toggleCardExpansion(cardEl);
});

refreshButton.addEventListener("click", () => loadCockpit({ manual: true }));

window.addEventListener("online", () => loadCockpit());
window.addEventListener("offline", () => setOnline(false));
document.addEventListener("visibilitychange", () => loadCockpit());

// Push notifications --------------------------------------------------------
function pushSupported() {
  return "serviceWorker" in navigator && "PushManager" in window && "Notification" in window;
}

async function refreshNotifyButton() {
  if (!pushSupported() || Notification.permission === "denied") {
    notifyButton.hidden = true;
    return;
  }
  const registration = await navigator.serviceWorker.ready;
  const existing = await registration.pushManager.getSubscription();
  notifyButton.hidden = Boolean(existing);
}

async function enableNotifications() {
  notifyButton.disabled = true;
  try {
    const permission = await Notification.requestPermission();
    if (permission !== "granted") {
      statusLine.textContent = "Notifications were not allowed";
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
    notifyButton.hidden = true;
    statusLine.textContent = "Notifications enabled";
  } catch (error) {
    statusLine.textContent = "Could not enable notifications";
  } finally {
    notifyButton.disabled = false;
  }
}

notifyButton.addEventListener("click", enableNotifications);

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js")
    .then(() => refreshNotifyButton())
    .catch((error) => {
      console.warn("service worker registration failed", error);
    });
}

if (!navigator.onLine) setOnline(false);

setInterval(loadCockpit, REFRESH_INTERVAL_MS);
loadCockpit();
