const CACHE_NAME = "vibeping-shell-v1";
const SHELL_ASSETS = [
  "/",
  "/index.html",
  "/styles.css",
  "/app.js",
  "/manifest.webmanifest",
  "/icons/icon-180.png",
  "/icons/icon-192.png",
  "/icons/icon-512.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE_NAME).then((cache) => cache.addAll(SHELL_ASSETS)));
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    Promise.all([
      caches
        .keys()
        .then((names) => Promise.all(names.filter((name) => name !== CACHE_NAME).map((name) => caches.delete(name)))),
      self.clients.claim(),
    ]),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);

  if (request.method !== "GET" || url.origin !== self.location.origin || url.pathname.startsWith("/api/")) {
    return;
  }

  if (request.mode === "navigate") {
    event.respondWith(fetch(request).catch(() => caches.match("/index.html")));
    return;
  }

  event.respondWith(caches.match(request).then((cached) => cached || fetch(request)));
});

self.addEventListener("push", (event) => {
  const fallback = {
    title: "VibePing",
    body: "A new VibePing notification arrived.",
    tag: "vibeping",
    url: "/",
  };
  let payload = fallback;

  if (event.data) {
    try {
      payload = { ...fallback, ...event.data.json() };
    } catch {
      const text = event.data.text();
      payload = { ...fallback, body: text || fallback.body };
    }
  }

  event.waitUntil(
    self.registration.showNotification(payload.title, {
      body: payload.body,
      icon: "/icons/icon-192.png",
      badge: "/icons/icon-192.png",
      tag: payload.tag,
      timestamp: Number(payload.timestamp) || Date.now(),
      data: { url: payload.url || "/" },
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const target = new URL(event.notification.data?.url || "/", self.location.origin).href;

  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then(async (windows) => {
      for (const client of windows) {
        if (new URL(client.url).origin === self.location.origin) {
          if ("navigate" in client && client.url !== target) {
            await client.navigate(target);
          }
          return client.focus();
        }
      }
      return self.clients.openWindow(target);
    }),
  );
});
