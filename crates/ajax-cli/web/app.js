// Ajax Mobile Cockpit — client for the companion PWA.
const inbox = document.getElementById("inbox");
const repos = document.getElementById("repos");
const statusLine = document.getElementById("status-line");
const offlineBanner = document.getElementById("offline-banner");
const emptyState = document.getElementById("empty-state");
const refreshButton = document.getElementById("refresh-button");
const installButton = document.getElementById("install-button");
const notifyButton = document.getElementById("notify-button");

const REFRESH_INTERVAL_MS = 10000;
let installPrompt = null;
let lastData = null;

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

function actionButton(action, handle, isPrimary) {
  const button = el("button", isPrimary ? "action primary" : "action", titleCase(action));
  button.type = "button";
  button.dataset.action = action;
  button.dataset.task = handle;
  return button;
}

function taskCard(card, options) {
  const opts = options || {};
  const article = el("article", opts.attention ? "card attention" : "card");
  article.dataset.state = card.ui_state;

  const head = el("div", "card-head");
  head.append(el("span", "handle", card.qualified_handle));
  head.append(el("span", "badge", card.status_label || card.ui_state));
  article.append(head);

  const summary = opts.reason || card.live_summary || card.status_label || card.title;
  if (summary) article.append(el("p", "summary", summary));

  if (card.available_actions && card.available_actions.length) {
    const actions = el("div", "actions");
    for (const action of card.available_actions) {
      actions.append(actionButton(action, card.qualified_handle, action === card.primary_action));
    }
    article.append(actions);
  }
  return article;
}

function groupBlock(title, attention) {
  const block = el("section");
  block.append(el("div", attention ? "group-title attention" : "group-title", title));
  const cards = el("div", "cards");
  block.append(cards);
  return { block, cards };
}

function renderInbox(data, cardsByHandle) {
  inbox.replaceChildren();
  const items = (data.inbox && data.inbox.items) || [];
  if (!items.length) return;
  const { block, cards } = groupBlock("Needs attention", true);
  for (const item of items) {
    const card = cardsByHandle.get(item.task_handle);
    if (card) cards.append(taskCard(card, { attention: true, reason: item.reason }));
  }
  if (cards.childElementCount) inbox.append(block);
}

function renderRepos(data) {
  repos.replaceChildren();
  const byRepo = new Map();
  for (const card of data.cards) {
    const repo = repoOf(card.qualified_handle);
    if (!byRepo.has(repo)) byRepo.set(repo, []);
    byRepo.get(repo).push(card);
  }
  for (const repo of [...byRepo.keys()].sort()) {
    const { block, cards } = groupBlock(repo, false);
    for (const card of byRepo.get(repo)) cards.append(taskCard(card));
    repos.append(block);
  }
}

function summarize(data) {
  const total = data.cards.length;
  const attention = data.inbox && data.inbox.items ? data.inbox.items.length : 0;
  const stamp = new Date().toLocaleTimeString();
  return `${total} task${total === 1 ? "" : "s"} · ${attention} needing attention · updated ${stamp}`;
}

function render(data) {
  lastData = data;
  const cardsByHandle = new Map(data.cards.map((card) => [card.qualified_handle, card]));
  renderInbox(data, cardsByHandle);
  renderRepos(data);
  emptyState.hidden = data.cards.length > 0;
}

function setOnline(online) {
  offlineBanner.hidden = online;
}

async function loadCockpit() {
  statusLine.textContent = "Refreshing…";
  try {
    const response = await fetch("/api/cockpit", { cache: "no-store" });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const data = await response.json();
    render(data);
    setOnline(true);
    statusLine.textContent = summarize(data);
  } catch (error) {
    setOnline(false);
    statusLine.textContent = "Could not reach Ajax — showing last known state";
  }
}

async function runAction(button) {
  button.disabled = true;
  statusLine.textContent = `Running ${button.dataset.action}…`;
  try {
    const response = await fetch("/api/actions", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ task_handle: button.dataset.task, action: button.dataset.action }),
    });
    const payload = await response.json().catch(() => ({}));
    if (payload.cockpit) {
      render(payload.cockpit);
      statusLine.textContent = summarize(payload.cockpit);
    } else {
      await loadCockpit();
    }
    if (!response.ok) {
      statusLine.textContent = payload.error || `Action failed (HTTP ${response.status})`;
    }
  } catch (error) {
    statusLine.textContent = "Action failed — network error";
  } finally {
    button.disabled = false;
  }
}

document.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-action]");
  if (button) runAction(button);
});

refreshButton.addEventListener("click", () => loadCockpit());

window.addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  installPrompt = event;
  installButton.hidden = false;
});

installButton.addEventListener("click", async () => {
  if (!installPrompt) return;
  installButton.disabled = true;
  installPrompt.prompt();
  await installPrompt.userChoice;
  installPrompt = null;
  installButton.hidden = true;
  installButton.disabled = false;
});

window.addEventListener("appinstalled", () => {
  installButton.hidden = true;
});

window.addEventListener("online", () => loadCockpit());
window.addEventListener("offline", () => setOnline(false));

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

setInterval(loadCockpit, REFRESH_INTERVAL_MS);
loadCockpit();
