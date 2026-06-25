// Ajax Web Cockpit compatibility worker.
//
// Safari is the primary mobile path. This worker exists only so browsers that
// already installed older PWA builds can update to a non-critical worker that
// unregisters itself. It intentionally does not cache or intercept requests.

self.addEventListener("install", (event) => {
  self.skipWaiting();
  event.waitUntil(Promise.resolve());
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    self.registration.unregister()
      .then(() => self.clients.matchAll({ type: "window", includeUncontrolled: true }))
      .then((clients) => Promise.all(clients.map((client) => client.navigate(client.url))))
      .catch(() => undefined),
  );
});
