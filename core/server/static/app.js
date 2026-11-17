"use strict";
// Interfaccia del prototipo JARVIS. Nessuna dipendenza esterna; tutto il testo è inserito con textContent.

const TOKEN = document.querySelector('meta[name="jarvis-token"]').content;
const $ = (sel) => document.querySelector(sel);

async function api(path, options = {}) {
  const res = await fetch(path, {
    ...options,
    headers: { "Content-Type": "application/json", "X-Jarvis-Token": TOKEN, ...(options.headers || {}) },
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error ? data.error.message : `HTTP ${res.status}`);
  return data;
}

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else node.setAttribute(k, v);
  }
  for (const c of children.flat()) if (c != null) node.append(c instanceof Node ? c : document.createTextNode(String(c)));
  return node;
}

const fmtTime = (iso) => (iso ? new Date(iso).toLocaleString("it-IT") : "");

// ------------------------------------------------------------------ stato

async function refreshStatus() {
  try {
    const h = await fetch("/api/health").then((r) => r.json());
    const ai = $("#st-ai");
    const b = h.ai && h.ai.gateway && h.ai.gateway.backend;
    if (h.ai && h.ai.status === "up" && b) {
      ai.textContent = `AI: ${b.name} · ${b.model}`;
      ai.className = "pill " + (b.name === "offline" ? "warn" : "ok");
      ai.title = h.ai.gateway.reason || "Modello locale attivo";
    } else if (h.busy) {
      ai.textContent = "AI: occupata";
    } else {
      ai.textContent = "AI: non disponibile";
      ai.className = "pill bad";
      ai.title = (h.ai && h.ai.error && h.ai.error.message) || "";
    }
    const au = $("#st-audit");
    if (h.audit) {
      au.textContent = `Audit: ${h.audit.events} eventi ${h.audit.chain_valid ? "✓ integro" : "✗ alterato"}`;
      au.className = "pill " + (h.audit.chain_valid ? "ok" : "bad");
    }
  } catch {
    $("#st-ai").textContent = "Core non raggiungibile";
    $("#st-ai").className = "pill bad";
  }
}

// ------------------------------------------------------------------ chat

const messages = $("#messages");

function scrollDown() { messages.scrollTop = messages.scrollHeight; }

function stepChips(steps) {
  const chips = [];
  for (const s of steps || []) {
    if (s.kind === "delegation" && s.backend) chips.push(el("span", { class: "step" }, `agente · ${s.backend.name}`));
    if (s.kind === "tool_call") chips.push(el("span", { class: "step" }, `strumento · ${s.tool}`));
    if (s.kind === "policy") chips.push(el("span", { class: `step ${s.decision}` }, `policy · ${s.decision} · rischio ${s.risk}`));
    if (s.kind === "confirmation") chips.push(el("span", { class: `step ${s.approved ? "allow" : "deny"}` }, s.approved ? "confermato" : "rifiutato"));
    if (s.kind === "tool_result") chips.push(el("span", { class: `step ${s.status}` }, `esito · ${s.status}${s.error ? " · " + s.error : ""}`));
  }
  return chips;
}

function sourceList(sources) {
  if (!sources || !sources.length) return null;
  const safe = sources.filter((s) => /^https?:\/\//i.test(s.url || ""));
  return el("div", { class: "sources" }, el("div", { class: "sources-title" }, "Fonti"),
    el("ol", {}, safe.map((s) => el("li", {},
      el("a", { href: s.url, target: "_blank", rel: "noopener noreferrer nofollow" }, s.title || s.url),
      el("span", { class: "src-meta" }, ` · ${new URL(s.url).hostname} · ${fmtTime(s.retrieved_at)}${s.content_sha256 ? " · sha256 " + s.content_sha256.slice(0, 12) + "…" : ""}${s.provider ? " · " + s.provider : ""}`)))));
}

function addMessage(role, text, extra = {}) {
  const meta = extra.steps && extra.steps.length ? el("div", { class: "meta" }, stepChips(extra.steps)) : null;
  const node = el("div", { class: `msg ${role}${extra.error ? " error" : ""}` }, text, sourceList(extra.sources), meta);
  messages.append(node);
  scrollDown();
  return node;
}

function addConfirmation(c) {
  const card = el("div", { class: "confirm", role: "group", "aria-label": "Conferma richiesta" });
  const decide = async (approve) => {
    card.querySelectorAll("button").forEach((b) => (b.disabled = true));
    try {
      const out = await api(`/api/confirmations/${encodeURIComponent(c.confirmation_id)}`, { method: "POST", body: JSON.stringify({ approve }) });
      card.replaceWith(el("div", { class: "confirm" }, el("p", { class: "small" }, approve ? "✓ Confermato" : "✗ Rifiutato")));
      addMessage("assistant", out.reply, { steps: out.steps, sources: out.sources });
    } catch (e) {
      addMessage("assistant", `Errore: ${e.message}`, { error: true });
      card.remove();
    }
    refreshStatus();
    pollNotifications();
  };
  card.append(
    el("h3", {}, `Conferma richiesta · ${c.tool} · rischio ${c.risk}`),
    el("p", {}, c.expected_effect),
    el("p", { class: "small" }, `Destinazione: ${c.target} · scade alle ${new Date(c.expires_at).toLocaleTimeString("it-IT")} · valida una sola volta`),
    el("div", { class: "actions" },
      el("button", { class: "primary", onclick: () => decide(true) }, "Approva"),
      el("button", { class: "danger", onclick: () => decide(false) }, "Rifiuta")),
  );
  messages.append(card);
  scrollDown();
}

async function send(text) {
  text = text.trim();
  if (!text) return;
  addMessage("user", text);
  const typing = el("div", { class: "msg typing" }, "JARVIS sta elaborando…");
  messages.append(typing);
  scrollDown();
  $("#send").disabled = true;
  try {
    const out = await api("/api/chat", { method: "POST", body: JSON.stringify({ message: text }) });
    typing.remove();
    addMessage("assistant", out.reply || "(nessuna risposta)", { steps: out.steps, sources: out.sources, error: out.task && out.task.state === "failed" });
    if (out.confirmation) addConfirmation(out.confirmation);
  } catch (e) {
    typing.remove();
    addMessage("assistant", `Errore: ${e.message}`, { error: true });
  } finally {
    $("#send").disabled = false;
    refreshStatus();
    pollNotifications();
  }
}

$("#composer").addEventListener("submit", (e) => {
  e.preventDefault();
  const input = $("#input");
  const text = input.value;
  input.value = "";
  input.style.height = "";
  send(text);
});
$("#input").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); $("#composer").requestSubmit(); }
});
$("#input").addEventListener("input", (e) => { e.target.style.height = ""; e.target.style.height = Math.min(e.target.scrollHeight, 200) + "px"; });
$("#suggestions").addEventListener("click", (e) => { if (e.target.classList.contains("chip")) send(e.target.textContent); });

async function loadConversation() {
  try {
    const { items } = await api("/api/conversation?limit=50");
    if (!items.length) addMessage("assistant", "Ciao, sono JARVIS. Scrivimi qualcosa o prova uno dei suggerimenti qui sotto.");
    for (const m of items) addMessage(m.role === "user" ? "user" : "assistant", m.content, { sources: m.sources });
    const pending = await api("/api/confirmations");
    for (const c of pending.items) addConfirmation(c);
  } catch (e) {
    addMessage("assistant", `Errore nel caricamento: ${e.message}`, { error: true });
  }
}

// ------------------------------------------------------------------ altre viste

const views = {
  async tasks() {
    const { items } = await api("/api/tasks?limit=50");
    const box = $("#tasks");
    box.replaceChildren(...(items.length ? items.map((t) => el("div", { class: "card" },
      el("div", { class: "body" },
        el("div", { class: "title" }, t.objective),
        el("div", { class: "sub" }, `${t.task_id} · ${fmtTime(t.created_at)}`),
        el("ol", { class: "history" }, t.history.map((h) => el("li", {}, `${h.to} — ${h.reason || ""}`))),
        t.error ? el("div", { class: "sub" }, `errore: ${t.error.code} — ${t.error.message}`) : null),
      el("span", { class: `badge ${t.state}` }, t.state))) : [el("p", { class: "empty" }, "Nessuna attività.")]));
  },
  async memory() {
    const { items } = await api("/api/memory");
    const box = $("#memory");
    box.replaceChildren(...(items.length ? items.map((m) => el("div", { class: "card" },
      el("div", { class: "body" }, el("div", { class: "title" }, m.text), el("div", { class: "sub" }, `${m.id} · ${fmtTime(m.created_at)}`)),
      el("button", { class: "danger", onclick: async () => {
        if (!confirm(`Cancellare definitivamente «${m.text}»?`)) return;
        await api(`/api/memory/${encodeURIComponent(m.id)}`, { method: "DELETE" });
        views.memory(); refreshStatus();
      } }, "Cancella"))) : [el("p", { class: "empty" }, "La memoria è vuota. Prova: \"ricorda che …\"")]));
  },
  async workspace() {
    const data = await api("/api/workspace");
    $("#workspace").replaceChildren(...(data.entries.length ? data.entries.map((e) => el("div", { class: "card" },
      el("div", { class: "body" }, el("div", { class: "title" }, e.name + (e.kind === "dir" ? "/" : "")), el("div", { class: "sub" }, `${e.size} byte`)))) :
      [el("p", { class: "empty" }, "Workspace vuota. Prova: \"scrivi note.txt: …\"")]));
  },
  async audit() {
    const data = await api("/api/audit?limit=100");
    $("#audit-summary").textContent = `${data.total} eventi · catena di hash ${data.chain_valid ? "integra ✓" : "ALTERATA ✗ " + data.chain_errors.join("; ")}`;
    $("#audit").replaceChildren(...(data.events.length ? data.events.map((e) => el("div", { class: "card" },
      el("div", { class: "body" },
        el("div", { class: "title" }, `#${e.sequence} ${e.action}`),
        el("div", { class: "sub" }, `${fmtTime(e.at)} · ${e.actor.kind}:${e.actor.id}${e.error_code ? " · " + e.error_code : ""}`),
        el("div", { class: "sub" }, `hash ${e.hash.slice(0, 16)}… ← ${e.prev_hash.slice(0, 16)}…`)),
      el("span", { class: `badge ${e.outcome}` }, e.outcome))) : [el("p", { class: "empty" }, "Nessun evento.")]));
  },
};

// ------------------------------------------------------------------ automazioni

let toolCatalog = null;
async function catalog() {
  if (!toolCatalog) toolCatalog = (await api("/api/tools")).items.filter((t) => t.workflow_step);
  return toolCatalog;
}

async function addStep() {
  const tools = await catalog();
  const box = el("div", { class: "step-box" });
  const select = el("select", { "aria-label": "Strumento" }, tools.map((t) => el("option", { value: t.name }, `${t.name} — ${t.summary}`)));
  const args = el("div", { class: "form" });
  const renderArgs = () => {
    const tool = tools.find((t) => t.name === select.value);
    const props = (tool.input_schema && tool.input_schema.properties) || {};
    const required = (tool.input_schema && tool.input_schema.required) || [];
    args.replaceChildren(
      ...(tool.risk === "high" || tool.risk === "critical" ? [el("span", { class: "risk-high" }, "Rischio alto: approvandolo ora, verrà eseguito in background senza ulteriori conferme.")] : []),
      ...Object.keys(props).map((k) => el("label", {}, `${k}${required.includes(k) ? " *" : ""}`,
        k === "content" ? el("textarea", { "data-arg": k, rows: "3" }) : el("input", { "data-arg": k, ...(required.includes(k) ? { required: "" } : {}) }))),
    );
  };
  select.addEventListener("change", renderArgs);
  box.append(el("div", { class: "head" }, select, el("button", { type: "button", class: "ghost danger", onclick: () => box.remove(), "aria-label": "Rimuovi passo" }, "✕")), args);
  $("#wf-steps").append(box);
  renderArgs();
}

function readForm(form) {
  const f = new FormData(form);
  const kind = f.get("kind");
  const trigger = kind === "daily" ? { kind, at: f.get("at") }
    : kind === "once" ? { kind, at: new Date(Date.now() + Number(f.get("every")) * 60000).toISOString().replace(/\.\d{3}Z$/, "Z") }
    : { kind, every_seconds: Number(f.get("every")) * 60 };
  const steps = [...form.querySelectorAll(".step-box")].map((box) => {
    const args = {};
    box.querySelectorAll("[data-arg]").forEach((i) => { if (i.value !== "") args[i.dataset.arg] = i.value; });
    return { tool: box.querySelector("select").value, arguments: args };
  });
  return { name: f.get("name"), trigger, steps, on_failure: f.get("continue") ? "continue" : "stop" };
}

$("#wf-add-step").addEventListener("click", addStep);
$("#wf-form").kind.addEventListener("change", (e) => {
  $("#wf-at").hidden = e.target.value !== "daily";
  $("#wf-every").hidden = e.target.value === "daily";
});
$("#wf-new").addEventListener("toggle", (e) => { if (e.target.open && !$("#wf-steps").children.length) addStep(); });
$("#wf-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const body = readForm(e.target);
  if (!body.steps.length) return alert("Aggiungi almeno un passo.");
  const list = body.steps.map((s, i) => `${i + 1}. ${s.tool} ${JSON.stringify(s.arguments)}`).join("\n");
  if (!confirm(`Approvi l'automazione «${body.name}»?\n\n${list}\n\nI passi verranno eseguiti in background con la policy vigente.`)) return;
  try {
    await api("/api/workflows", { method: "POST", body: JSON.stringify(body) });
    e.target.reset();
    $("#wf-steps").replaceChildren();
    $("#wf-new").open = false;
    views.automations();
  } catch (err) { alert(err.message); }
});

async function showRuns(w, box) {
  if (box.childElementCount) return box.replaceChildren();
  const { items } = await api(`/api/workflows/${w.workflow_id}/runs`);
  box.replaceChildren(...(items.length ? items.map((r) => el("div", { class: "sub" },
    `${fmtTime(r.started_at)} · ${r.trigger.kind} · ${r.status} · ` + r.steps.map((s) => `${s.tool}:${s.status}${s.preapproved ? " (pre-approvato)" : ""}${s.error_code ? " " + s.error_code : ""}`).join(", "))) :
    [el("div", { class: "sub" }, "Nessuna esecuzione.")]));
}

views.automations = async () => {
  const { items } = await api("/api/workflows");
  $("#automations").replaceChildren(...(items.length ? items.map((w) => {
    const runs = el("div", { class: "runs" });
    const act = (label, fn, cls = "ghost") => el("button", { class: cls, onclick: async () => {
      try { await fn(); views.automations(); refreshStatus(); } catch (e) { if (e.message !== "Annullato") alert(e.message); }
    } }, label);
    return el("div", { class: "card" },
      el("div", { class: "body" },
        el("div", { class: "title" }, `${w.name}`),
        el("div", { class: "sub" }, `[${w.id}] ${w.trigger_text} · ${w.steps.map((s) => s.tool).join(" → ")}`),
        el("div", { class: "sub" }, `prossima: ${w.next_run || "—"} · ultima: ${w.last_run ? `${w.last_run.status} (${fmtTime(w.last_run.started_at)})` : "mai"}`),
        el("div", { class: "actions-row" },
          act("Esegui ora", () => api(`/api/workflows/${w.workflow_id}/run`, { method: "POST" })),
          act(w.enabled ? "Sospendi" : "Riattiva", () => api(`/api/workflows/${w.workflow_id}/enabled`, { method: "POST", body: JSON.stringify({ enabled: !w.enabled }) })),
          el("button", { class: "ghost", onclick: () => showRuns(w, runs).catch((e) => alert(e.message)) }, "Cronologia"),
          act("Elimina", async () => {
            if (!confirm(`Eliminare «${w.name}»?`)) throw new Error("Annullato");
            await api(`/api/workflows/${w.workflow_id}`, { method: "DELETE" });
          }, "ghost danger")),
        runs),
      el("span", { class: `badge ${w.enabled ? (w.last_run ? w.last_run.status : "allowed") : "cancelled"}` }, w.enabled ? "attiva" : "sospesa"));
  }) : [el("p", { class: "empty" }, "Nessuna automazione. Creane una qui sopra o scrivi in chat: \"ogni ora info sistema\".")]));
};

function show(view) {
  document.querySelectorAll(".nav-item").forEach((b) => b.classList.toggle("active", b.dataset.view === view));
  document.querySelectorAll(".view").forEach((v) => v.classList.toggle("active", v.id === `view-${view}`));
  if (views[view]) views[view]().catch((e) => alert(e.message));
  if (view === "chat") $("#input").focus();
}

document.querySelectorAll(".nav-item").forEach((b) => b.addEventListener("click", () => show(b.dataset.view)));
document.querySelectorAll("[data-refresh]").forEach((b) => b.addEventListener("click", () => views[b.dataset.refresh]().catch((e) => alert(e.message))));

// ------------------------------------------------------------------ notifiche (RF-13)

const CATEGORY = { automation: "automazione", confirmation: "conferma", system: "sistema", security: "sicurezza" };
let notifSince = null;
let notifPrimed = false;

function toast(n) {
  const t = el("div", { class: `toast ${n.severity}`, role: "status", onclick: () => { openNotification(n); t.remove(); } },
    el("div", { class: "t" }, n.title), n.body ? el("div", { class: "b" }, n.body) : null);
  $("#toasts").append(t);
  setTimeout(() => t.remove(), 8000);
  if ("Notification" in window && Notification.permission === "granted" && document.hidden) {
    try { new Notification(`JARVIS · ${n.title}`, { body: n.body || "", tag: n.dedupe_key || n.notification_id }); } catch { /* non supportato */ }
  }
}

function setCount(unread) {
  const c = $("#notif-count");
  c.textContent = unread > 99 ? "99+" : String(unread);
  c.hidden = !unread;
  $("#bell").setAttribute("aria-label", `Notifiche, ${unread} non lette`);
}

async function openNotification(n) {
  if (!n.read_at) {
    try { const r = await api(`/api/notifications/${n.notification_id}/read`, { method: "POST" }); setCount(r.unread); } catch { /* già letta */ }
  }
  $("#notif-panel").hidden = true;
  $("#bell").setAttribute("aria-expanded", "false");
  if (n.link && n.link.view) show(n.link.view);
}

async function renderNotifications() {
  const data = await api("/api/notifications?limit=30");
  setCount(data.unread);
  $("#notif-list").replaceChildren(...(data.items.length ? data.items.map((n) => el("button", { class: `notif${n.read_at ? "" : " unread"}`, onclick: () => openNotification(n) },
    el("span", { class: `dot ${n.severity}` }),
    el("span", {},
      el("div", { class: "t" }, n.title),
      n.body ? el("div", { class: "b" }, n.body) : null,
      el("div", { class: "m" }, `${fmtTime(n.created_at)} · ${CATEGORY[n.category] || n.category}${n.task_id ? " · task " + n.task_id.slice(0, 8) : ""}`)))) :
    [el("p", { class: "empty", style: "padding: .8rem" }, "Nessuna notifica.")]));
}

async function pollNotifications() {
  try {
    const data = await api(`/api/notifications?limit=20${notifSince ? "&since=" + encodeURIComponent(notifSince) : ""}`);
    setCount(data.unread);
    if (data.items.length) notifSince = data.items[0].created_at;
    if (notifPrimed) data.items.filter((n) => !n.read_at).reverse().forEach(toast);
    notifPrimed = true;
    if (!$("#notif-panel").hidden && data.items.length) renderNotifications();
  } catch { /* il Core può essere momentaneamente occupato */ }
}

$("#bell").addEventListener("click", async () => {
  const panel = $("#notif-panel");
  panel.hidden = !panel.hidden;
  $("#bell").setAttribute("aria-expanded", String(!panel.hidden));
  if (!panel.hidden) renderNotifications().catch((e) => alert(e.message));
});
$("#notif-readall").addEventListener("click", async () => {
  await api("/api/notifications/read-all", { method: "POST" });
  renderNotifications();
});
document.addEventListener("keydown", (e) => { if (e.key === "Escape") { $("#notif-panel").hidden = true; $("#bell").setAttribute("aria-expanded", "false"); } });
if ("Notification" in window && Notification.permission === "default") {
  $("#notif-browser").hidden = false;
  $("#notif-browser").addEventListener("click", async () => { await Notification.requestPermission(); $("#notif-browser").hidden = true; });
}

loadConversation();
refreshStatus();
pollNotifications();
setInterval(refreshStatus, 15000);
setInterval(pollNotifications, 5000);
