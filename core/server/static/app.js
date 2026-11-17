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

const fmtTime = (iso) => (iso ? new Date(iso).toLocaleString() : "");

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

function addMessage(role, text, extra = {}) {
  const meta = extra.steps && extra.steps.length ? el("div", { class: "meta" }, stepChips(extra.steps)) : null;
  const node = el("div", { class: `msg ${role}${extra.error ? " error" : ""}` }, text, meta);
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
      addMessage("assistant", out.reply, { steps: out.steps });
    } catch (e) {
      addMessage("assistant", `Errore: ${e.message}`, { error: true });
      card.remove();
    }
    refreshStatus();
  };
  card.append(
    el("h3", {}, `Conferma richiesta · ${c.tool} · rischio ${c.risk}`),
    el("p", {}, c.expected_effect),
    el("p", { class: "small" }, `Destinazione: ${c.target} · scade alle ${new Date(c.expires_at).toLocaleTimeString()} · valida una sola volta`),
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
    addMessage("assistant", out.reply || "(nessuna risposta)", { steps: out.steps, error: out.task && out.task.state === "failed" });
    if (out.confirmation) addConfirmation(out.confirmation);
  } catch (e) {
    typing.remove();
    addMessage("assistant", `Errore: ${e.message}`, { error: true });
  } finally {
    $("#send").disabled = false;
    refreshStatus();
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
    for (const m of items) addMessage(m.role === "user" ? "user" : "assistant", m.content);
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

function show(view) {
  document.querySelectorAll(".nav-item").forEach((b) => b.classList.toggle("active", b.dataset.view === view));
  document.querySelectorAll(".view").forEach((v) => v.classList.toggle("active", v.id === `view-${view}`));
  if (views[view]) views[view]().catch((e) => alert(e.message));
  if (view === "chat") $("#input").focus();
}

document.querySelectorAll(".nav-item").forEach((b) => b.addEventListener("click", () => show(b.dataset.view)));
document.querySelectorAll("[data-refresh]").forEach((b) => b.addEventListener("click", () => views[b.dataset.refresh]().catch((e) => alert(e.message))));

loadConversation();
refreshStatus();
setInterval(refreshStatus, 15000);
