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
  const stats = await request("/api/system/stats", { token: ownerToken });
  check(stats.status === 200 && ["cpu_percent", "memory_percent", "disk_percent"].every((key) => stats.json?.[key] == null || (stats.json[key] >= 0 && stats.json[key] <= 100)),
    "carichi del sistema entro 0–100");
  check((await request("/api/system/stats")).status === 401, "carichi del sistema protetti dal token");
  // Profilo, correttore, MCP e indirizzi pubblici (ADR-0133, 0135, 0137, 0138).
  const profile = await request("/api/profile", { token: ownerToken });
  check(profile.status === 200 && profile.json?.completion_percent === 0, "profilo del proprietario vuoto all'inizio");
  const profileRefused = await request("/api/profile", { method: "PATCH", token: ownerToken, body: { goals: "scrivimi a x@example.org" } });
  check(profileRefused.status === 400, "il profilo rifiuta email e recapiti");
  const checked = await request("/api/prompt/check", { method: "POST", token: ownerToken, body: { text: "perche e' gia tardi,vero?" } });
  check(checked.json?.text === "Perché è già tardi, vero?", "correttore dei messaggi scritti");
  const mcp = await request("/api/mcp", { method: "POST", token: ownerToken, body: { jsonrpc: "2.0", id: 1, method: "tools/list" } });
  const mcpNames = (mcp.json?.result?.tools ?? []).map((tool) => tool.name);
  check(mcpNames.includes("biz__margin") && !mcpNames.includes("shell__run") && !mcpNames.includes("fs__write"),
    "MCP espone solo strumenti di calcolo e sola lettura");
  check((await request("/api/mcp", { method: "POST", body: { jsonrpc: "2.0", id: 1, method: "ping" } })).status === 401, "MCP richiede il token");
  const access = await request("/api/devices/access", { token: ownerToken });
  check(access.status === 200 && Array.isArray(access.json?.hosts), "indirizzi pubblici gestiti dall'app");
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

  const snapshot = await request("/api/chat/sync", { host: remoteHost, deviceToken: paired.json.token });
  check(snapshot.status === 200 && Array.isArray(snapshot.json?.messages) && Array.isArray(snapshot.json?.confirmations),
    "il dispositivo associato sincronizza chat e conferme con una sola richiesta");

  const catalog = await request("/api/connectors/templates", { token: ownerToken });
  const github = catalog.json?.templates?.find((template) => template.id === "github");
  check(catalog.status === 200 && github?.needs_client_id === false, "catalogo connettori disponibile al desktop");
  const installed = await request("/api/connectors/templates/github", { method: "POST", token: ownerToken, body: {} });
  check(installed.status === 200 || installed.status === 201, "installazione guidata dal catalogo");
  const connectors = await request("/api/connectors", { token: ownerToken });
  check(connectors.json?.items?.some((connector) => connector.id === "github"), "connettore presente nell’elenco del Core");

  const setup = await request("/api/setup", { token: ownerToken });
  check(setup.status === 200 && setup.json?.choices?.stt_models?.includes("tiny"), "opzioni del setup disponibili");
  const femaleVoice = await request("/api/setup/start", {
    method: "POST", token: ownerToken, body: { stt_model: "tiny", tts_voice: "it_IT-paola-medium" },
  });
  check(femaleVoice.status === 400, "una voce femminile non è ammessa (solo voci maschili)");
  check(setup.json?.choices?.tts_voices?.every((voice) => !/paola|lessac/.test(voice)), "l'elenco delle voci è solo maschile");
  const setupStart = await request("/api/setup/start", {
    method: "POST", token: ownerToken, body: { stt_model: "tiny", tts_voice: "it_IT-riccardo-x_low" },
  });
  check(setupStart.status === 200 || setupStart.status === 202, "setup vocale avviato dal Core");
  let setupState;
  for (let attempt = 0; attempt < 25; attempt++) {
    setupState = (await request("/api/setup", { token: ownerToken })).json;
    if (!setupState?.running) break;
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  check(!setupState?.running && setupState?.saved?.plan?.stt_model === "tiny", "setup di prova completato con la scelta del client");

  const localFact = await request("/api/cloud/chat", {
    method: "POST", token: ownerToken, body: { provider: "auto", message: "valida il JSON: {}", use_context: true },
  });
  check(localFact.status === 200 && localFact.json?.steps?.some((step) =>
    step.kind === "tool_result" && step.tool === "code.validate_json" && step.status === "succeeded"),
    "la chat cloud delega al Core i controlli verificabili in locale senza chiavi API");

  // Chat cloud in streaming (ADR-0131): un comando locale arriva comunque come evento `done`, senza chiavi API.
  const cloudStream = await request("/api/cloud/chat/stream", {
    method: "POST", token: ownerToken, body: { provider: "auto", message: "valida il JSON: {}" },
  });
  const cloudEvents = cloudStream.text.split("\n").filter(Boolean).map((line) => JSON.parse(line));
  check(cloudStream.status === 200 && cloudEvents.at(-1)?.type === "done" && cloudEvents.at(-1)?.result?.reply,
    "la chat cloud in streaming chiude con un esito completo");
  check((await request("/api/cloud/chat/stream", { method: "POST", body: { message: "ciao" } })).status === 401,
    "la chat cloud in streaming richiede il token");

  const chat = await request("/api/chat/stream", {
    method: "POST",
    token: ownerToken,
    body: { message: "che ore sono" },
  });
  const events = chat.text.split("\n").filter(Boolean).map((line) => JSON.parse(line));
  check(chat.status === 200 && events.some((event) => event.type === "done"), "chat streaming NDJSON disponibile al client nativo");

  for (const [message, tool, matches] of [
    ["conta le parole: città blu", "text.count",
      /2 parole, 1 riga, 9 caratteri Unicode, 10 byte UTF-8/],
    ['valida il JSON: {"ok": true}', "code.validate_json", /Il JSON è valido/],
    ['valida il JSON: {"ok": }', "code.validate_json",
      /JSON non valido alla riga 1, colonna [1-9]/],
    ["punto di pareggio costi fissi 10000 prezzo 50 costo variabile 30", "biz.breakeven", /Pareggio a 500 unità/],
    ["runway cassa 120000 costi 30000 ricavi 10000", "biz.runway", /circa 6\.0 mesi/],
    ["van tasso 10% flussi -1000; 500; 500; 500", "biz.npv", /VAN 243\.43; TIR 23\.38%/],
  ]) {
    const response = await request("/api/chat/stream", { method: "POST", token: ownerToken, body: { message } });
    const result = response.text.split("\n").filter(Boolean).map(JSON.parse).find((event) => event.type === "done")?.result;
    const step = result?.steps.find((item) => item.kind === "tool_result" && item.tool === tool);
    check(response.status === 200 && step?.status === "succeeded" && matches.test(result.reply), `strumento offline eseguito: ${message}`);
  }

  // Missioni autonome e iniziative proattive (ADR-0096, ADR-0097).
  const created = await request("/api/missions", { method: "POST", token: ownerToken, body: { goal: "controlla che ore sono", every_minutes: 60 } });
  check(created.status === 200 && created.json?.status === "active" && created.json?.every_minutes === 60, "missione ricorrente creata dal proprietario");
  const refused = await request("/api/missions", { method: "POST", token: ownerToken, body: { goal: "corto" } });
  check(refused.status === 400, "obiettivo troppo breve rifiutato");
  const listed = await request("/api/missions", { token: ownerToken });
  check(listed.json?.items?.some((m) => m.id === created.json.id) && listed.json?.proactive?.briefing === true, "missioni e impostazioni proattive nell'elenco");
  const paused = await request(`/api/missions/${created.json.id}/pause`, { method: "POST", token: ownerToken, body: {} });
  check(paused.json?.status === "paused", "missione messa in pausa");
  const noDelete = await request(`/api/missions/${created.json.id}/delete`, { method: "POST", token: ownerToken, body: {} });
  check(noDelete.status === 400, "una missione aperta non si elimina");
  const cancelled = await request(`/api/missions/${created.json.id}/cancel`, { method: "POST", token: ownerToken, body: {} });
  const deleted = await request(`/api/missions/${created.json.id}/delete`, { method: "POST", token: ownerToken, body: {} });
  check(cancelled.json?.status === "cancelled" && deleted.json?.deleted === true, "missione annullata ed eliminata");
  const anon = await request("/api/missions");
  check(anon.status === 401, "le missioni richiedono il token");
  const settings = await request("/api/proactive", { method: "PATCH", token: ownerToken, body: { briefing_time: "07:45" } });
  check(settings.status === 200 && settings.json?.briefing_time === "07:45", "orario del briefing salvato");
  const badSettings = await request("/api/proactive", { method: "PATCH", token: ownerToken, body: { briefing_time: "7:45" } });
  check(badSettings.status === 400, "orario non valido rifiutato");
  const briefing = await request("/api/proactive/briefing", { method: "POST", token: ownerToken, body: {} });
  check(briefing.status === 200 && /^(Buongiorno|Buon pomeriggio|Buonasera):/.test(briefing.json?.title || ""), "briefing su richiesta");
  const weekly = await request("/api/proactive/weekly", { method: "POST", token: ownerToken, body: {} });
  check(weekly.status === 200 && /Briefing settimanale/.test(weekly.json?.title || "") && /Settimana in arrivo/.test(weekly.json?.body || ""), "briefing settimanale su richiesta");
  const badDay = await request("/api/proactive", { method: "PATCH", token: ownerToken, body: { weekly_day: 9 } });
  check(badDay.status === 400, "giorno del briefing settimanale non valido rifiutato");
  const inbox = await request("/api/notifications?limit=20", { token: ownerToken });
  check(inbox.json?.items?.some((n) => n.category === "briefing"), "il briefing arriva fra le notifiche");
  const spec = await request("/api/openapi.json", { token: ownerToken });
  check(spec.status === 200 && spec.json?.openapi === "3.1.0" && Object.keys(spec.json?.paths || {}).length > 30, "documento OpenAPI");
  const newChat = await request("/api/conversations", { method: "POST", token: ownerToken, body: {} });
  check(newChat.status === 200 && typeof newChat.json?.id === "string", "nuova chat");
  const chats = await request("/api/conversations", { token: ownerToken });
  check(chats.json?.active === newChat.json?.id && chats.json?.items?.length >= 2, "chat attiva e chat recenti");
  const backToMain = await request("/api/conversations/select", { method: "POST", token: ownerToken, body: { id: "main" } });
  check(backToMain.status === 200, "ritorno alla chat principale");
  const job = await request("/api/cloud/background", { method: "POST", token: ownerToken, body: { message: "che ore sono" } });
  check(job.status === 202 && job.json?.status === "running", "lavoro in background avviato");
  const jobs = await request("/api/cloud/jobs", { token: ownerToken });
  check(Array.isArray(jobs.json?.items) && jobs.json.items.length >= 1, "elenco dei lavori in background");
  const security = await request("/api/security", { token: ownerToken });
  check(security.status === 200 && typeof security.json?.posture?.score === "number" && security.json?.encryption?.algorithm === "XChaCha20-Poly1305", "punteggio di protezione e cifratura");
  const docker = await request("/api/docker", { token: ownerToken });
  check(docker.status === 200 && docker.json?.profiles?.length === 4 && docker.json?.limits?.network === "none", "stato di Docker e profili del sandbox");
  const persona = await request("/api/persona", { method: "PATCH", token: ownerToken, body: { address: "Signore", address_frequency: "rare" } });
  check(persona.status === 200 && /Personalità aggiornata/.test(persona.json?.message || ""), "conferma specifica e non generica");
  const badFrequency = await request("/api/persona", { method: "PATCH", token: ownerToken, body: { address_frequency: "sempre" } });
  check(badFrequency.status === 400, "frequenza del titolo non valida rifiutata");
  const found = await request("/api/conversations/search?q=ore", { token: ownerToken });
  check(found.status === 200 && Array.isArray(found.json?.items), "ricerca nelle chat");
  const exported = await request("/api/conversations/export?id=main", { token: ownerToken });
  check(exported.status === 200 && /^# Chat principale/.test(exported.json?.markdown || ""), "esportazione della chat in Markdown");
  const missingJob = await request("/api/cloud/jobs/inesistente/cancel", { method: "POST", token: ownerToken, body: {} });
  check(missingJob.status === 404, "annullamento di un lavoro sconosciuto");
  const mcpStats = await request("/api/mcp", { method: "POST", token: ownerToken, body: { jsonrpc: "2.0", id: 9, method: "tools/call", params: { name: "data__stats", arguments: { values: [1, 2, 3, 4] } } } });
  check(mcpStats.status === 200 && /"mean": 2.5/.test(mcpStats.json?.result?.content?.[0]?.text || ""), "strumento data.stats via MCP");
  const registry = await request("/api/tools/registry?category=analysis", { token: ownerToken });
  check(registry.status === 200 && registry.json?.items?.length > 5 && registry.json.items.every((i) => i.category === "analysis" && Array.isArray(i.platforms)), "registro degli strumenti con filtri");
  const agentList = await request("/api/agents", { token: ownerToken });
  check(agentList.status === 200 && agentList.json?.items?.some((i) => i.name === "coding" && i.context === "isolated"), "registro degli agenti");
  const modelRoute = await request("/api/models/route?q=Traduci%20in%20inglese%3A%20ciao", { token: ownerToken });
  check(modelRoute.status === 200 && modelRoute.json?.providers?.[0]?.route === "translation", "instradamento dei modelli spiegato");
  const plan = await request("/api/plans", { method: "POST", token: ownerToken, body: { goal: "Smoke", run: true, steps: [
    { id: "calc", tool: "math.calc", arguments: { expression: "2+2" } },
    { id: "words", tool: "text.count", depends_on: ["calc"], arguments: { text: "risultato ${calc.result}" } } ] } });
  check(plan.status === 200 && plan.json?.id, "piano creato e avviato");
  let planDone = null;
  for (let i = 0; i < 80 && !planDone; i++) {
    const current = await request(`/api/plans/${plan.json.id}`, { token: ownerToken });
    if (["succeeded", "failed"].includes(current.json?.status)) planDone = current.json;
    else await new Promise((resolve) => setTimeout(resolve, 100));
  }
  check(planDone?.status === "succeeded" && planDone.steps?.[1]?.output?.words === 2, "piano eseguito con valori passati fra i passi");
  const emergency = await request("/api/emergency", { token: ownerToken });
  check(emergency.status === 200 && emergency.json?.active === false, "arresto di emergenza spento");
  const tailscale = await request("/api/tailscale", { token: ownerToken });
  check(tailscale.status === 200 && typeof tailscale.json?.installed === "boolean" && tailscale.json?.allowed === false, "stato di Tailscale senza autorizzare indirizzi");
  const templates = await request("/api/plans/templates", { token: ownerToken });
  check(templates.status === 200 && templates.json?.items?.some((i) => i.builtin === true), "modelli di piano incorporati");
  const twin = await request("/api/twin", { token: ownerToken });
  check(twin.status === 200 && twin.json && !JSON.stringify(twin.json).includes(ownerToken), "gemello dell'ambiente senza segreti");
  const mcpServers = await request("/api/mcp/servers", { token: ownerToken });
  check(mcpServers.status === 200, "elenco dei server MCP autorizzati");
  const watches = await request("/api/watches", { token: ownerToken });
  check(watches.status === 200, "elenco delle osservazioni di file");
  const wsSave = await request("/api/workspaces/profiles", { method: "POST", token: ownerToken, body: { name: "Prova e2e", apps: ["Terminal"], dirs: ["."] } });
  check(wsSave.status === 200 && wsSave.json?.name === "Prova e2e", "profilo di workspace salvato");
  const wsBad = await request("/api/workspaces/profiles", { method: "POST", token: ownerToken, body: { name: "x", apps: ["rm -rf /"] } });
  check(wsBad.status === 400, "profilo di workspace con comando rifiutato");
  const wsPlan = await request("/api/workspaces/profiles/plan", { method: "POST", token: ownerToken, body: { id: "Prova e2e" } });
  check(wsPlan.status === 200 && wsPlan.json?.executes === false && wsPlan.json?.steps?.some((x) => x.tool === "app.open" && x.needs_confirmation === true), "attivazione del profilo: solo piano, nessuna esecuzione");
  const wsDel = await request("/api/workspaces/profiles/remove", { method: "POST", token: ownerToken, body: { id: "Prova e2e" } });
  check(wsDel.status === 200, "profilo di workspace rimosso");
  const propose = await request("/api/plans/propose", { method: "POST", token: ownerToken, body: { goal: "Riassumi lo stato del repository" } });
  check([503, 200].includes(propose.status) && (propose.status === 200 || propose.json?.error?.code === "unavailable"), "proposta di piano: senza modello cloud risposta chiara, mai un piano avviato");
  const badGoal = await request("/api/plans/propose", { method: "POST", token: ownerToken, body: { goal: "" } });
  check(badGoal.status === 400, "proposta di piano: obiettivo vuoto rifiutato");
  const replanMissing = await request("/api/plans/inesistente/replan", { method: "POST", token: ownerToken, body: {} });
  check(replanMissing.status === 404, "ripianificazione di un piano inesistente rifiutata");
  const replanDone = await request(`/api/plans/${plan.json.id}/replan`, { method: "POST", token: ownerToken, body: {} });
  check(replanDone.status === 400, "un piano riuscito non si ripianifica");
  const docRegistry = await request("/api/tools/registry", { token: ownerToken });
  const docTools = ["pdf.extract", "sheet.read", "clipboard.read", "clipboard.write"];
  const names = (docRegistry.json?.items ?? []).map((t) => t.name);
  check(docRegistry.status === 200 && docTools.every((n) => names.includes(n)), "strumenti per documenti nel registro");
  const improvements = await request("/api/improvements", { token: ownerToken });
  check(improvements.status === 200 && Array.isArray(improvements.json?.items) && improvements.json?.policy?.includes("tu decidi"), "suggerimenti di miglioramento senza modifiche automatiche");
  const noAction = await request("/api/improvements/apply", { method: "POST", token: ownerToken, body: { id: "sconosciuto" } });
  check(noAction.status === 404, "suggerimento sconosciuto rifiutato");
  const supervisor = await request("/api/agents/supervisor", { token: ownerToken });
  check(supervisor.status === 200 && supervisor.json?.items?.every((a) => a.state === "attivo"), "supervisione degli agenti: tutti attivi di default");
  const badLimit = await request("/api/agents/limits", { method: "POST", token: ownerToken, body: { agent: "inventato", enabled: false } });
  check(badLimit.status === 400, "limiti per un agente sconosciuto rifiutati");
  const plugins = await request("/api/plugins", { token: ownerToken });
  check(plugins.status === 200 && Array.isArray(plugins.json?.items), "elenco dei pacchetti di estensione");
  const badPlugin = await request("/api/plugins", { method: "POST", token: ownerToken, body: { contract: "jarvis.plugin/1", name: "prova", version: "1.0.0", scripts: { postinstall: "x" }, skills: [] } });
  check(badPlugin.status === 400, "pacchetto con campi eseguibili rifiutato");
  const userList = await request("/api/users", { token: ownerToken });
  check(userList.status === 200 && Array.isArray(userList.json?.items) && !JSON.stringify(userList.json).includes("argon2"), "elenco degli utenti senza impronte");
  const noSession = await request("/api/me", {});
  check(noSession.status === 401, "area utente senza sessione rifiutata");
  const ownerNotUser = await request("/api/me", { token: ownerToken });
  check(noSession.status === 401 && ownerNotUser.status === 401, "il token del proprietario non apre l'area utente");
  const noInvite = await request("/api/auth/register", { method: "POST", body: { username: "prova-e2e", password: "una frase lunga e strana" } });
  check(noInvite.status === 401, "registrazione senza invito rifiutata");
  const intent = await request("/api/intent/analyze", { method: "POST", token: ownerToken, body: { message: "Non funziona un cazzo" } });
  check(intent.status === 200 && intent.json?.kind === "observation" && intent.json?.grants_permission === false, "intento: uno sfogo non è un'autorizzazione");
  const autonomy = await request("/api/autonomy", { token: ownerToken });
  check(autonomy.status === 200 && autonomy.json?.default === 4 && autonomy.json?.background_allowed === true, "livelli di autonomia predefiniti");
  const badLevel = await request("/api/autonomy", { method: "PATCH", token: ownerToken, body: { default: 9 } });
  check(badLevel.status === 400, "livello di autonomia fuori scala rifiutato");
  const sync = await request("/api/world/sync", { method: "POST", token: ownerToken });
  const world = await request("/api/world", { token: ownerToken });
  check(sync.status === 200 && world.status === 200 && world.json?.items?.some((e) => e.key === "system:local" && e.source === "observed"), "stato del mondo: sistema osservato");
  const inferred = await request("/api/world/infer", { method: "POST", token: ownerToken, body: { kind: "policy", name: "owner", value: { profile: "libero" }, confidence: 0.9 } });
  check(inferred.status === 400, "una deduzione non sostituisce un'osservazione valida");
  const cognitive = await request("/api/memory/cognitive", { token: ownerToken });
  check(cognitive.status === 200 && Array.isArray(cognitive.json?.items) && cognitive.json?.reliability_levels?.includes("verified"), "memoria con importanza e affidabilità");
  const contradictions = await request("/api/memory/contradictions", { token: ownerToken });
  check(contradictions.status === 200 && Array.isArray(contradictions.json?.items), "contraddizioni della memoria");
  const handoffNoDevice = await request("/api/handoffs", { method: "POST", token: ownerToken, body: { to: "inesistente", goal: "prova" } });
  check(handoffNoDevice.status === 400, "passaggio verso un dispositivo sconosciuto rifiutato");
  const mcpPresets = await request("/api/mcp/presets", { token: ownerToken });
  check(mcpPresets.status === 200 && mcpPresets.json?.items?.some((p) => p.id === "huggingface" && p.url === "https://huggingface.co/mcp"), "server MCP noti: Hugging Face");
  const mcpHealthMissing = await request("/api/mcp/servers/health", { method: "POST", token: ownerToken, body: { name: "inesistente" } });
  check(mcpHealthMissing.status === 404, "salute di un server MCP sconosciuto rifiutata");
  const memExport = await request("/api/memory/export", { token: ownerToken });
  check(memExport.status === 200 && memExport.json?.contract === "jarvis.memory-export/1" && Array.isArray(memExport.json?.items), "esportazione della memoria");
  const memBad = await request("/api/memory/import", { method: "POST", token: ownerToken, body: { data: { contract: "altro" } } });
  check(memBad.status === 400, "importazione con formato sconosciuto rifiutata");
  const badLogin = await request("/api/auth/login", { method: "POST", body: { username: "nessuno", password: "una frase sbagliata" } });
  check(badLogin.status === 401, "accesso con credenziali sbagliate rifiutato");
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
