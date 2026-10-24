// Service worker for PWA installation. Online-only: it never caches HTML pages,
// server function responses (/api/*) or anything with personal data, and never
// queues actions. Offline navigation shows a static notice.
//
// Caches:
// - STATIC: offline notice, icon and manifest. Bump VERSION when they change.
// - PKG: content-hashed build assets (/pkg/dwc.<hash>.{js,wasm,css}); immutable,
//   so cache-first is safe. When a new hash arrives, older entries of the same
//   kind are deleted.
const VERSION = "v1";
const STATIC = `dwc-static-${VERSION}`;
const PKG = "dwc-pkg";
const STATIC_URLS = ["/offline.html", "/icon.svg", "/manifest.webmanifest"];
const HASHED = /^\/pkg\/[\w-]+\.[\w-]+\.(js|wasm|css)$/;

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(STATIC).then((c) => c.addAll(STATIC_URLS)).then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== STATIC && k !== PKG).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

async function cachePkg(request) {
  const cache = await caches.open(PKG);
  const hit = await cache.match(request);
  if (hit) return hit;
  const response = await fetch(request);
  if (response.ok) {
    const kind = new URL(request.url).pathname.split(".").pop();
    for (const old of await cache.keys()) {
      if (new URL(old.url).pathname.endsWith(`.${kind}`)) await cache.delete(old);
    }
    await cache.put(request, response.clone());
  }
  return response;
}

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  if (request.mode === "navigate") {
    // Always from the network; never stored. Offline: static notice only.
    event.respondWith(fetch(request).catch(() => caches.match("/offline.html")));
    return;
  }
  if (HASHED.test(url.pathname)) {
    event.respondWith(cachePkg(request));
    return;
  }
  if (STATIC_URLS.includes(url.pathname)) {
    event.respondWith(fetch(request).catch(() => caches.match(request)));
  }
  // Everything else (including /api/*) goes to the network untouched.
});
