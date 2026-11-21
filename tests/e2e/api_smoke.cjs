// Smoke test dell'API desktop/mobile senza browser o frontend Web.
// Uso: JARVIS_URL=http://127.0.0.1:8765 JARVIS_TOKEN=<fixture> node tests/e2e/api_smoke.cjs
const http = require("node:http");

const core = new URL(process.env.JARVIS_URL || "http://127.0.0.1:8765");
const ownerToken = process.env.JARVIS_TOKEN;
if (!ownerToken) throw new Error("JARVIS_TOKEN è richiesto per il test API");
const port = Number(core.port || 80);

function request(path, { method = "GET", body, token, deviceToken, host } = {}) {
  const data = body === undefined ? null : Buffer.from(JSON.stringify(body));
  const headers = {};
  if (data) {
    headers["content-type"] = "application/json";
    headers["content-length"] = data.length;
  }
  if (token) headers["x-jarvis-token"] = token;
  if (deviceToken) headers["x-jarvis-device"] = deviceToken;
  if (host) headers.host = host;
  return new Promise((resolve, reject) => {
    const req = http.request({
      hostname: "127.0.0.1",
      port,
      path,
      method,
      headers,
      timeout: 35_000,
    }, (res) => {
      const chunks = [];
      res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () => {
        const text = Buffer.concat(chunks).toString("utf8");
        let json;
        try { json = JSON.parse(text); } catch { json = null; }
        resolve({ status: res.statusCode, text, json });
      });
    });
    req.on("timeout", () => req.destroy(new Error(`Timeout: ${method} ${path}`)));
    req.on("error", reject);
    if (data) req.write(data);
    req.end();
  });
}

function check(condition, message) {
  if (!condition) throw new Error(`Verifica fallita: ${message}`);
  console.log(`✓ ${message}`);
}

(async () => {
  for (const path of ["/", "/app.js", "/app.css", "/pair", "/sw.js", "/fcm-sw.js", "/manifest.webmanifest"]) {
    const response = await request(path);
    check(response.status === 404, `nessuna interfaccia Web servita da ${path}`);
  }

  const health = await request("/api/health");
  check(health.status === 200 && health.json?.core?.status, "health del Core disponibile");
  const unauthorized = await request("/api/tasks");
  check(unauthorized.status === 401, "API protetta senza token");
  const tasks = await request("/api/tasks?limit=5", { token: ownerToken });
  check(tasks.status === 200 && Array.isArray(tasks.json?.items), "attività accessibili con il token proprietario");
  const notifications = await request("/api/notifications?limit=5", { token: ownerToken });
  check(notifications.status === 200 && Array.isArray(notifications.json?.items), "notifiche accessibili con il token proprietario");

  const invite = await request("/api/devices/pairing", {
    method: "POST",
    token: ownerToken,
    body: { scopes: ["chat"] },
  });
  check(invite.status === 200 && invite.json?.code, "codice monouso creato dal Core");
  const remoteHost = `${process.env.JARVIS_E2E_REMOTE_HOST || "jarvis.test"}:${port}`;
  const paired = await request("/api/devices/pair", {
    method: "POST",
    host: remoteHost,
    body: { code: invite.json.code, name: "Dispositivo E2E", platform: "desktop" },
  });
  check(paired.status === 200 && paired.json?.token, "associazione remota senza pagina Web");
  const device = await request("/api/conversation", {
    host: remoteHost,
    deviceToken: paired.json.token,
  });
  check(device.status === 200 && Array.isArray(device.json?.items), "client associato accede alla conversazione via API");

  const chat = await request("/api/chat/stream", {
    method: "POST",
    token: ownerToken,
    body: { message: "che ore sono" },
  });
  const events = chat.text.split("\n").filter(Boolean).map((line) => JSON.parse(line));
  check(chat.status === 200 && events.some((event) => event.type === "done"), "chat streaming NDJSON disponibile al client nativo");
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
