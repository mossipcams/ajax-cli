// Ajax Mobile Cockpit service worker: offline app shell + push notifications.
const CACHE = "ajax-cockpit-v21";
const SHELL = [
  "/",
  "/app.css",
  "/app.js",
  "/manifest.webmanifest",
  "/sw.js",
  "/icons/icon-192.png",
  "/icons/icon-512.png",
  "/icons/icon-maskable-512.png",
  "/icons/apple-touch-icon.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(SHELL)));
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);

  // Live task data is never cached: always go to the network.
  if (url.pathname.startsWith("/api/")) return;

  // App shell: network-first so the latest deploy is always picked up on the
  // next reload; cache only catches us when the network is unreachable.
  event.respondWith(
    fetch(request)
      .then((response) => {
        if (response && response.ok) {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
        }
        return response;
      })
      .catch(() => caches.match(request).then((cached) => cached || caches.match("/"))),
  );
});

function taskDeepLink(data) {
  const handle = data && (data.task_handle || data.handle);
  if (handle) return `#/t/${encodeURIComponent(handle)}`;
  return "/";
}

// Web Push: show a notification when the companion reports a task needs
// attention, and focus the cockpit when the operator taps it.
self.addEventListener("push", (event) => {
  let data = { title: "Ajax Cockpit", body: "A task needs attention", tag: "ajax" };
  if (event.data) {
    try {
      data = Object.assign(data, event.data.json());
    } catch (error) {
      data.body = event.data.text();
    }
  }
  const url = taskDeepLink(data);
  const answerable = !!(data.answerable && data.fingerprint && (data.task_handle || data.handle));
  const actions = answerable
    ? [
        { action: "approve", title: "Approve" },
        { action: "deny", title: "Deny" },
      ]
    : [];
  event.waitUntil(
    self.clients
      .matchAll({ type: "window", includeUncontrolled: true })
      .then((clients) => {
        // Skip the system notification flash when the operator already has Cockpit open.
        if (clients.some((client) => client.visibilityState === "visible")) {
          return undefined;
        }
        return self.registration.showNotification(data.title, {
          body: data.body,
          tag: data.tag || "ajax",
          renotify: false,
          icon: "/icons/icon-192.png",
          badge: "/icons/icon-192.png",
          actions,
          data: {
            url,
            handle: data.task_handle || data.handle || null,
            fingerprint: data.fingerprint || null,
          },
        });
      }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const data = event.notification.data || {};
  const target = data.url || "/";

  if (event.action === "approve" || event.action === "deny") {
    const requestId =
      self.crypto && self.crypto.randomUUID
        ? self.crypto.randomUUID()
        : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    event.waitUntil(
      fetch(`/api/tasks/${encodeURIComponent(data.handle)}/answer`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          answer: event.action,
          fingerprint: data.fingerprint,
          request_id: requestId,
        }),
      }).catch(() => undefined),
    );
    return;
  }

  event.waitUntil(
    self.clients
      .matchAll({ type: "window", includeUncontrolled: true })
      .then((clients) => {
        for (const client of clients) {
          if ("focus" in client) {
            if ("navigate" in client && target.startsWith("#")) {
              return client.focus().then(() => client.navigate(target));
            }
            return client.focus();
          }
        }
        return self.clients.openWindow(target.startsWith("#") ? `/${target}` : target);
      }),
  );
});
