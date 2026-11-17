// Service worker minimo dell'app installabile (RF-10 mobile). Non memorizza nulla: l'API e le pagine
// vengono sempre richieste al Core; senza connessione mostra solo un avviso.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (e) => {
  if (e.request.mode !== "navigate") return;
  e.respondWith(fetch(e.request).catch(() => new Response(
    "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><title>JARVIS</title>" +
    "<body style='font-family:system-ui;background:#0f1115;color:#e6e6e6;padding:2rem'>" +
    "<h1>JARVIS non raggiungibile</h1><p>Il computer con JARVIS è spento o la connessione è assente. Riprova più tardi.</p>",
    { headers: { "Content-Type": "text/html; charset=utf-8" } })));
});
