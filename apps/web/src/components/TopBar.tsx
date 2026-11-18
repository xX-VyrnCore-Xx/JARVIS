import { useEffect, useState } from "react";
import { api, fmtTime, guard, post } from "../lib/api";
import { voice } from "../lib/voice";
import { useApp, useVoice } from "../state";
import type { Notification } from "../state";
import { SHORTCUT } from "./CommandPalette";
import { Icon } from "./Icon";

const CATEGORY: Record<string, string> = {
  automation: "automazione", confirmation: "conferma", system: "sistema", security: "sicurezza", integration: "integrazione",
};
const VOICE_LABELS: Record<string, string> = {
  idle: "pronta", listening: "in ascolto", transcribing: "trascrizione…", speaking: "risposta vocale",
};

function AiPill() {
  const { health, reachable } = useApp();
  if (!reachable) return <span className="pill bad" id="st-ai">Core non raggiungibile</span>;
  const b = health?.ai?.gateway?.backend;
  if (health?.ai?.status === "up" && b)
    return (
      <span className={`pill ${b.name === "offline" ? "warn" : "ok"}`} id="st-ai" title={health.ai.gateway.reason || "Modello locale attivo"}>
        AI: {b.name} · {b.model}
      </span>
    );
  if (health?.busy) return <span className="pill" id="st-ai">AI: occupata</span>;
  if (!health) return <span className="pill" id="st-ai">AI: …</span>;
  return <span className="pill bad" id="st-ai" title={health.ai?.error?.message || ""}>AI: non disponibile</span>;
}

function VoicePill() {
  useVoice();
  const s = voice.status;
  if (!s) return <span className="pill" id="st-voice" title="Stato della voce">Voce: …</span>;
  const cls = !s.available ? "" : voice.state === "listening" ? "listening" : voice.state === "speaking" ? "speaking" : "ok";
  const title = s.available
    ? `STT ${s.stt.name}/${s.stt.model} · TTS ${s.tts ? s.tts.name + "/" + s.tts.voice : "assente"} · attivazione: ${s.wake || "—"}`
    : (s.reasons || []).join("; ");
  return (
    <span className={`pill ${cls}`} id="st-voice" title={title}>
      {s.available ? `Voce: ${VOICE_LABELS[voice.state]}` : "Voce: non disponibile"}
    </span>
  );
}

function ThemeToggle() {
  const [light, setLight] = useState(document.documentElement.dataset.theme === "light");
  return (
    <button
      className="bell" id="theme-toggle" aria-pressed={light} title={light ? "Passa al tema scuro" : "Passa al tema chiaro"}
      aria-label="Tema chiaro"
      onClick={() => {
        const next = !light;
        setLight(next);
        if (next) document.documentElement.dataset.theme = "light";
        else delete document.documentElement.dataset.theme;
        try {
          localStorage.setItem("jarvis.theme", next ? "light" : "dark");
        } catch {
          /* archiviazione non disponibile */
        }
      }}
    >
      <Icon name="theme" />
    </button>
  );
}

export function TopBar() {
  const { health, unread, setUnread, show, notifTick } = useApp();
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<Notification[] | null>(null);
  const [browserAsk, setBrowserAsk] = useState("Notification" in window && Notification.permission === "default");

  const render = () =>
    api<{ unread: number; items: Notification[] }>("/api/notifications?limit=30").then((d) => {
      setUnread(d.unread);
      setItems(d.items);
    });

  useEffect(() => {
    if (open) render().catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, notifTick]);

  useEffect(() => {
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", esc);
    return () => document.removeEventListener("keydown", esc);
  }, []);

  const openNotification = async (n: Notification) => {
    if (!n.read_at) {
      try {
        const r = await post<{ unread: number }>(`/api/notifications/${n.notification_id}/read`);
        setUnread(r.unread);
      } catch {
        /* già letta */
      }
    }
    setOpen(false);
    if (n.link?.view) show(n.link.view);
  };

  return (
    <header className="topbar">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true"></span>
        <span className="brand-name">JARVIS</span> <small id="brand-version">{health?.core?.version ? `v${health.core.version}` : ""}</small>
      </div>
      <button className="palette-trigger" type="button" onClick={() => window.dispatchEvent(new Event("jarvis:palette"))} aria-label="Apri la palette comandi">
        <Icon name="search" size={16} />
        <span>Cerca o chiedi…</span>
        <kbd>{SHORTCUT}</kbd>
      </button>
      <div className="status" id="status" role="status" aria-live="polite">
        <AiPill />
        {health?.audit ? (
          <span className={`pill ${health.audit.chain_valid ? "ok" : "bad"}`} id="st-audit">
            Audit: {health.audit.events} eventi {health.audit.chain_valid ? "✓ integro" : "✗ alterato"}
          </span>
        ) : (
          <span className="pill" id="st-audit">Audit: …</span>
        )}
        <VoicePill />
        <ThemeToggle />
        <button
          className="bell" id="bell" aria-haspopup="true" aria-expanded={open} aria-controls="notif-panel" title="Notifiche"
          aria-label={`Notifiche, ${unread} non lette`} onClick={() => setOpen(!open)}
        >
          <Icon name="bell" />
          <span className="count" id="notif-count" hidden={!unread}>{unread > 99 ? "99+" : String(unread)}</span>
        </button>
      </div>
      <div className="notif-panel" id="notif-panel" hidden={!open} role="dialog" aria-label="Notifiche">
        <div className="notif-head">
          <strong>Notifiche</strong>
          <div>
            <button
              className="ghost small" id="notif-browser" hidden={!browserAsk}
              onClick={async () => {
                await Notification.requestPermission();
                setBrowserAsk(false);
              }}
            >
              Attiva sul desktop
            </button>
            <button className="ghost small" id="notif-readall" onClick={guard(async () => {
              await post("/api/notifications/read-all");
              await render();
            })}>
              Segna tutte come lette
            </button>
          </div>
        </div>
        <div id="notif-list" className="notif-list">
          {items && items.length ? (
            items.map((n) => (
              <button key={n.notification_id} className={`notif${n.read_at ? "" : " unread"}`} onClick={() => openNotification(n)}>
                <span className={`dot ${n.severity}`}></span>
                <span>
                  <div className="t">{n.title}</div>
                  {n.body ? <div className="b">{n.body}</div> : null}
                  <div className="m">
                    {fmtTime(n.created_at)} · {CATEGORY[n.category] || n.category}
                    {n.task_id ? " · task " + n.task_id.slice(0, 8) : ""}
                  </div>
                </span>
              </button>
            ))
          ) : (
            <p className="empty pad">Nessuna notifica.</p>
          )}
        </div>
      </div>
    </header>
  );
}

export function Toasts() {
  const { toasts, dismissToast, show, setUnread } = useApp();
  return (
    <div className="toasts" id="toasts" aria-live="assertive">
      {toasts.map((n) => {
        const key = n.dedupe_key || n.notification_id;
        return (
          <div
            key={key} className={`toast ${n.severity}`} role="status" data-key={key}
            onClick={async () => {
              dismissToast(key);
              if (!n.read_at && !n.notification_id.startsWith("local-")) {
                try {
                  setUnread((await post<{ unread: number }>(`/api/notifications/${n.notification_id}/read`)).unread);
                } catch {
                  /* già letta */
                }
              }
              if (n.link?.view) show(n.link.view);
            }}
          >
            <Icon name={n.severity === "error" || n.severity === "warning" ? "alert" : n.severity === "success" ? "check" : "info"} className="toast-icon" />
            <div className="toast-text">
              <div className="t">{n.title}</div>
              {n.body ? <div className="b">{n.body}</div> : null}
            </div>
            <i className="toast-timer" aria-hidden="true"></i>
          </div>
        );
      })}
    </div>
  );
}
