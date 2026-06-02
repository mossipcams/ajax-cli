// Ajax Mobile Cockpit service worker: offline app shell + push notifications.
const CACHE = "ajax-cockpit-v22";
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

self.addEventListener("push", (event) => {
  if (!event.data) return;
  let payload;
  try {
    payload = event.data.json();
  } catch (error) {
    return;
  }
  const title = payload.title || "Ajax Cockpit";
  const options = {
    body: payload.body || "",
    tag: payload.tag || "ajax-attention",
    data: payload.data || {},
    icon: "/icons/icon-192.png",
    badge: "/icons/icon-192.png",
  };
  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const data = event.notification.data || {};
  const handle = data.task_handle;
  const target = handle ? `#/t/${encodeURIComponent(handle)}` : "#/";
  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((clients) => {
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
