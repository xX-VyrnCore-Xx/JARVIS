// Plancia: la dashboard centrale (INTERFACE_AND_DEVICES.md). Mostra attività ed esecutore, condizioni offline,
// errori, operazioni in attesa di conferma, memoria, automazioni, dispositivi e prestazioni, con un comando
// rapido verso la chat.

import { useEffect, useState } from "react";
import { api, fmtClock, fmtTime, fmtUptime } from "../lib/api";
import { voice } from "../lib/voice";
import { activityFromPrompt, activityFromResult } from "../lib/avatarActivity";
import type { AvatarActivity } from "../lib/avatarActivity";
import { Avatar, AvatarConversation, AvatarStatus } from "../components/Avatar";
import { useApp, useVoice } from "../state";
import { decideConfirmation } from "../lib/passkey";
import type { Notification } from "../state";
import type { Confirmation } from "./Chat";
import { MemoryGraph } from "./Memory";
import { Icon } from "../components/Icon";
import type { MemoryItem } from "./Memory";

interface Snapshot {
  tasks: any[];
  confirmations: Confirmation[];
  nodes: any[];
  devices: any[];
  workflows: any[];
  memory: MemoryItem[];
  graph: { nodes: any[]; edges: any[] };
  metrics: any;
  history: any[];
  notifications: Notification[];
}

const STATE_LABEL: Record<string, string> = {
  created: "creato", planned: "pianificato", running: "in esecuzione", waiting_confirmation: "attende conferma",
  succeeded: "riuscito", failed: "fallito", cancelled: "annullato",
};

function executor(t: any): string {
  const node = /^\s*nodo\s+([a-z0-9-]+)/i.exec(t.objective || "");
  if (node) return `nodo ${node[1]}`;
  if (t.requester?.kind === "workflow") return "automazione · Core";
  if (t.requester?.kind === "device") return "dispositivo · Core";
  return "Core locale";
}

function Panel({ title, icon, children, className = "" }: { title: string; icon: string; children: React.ReactNode; className?: string }) {
  return (
    <section className={`hud-panel ${className}`} aria-label={title}>
      <header>
        <h3><Icon name={icon} size={16} className="panel-icon" />{title}</h3>
      </header>
      <div className="hud-body">{children}</div>
    </section>
  );
}

function Gauge({ value, max, label, unit, tone = "accent" }: { value: number; max: number; label: string; unit?: string; tone?: string }) {
  const r = 34, c = 2 * Math.PI * r;
  const frac = Math.max(0, Math.min(1, max ? value / max : 0));
  return (
    <figure className={`gauge tone-${tone}`}>
      <svg viewBox="0 0 84 84" aria-hidden="true">
        <circle cx="42" cy="42" r={r} className="track" />
        <circle cx="42" cy="42" r={r} className="arc" strokeDasharray={`${c * frac} ${c}`} transform="rotate(-90 42 42)" />
      </svg>
      <div className="gauge-v">{value}{unit ? <small>{unit}</small> : null}</div>
      <figcaption>{label}</figcaption>
    </figure>
  );
}

function Sparkline({ points }: { points: number[] }) {
  if (points.length < 2) return <p className="empty">Lo storico compare dopo le prime istantanee (ogni 5 minuti).</p>;
  const W = 300, H = 60, max = Math.max(...points, 1);
  const d = points.map((p, i) => `${i ? "L" : "M"}${((i / (points.length - 1)) * W).toFixed(1)},${(H - (p / max) * (H - 6) - 3).toFixed(1)}`).join(" ");
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="spark" role="img" aria-label={`Richieste per istantanea, massimo ${max}`}>
      <path d={`${d} L${W},${H} L0,${H} Z`} className="area" />
      <path d={d} className="line" />
    </svg>
  );
}

function Kpi({ icon, label, value, tone = "" }: { icon: string; label: string; value: React.ReactNode; tone?: string }) {
  return (
    <div className={`kpi ${tone}`}>
      <Icon name={icon} size={20} className="kpi-icon" />
      <div>
        <b>{value}</b>
        <span>{label}</span>
      </div>
    </div>
  );
}

function Clock() {
  const [now, setNow] = useState(new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);
  return (
    <div className="hud-clock">
      <span className="time">{now.toLocaleTimeString("it-IT", { hour: "2-digit", minute: "2-digit" })}</span>
      <span className="date">{now.toLocaleDateString("it-IT", { weekday: "long", day: "numeric", month: "long" })}</span>
    </div>
  );
}

export function Dashboard({ active }: { active: boolean }) {
  const app = useApp();
  const { health, reachable, show, sendRef, activity, notifTick, refreshStatus, pollNotifications, dismissToast, bumpActivity } = app;
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [command, setCommand] = useState("");
  useVoice();

  const load = async () => {
    const get = <T,>(p: string, fallback: T) => api<T>(p).catch(() => fallback);
    const [tasks, confirmations, nodes, devices, workflows, memory, graph, metrics, history, notifications] = await Promise.all([
      get<any>("/api/tasks?limit=12", { items: [] }),
      get<any>("/api/confirmations", { items: [] }),
      get<any>("/api/nodes", { items: [] }),
      get<any>("/api/devices", { items: [] }),
      get<any>("/api/workflows", { items: [] }),
      get<any>("/api/memory", { items: [] }),
      get<any>("/api/memory/graph", { nodes: [], edges: [] }),
      get<any>("/api/metrics", null),
      get<any>("/api/metrics/history?hours=24", { items: [] }),
      get<any>("/api/notifications?limit=8", { items: [] }),
    ]);
    setSnap({
      tasks: tasks.items, confirmations: confirmations.items, nodes: nodes.items, devices: devices.items, workflows: workflows.items,
      memory: memory.items, graph, metrics, history: history.items, notifications: notifications.items,
    });
  };

  useEffect(() => {
    if (!active) return;
    void load();
    const t = setInterval(() => void load(), 10000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, activity, notifTick]);

  const decide = async (c: Confirmation, approve: boolean) => {
    dismissToast(`confirmation:${c.confirmation_id}`);
    try {
      const result = await decideConfirmation(c.confirmation_id, approve);
      app.setAvatarActivity(activityFromResult(app.avatarActivity, [{ kind: "tool_call", tool: c.tool }], result.task?.state === "failed" ? "error" : undefined));
    } catch (e) {
      alert((e as Error).message);
    }
    refreshStatus();
    pollNotifications();
    bumpActivity();
  };

  const runCommand = (e: React.FormEvent) => {
    e.preventDefault();
    const t = command.trim();
    if (!t || app.thinking) return;
    setCommand("");
    show("chat");
    sendRef.current(t);
  };

  const backend = health?.ai?.gateway?.backend;
  const vs = voice.status;
  const http = snap?.metrics?.series?.http || {};
  const chatP95 = http["POST /api/chat"]?.p95_ms ?? 0;
  const reqs = Object.values<any>(http).reduce((a, x) => a + (x.count || 0), 0);
  const errs = Object.values<any>(http).reduce((a, x) => a + (x.errors || 0), 0);
  const spark = (snap?.history || []).slice(-36).map((x) => Object.values<any>(x.series?.http || {}).reduce((a, s) => a + (s.count || 0), 0));
  const kinds = { semantic: 0, procedural: 0, episodic: 0 } as Record<string, number>;
  for (const m of snap?.memory || []) kinds[m.kind] = (kinds[m.kind] || 0) + 1;
  const failed = (snap?.tasks || []).filter((t) => t.state === "failed");
  const alerts = (snap?.notifications || []).filter((n) => n.severity === "error" || n.severity === "warning");
  const nodesOnline = (snap?.nodes || []).filter((n) => n.online).length;
  const devicesActive = (snap?.devices || []).filter((d) => !d.revoked_at && !d.slug).length;
  const nextRuns = (snap?.workflows || []).filter((w) => w.enabled).slice(0, 5);

  const runningTask = snap?.tasks.find((task) => task.state === "running");
  const backgroundActivity: AvatarActivity | undefined = runningTask && !app.thinking && voice.state === "idle"
    ? runningTask.requester?.kind === "workflow" ? { category: "automation" } : activityFromPrompt(runningTask.objective || "")
    : undefined;

  const today = new Date().toDateString();
  const todays = (snap?.tasks || []).filter((t) => new Date(t.created_at).toDateString() === today);
  const done = (snap?.tasks || []).filter((t) => t.state === "succeeded" || t.state === "failed");
  const okRate = done.length ? Math.round((100 * done.filter((t) => t.state === "succeeded").length) / done.length) : null;
  const offline: { label: string; detail: string; tone: string }[] = [];
  if (!reachable) offline.push({ label: "Core", detail: "non raggiungibile", tone: "bad" });
  if (health?.ai?.status && health.ai.status !== "up") offline.push({ label: "AI", detail: health.ai.error?.message || "non disponibile", tone: "bad" });
  if (backend?.name === "offline") offline.push({ label: "AI", detail: backend.degraded_reason || "modalità offline: comandi semplici", tone: "warn" });
  if (vs && !vs.available) offline.push({ label: "Voce", detail: (vs.reasons || []).join("; ") || "non disponibile", tone: "warn" });
  if (health?.web && !health.web.enabled) offline.push({ label: "Web", detail: "accesso al Web disattivato", tone: "muted" });
  for (const n of snap?.nodes || []) if (!n.online) offline.push({ label: `Nodo ${n.slug}`, detail: "non in linea", tone: "warn" });

  return (
    <section className={`view hud${active ? " active" : ""}`} id="view-dashboard" aria-label="Plancia">
      <div className="hud-grid">
        <section className="hud-hero" aria-label="Stato di JARVIS">
          <div className="hero-side">
            <div className="hero-availability">
              <span className={`led ${reachable ? "succeeded" : "failed"}`} aria-hidden="true" />
              {reachable ? "Core in linea" : "Core non raggiungibile"}
            </div>
            <h1>Ciao, sono <br />JARVIS.</h1>
            <AvatarConversation />
          </div>

          <div className="hero-core">
            <Avatar id="avatar-core" size="large" activity={backgroundActivity} />
            <AvatarStatus activity={backgroundActivity} />
          </div>

          <aside className="hero-telemetry" aria-label="Stato del sistema">
            <Clock />
            <h2><Icon name="system" size={17} /> Stato del sistema</h2>
            <dl className="hero-readouts">
              <div><dt>Inferenza locale</dt><dd className={backend ? "ok" : "warn"}>{backend ? (backend.name === "offline" ? "Comandi offline" : backend.name) : "—"}</dd></div>
              <div><dt>Voce</dt><dd className={vs?.available ? "ok" : "warn"}>{vs ? (vs.available ? (vs.tts?.name || "Solo dettatura") : "Non disponibile") : "—"}</dd></div>
              <div><dt>Audit</dt><dd className={health?.audit?.chain_valid ? "ok" : "bad"}>{health?.audit ? (health.audit.chain_valid ? "Integro" : "Alterato") : "—"}</dd></div>
              <div><dt>Core</dt><dd>{health?.core ? `v${health.core.version}${snap?.metrics ? " · " + fmtUptime(snap.metrics.uptime_s) : ""}` : "—"}</dd></div>
            </dl>
          </aside>

          <form className="hud-command" id="dashboard-command" onSubmit={runCommand}>
            <span className="hud-command-icon"><Icon name="chat" size={21} /></span>
            <input aria-label="Comando a JARVIS" placeholder="Chiedi qualcosa a JARVIS…" value={command} onChange={(e) => setCommand(e.target.value)} maxLength={8000} />
            <button className="primary" type="submit" disabled={app.thinking}><Icon name="send" size={18} /> Invia</button>
          </form>
        </section>

        <section className="kpi-row" aria-label="Indicatori">
          <Kpi icon="bolt" label="attività oggi" value={todays.length} />
          <Kpi icon="check" label="riuscite" value={okRate == null ? "—" : `${okRate}%`} tone={okRate != null && okRate < 80 ? "warn" : "ok"} />
          <Kpi icon="shield" label="da confermare" value={snap?.confirmations.length ?? 0} tone={snap?.confirmations.length ? "warn" : ""} />
          <Kpi icon="memory" label="ricordi" value={(snap?.memory || []).length} />
          <Kpi icon="node" label="nodi in linea" value={`${nodesOnline}/${(snap?.nodes || []).length}`} />
        </section>

        <Panel title="In attesa di conferma" icon="shield" className={snap?.confirmations.length ? "alert" : ""}>
          <div id="dash-confirmations" className="hud-list">
            {snap && !snap.confirmations.length ? <p className="empty">Nessuna operazione in attesa.</p> : null}
            {(snap?.confirmations || []).map((c) => (
              <div className="hud-item confirm-item" key={c.confirmation_id}>
                <div className="grow">
                  <div className="t">{c.tool} · rischio {c.risk}</div>
                  <div className="d">{c.expected_effect}</div>
                  <div className="m">scade alle {fmtClock(c.expires_at)}</div>
                </div>
                <div className="hud-actions">
                  <button className="primary small" onClick={() => decide(c, true)}>Approva</button>
                  <button className="danger small" onClick={() => decide(c, false)}>Rifiuta</button>
                </div>
              </div>
            ))}
          </div>
        </Panel>

        <Panel title="Attività ed esecutori" icon="tasks" className="wide">
          <div id="dash-tasks" className="hud-list">
            {snap && !snap.tasks.length ? <p className="empty">Nessuna attività: prova un comando qui sopra.</p> : null}
            {(snap?.tasks || []).slice(0, 7).map((t) => (
              <div className="hud-item" key={t.task_id}>
                <span className={`led ${t.state}`} aria-hidden="true"></span>
                <div className="grow">
                  <div className="t">{t.objective}</div>
                  <div className="m">{executor(t)} · {fmtTime(t.created_at)}</div>
                </div>
                <span className={`badge ${t.state}`}>{STATE_LABEL[t.state] || t.state}</span>
              </div>
            ))}
          </div>
          <button className="ghost small link" onClick={() => show("tasks")}>Tutte le attività →</button>
        </Panel>

        <Panel title="Condizioni e nodi" icon="node">
          <div className="hud-list" id="dash-conditions">
            <div className="hud-item">
              <span className={`led ${reachable ? "succeeded" : "failed"}`} aria-hidden="true"></span>
              <div className="grow"><div className="t">Core locale</div><div className="m">{reachable ? "in linea" : "non raggiungibile"}</div></div>
            </div>
            {(snap?.nodes || []).map((n) => (
              <div className="hud-item" key={n.device_id}>
                <span className={`led ${n.online ? "succeeded" : "failed"}`} aria-hidden="true"></span>
                <div className="grow"><div className="t">Nodo {n.slug}</div><div className="m">{n.online ? "in linea" : "non in linea"} · {(n.capabilities || []).length} strumenti</div></div>
              </div>
            ))}
            {offline.length ? offline.map((o, i) => (
              <div className={`hud-item cond ${o.tone}`} key={i}><div className="grow"><div className="t">{o.label}</div><div className="m">{o.detail}</div></div></div>
            )) : <p className="empty">Tutto in linea.</p>}
          </div>
        </Panel>

        <Panel title="Prestazioni" icon="gauge">
          <div className="gauges">
            <Gauge value={chatP95} max={Math.max(1000, chatP95)} label="p95 chat" unit="ms" tone={chatP95 > 2000 ? "warn" : "accent"} />
            <Gauge value={reqs} max={Math.max(reqs, 100)} label="richieste" />
            <Gauge value={errs} max={Math.max(reqs, 1)} label="errori" tone={errs ? "bad" : "ok"} />
          </div>
          <Sparkline points={spark} />
          <div className="hud-tokens" id="dash-tokens">
            token al modello: <b>{snap?.metrics?.counters?.["ai.tokens.prompt"] ?? 0}</b> in ingresso ·{" "}
            <b>{snap?.metrics?.counters?.["ai.tokens.output"] ?? 0}</b> generati
          </div>
        </Panel>

        <Panel title="Errori e avvisi" icon="alert" className={failed.length || alerts.length ? "warnline" : ""}>
          <div className="hud-list" id="dash-errors">
            {snap && !failed.length && !alerts.length ? <p className="empty">Nessun errore recente.</p> : null}
            {failed.slice(0, 3).map((t) => (
              <div className="hud-item cond bad" key={t.task_id}><div className="grow"><div className="t">{t.objective}</div><div className="m">{t.error ? `${t.error.code} — ${t.error.message}` : "attività fallita"}</div></div></div>
            ))}
            {alerts.slice(0, 4).map((n) => (
              <div className={`hud-item cond ${n.severity === "error" ? "bad" : "warn"}`} key={n.notification_id}><div className="grow"><div className="t">{n.title}</div><div className="m">{fmtTime(n.created_at)}{n.body ? " · " + n.body : ""}</div></div></div>
            ))}
          </div>
        </Panel>

        <Panel title="Memoria" icon="memory" className="wide">
          <div className="mem-stats">
            <div><b>{kinds.semantic}</b><span>fatti</span></div>
            <div><b>{kinds.procedural}</b><span>preferenze</span></div>
            <div><b>{kinds.episodic}</b><span>episodi</span></div>
            <div><b>{snap?.graph.edges.length ?? 0}</b><span>relazioni</span></div>
          </div>
          {snap && snap.graph.edges.length ? <MemoryGraph graph={snap.graph} compact /> : <p className="empty">Il grafo compare quando JARVIS impara relazioni («ricorda che mia sorella si chiama Anna»).</p>}
          <button className="ghost small link" onClick={() => show("memory")}>Apri la memoria →</button>
        </Panel>

        <Panel title="Automazioni" icon="automations">
          <div className="hud-list">
            {snap && !nextRuns.length ? <p className="empty">Nessuna automazione attiva.</p> : null}
            {nextRuns.map((w) => (
              <div className="hud-item" key={w.workflow_id}>
                <span className={`led ${w.last_run ? w.last_run.status : "pending"}`} aria-hidden="true"></span>
                <div className="grow"><div className="t">{w.name}</div><div className="m">{w.trigger_text} · prossima {w.next_run || "—"}</div></div>
              </div>
            ))}
          </div>
          <button className="ghost small link" onClick={() => show("automations")}>Gestisci →</button>
        </Panel>

        <Panel title="Dispositivi" icon="devices">
          <div className="mem-stats">
            <div><b>{devicesActive}</b><span>associati</span></div>
            <div><b>{nodesOnline}/{(snap?.nodes || []).length}</b><span>nodi in linea</span></div>
          </div>
          <button className="ghost small link" onClick={() => show("devices")}>Associa un dispositivo →</button>
        </Panel>
      </div>
    </section>
  );
}
