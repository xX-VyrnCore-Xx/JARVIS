// Service worker minimo dell'app installabile (RF-10 mobile). Non memorizza nulla: l'API e le pagine
// vengono sempre richieste al Core; senza connessione mostra solo un avviso.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (e) => {
  if (e.request.mode !== "navigate") return;
  e.respondWith(navigate(e.request));
});

async function navigate(request) {
  const response = await fetch(request).catch(() => null);
  if (response && ![502, 503, 504].includes(response.status)) return response;
  return new Response(
    "<!doctype html><html lang='it'><meta charset='utf-8'><meta name='viewport' content='width=device-width'><title>JARVIS</title>" +
    "<main><h1>JARVIS non raggiungibile</h1><p>Controlla la connessione e il computer su cui gira JARVIS.</p>" +
    "<p><a href=''>Riprova</a></p></main></html>",
    { status: 503, headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" } },
  );
}
