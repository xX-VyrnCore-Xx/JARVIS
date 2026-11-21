// Viste elenco: Attività, Workspace, Audit e Dispositivi.

import { useEffect, useMemo, useState } from "react";
import qrcode from "qrcode-generator";
import { Icon } from "../components/Icon";
import { api, fmtTime, guard, post } from "../lib/api";
import { useApp, useViewData } from "../state";

export function ViewHead({ title, onRefresh, refreshKey }: { title: string; onRefresh: () => void; refreshKey: string }) {
  return (
    <div className="view-head">
      <h2>{title}</h2>
      <button className="ghost" data-refresh={refreshKey} onClick={onRefresh}>Aggiorna</button>
    </div>
  );
}

export const Empty = ({ children }: { children: React.ReactNode }) => <p className="empty">{children}</p>;

interface Span {
  span_id: string; name: string; kind: string; start: string; duration_ms: number; status: string;
  attributes?: Record<string, unknown>;
}
const SPAN_KIND: Record<string, string> = {
  task: "richiesta", agent: "agente", policy: "policy", tool: "strumento", connector: "connettore",
  node: "nodo", web: "web", terminal: "terminale", confirmation: "conferma",
};

/** Traccia del task (ADR-0039): cronologia degli intervalli con durata ed esito. */
function TraceView({ taskId }: { taskId: string }) {
  const [trace, setTrace] = useState<{ total_ms: number | null; spans: Span[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api<{ total_ms: number | null; spans: Span[] }>(`/api/tasks/${taskId}/trace`).then(setTrace, (e) => setError((e as Error).message));
  }, [taskId]);
  if (error) return <p className="sub">{error}</p>;
  if (!trace) return <p className="sub">Caricamento della traccia…</p>;
  const spans = [...trace.spans].sort((a, b) => Date.parse(a.start) - Date.parse(b.start) || (a.kind === "task" ? -1 : 1));
  const t0 = Math.min(...spans.map((s) => Date.parse(s.start)));
  const end = Math.max(...spans.map((s) => Date.parse(s.start) + s.duration_ms));
  const total = Math.max(end - t0, 1);
  return (
    <div className="trace" role="list" aria-label="Traccia del task">
      {spans.map((s) => {
        const left = ((Date.parse(s.start) - t0) / total) * 100;
        const width = Math.max((s.duration_ms / total) * 100, 0.8);
        return (
          <div className={`span-row ${s.kind}`} role="listitem" key={s.span_id}>
            <span className="span-label">{s.kind === "task" ? "richiesta completa" : `${SPAN_KIND[s.kind] || s.kind} · ${s.name}`}</span>
            <span className="span-track">
              <i className={`span-bar ${s.status}`} style={{ left: `${Math.min(left, 99.2)}%`, width: `${Math.min(width, 100 - left)}%` }} />
            </span>
            <span className="span-ms">{s.duration_ms < 1 ? "<1" : Math.round(s.duration_ms)} ms</span>
          </div>
        );
      })}
    </div>
  );
}

export function Tasks({ active }: { active: boolean }) {
  const { activity } = useApp();
  const { data, reload } = useViewData(active, () => api("/api/tasks?limit=50"), [activity], 5000);
  const items: any[] = data?.items || [];
  const [open, setOpen] = useState<string | null>(null);
  return (
    <section className={`view${active ? " active" : ""}`} id="view-tasks" aria-label="Attività">
      <ViewHead title="Attività" refreshKey="tasks" onRefresh={reload} />
      <p className="hint">Ogni richiesta è un task con stati verificati dal contratto <code>jarvis.task/1</code>.</p>
      <div id="tasks" className="list">
        {data && !items.length ? <Empty>Nessuna attività.</Empty> : null}
        {items.map((t) => (
          <div className="card" key={t.task_id}>
            <div className="body">
              <div className="title">{t.objective}</div>
              <div className="sub">{t.task_id} · {fmtTime(t.created_at)}{t.parent_task_id ? ` · agente temporaneo del task ${t.parent_task_id.slice(0, 8)}` : ""}</div>
              <ol className="history">
                {t.history.map((h: any, i: number) => <li key={i}>{h.to} — {h.reason || ""}</li>)}
              </ol>
              {t.error ? <div className="sub">errore: {t.error.code} — {t.error.message}</div> : null}
              <button type="button" className="ghost small trace-toggle" onClick={() => setOpen(open === t.task_id ? null : t.task_id)}>
                {open === t.task_id ? "Nascondi traccia" : "Mostra traccia"}
              </button>
              {open === t.task_id ? <TraceView taskId={t.task_id} /> : null}
            </div>
            <span className={`badge ${t.state}`}>{t.state}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

export function Workspace({ active }: { active: boolean }) {
  const { activity } = useApp();
  const { data, reload } = useViewData(active, () => api("/api/workspace"), [activity], 8000);
  const entries: any[] = data?.entries || [];
  return (
    <section className={`view${active ? " active" : ""}`} id="view-workspace" aria-label="Workspace">
      <ViewHead title="Workspace" refreshKey="workspace" onRefresh={reload} />
      <p className="hint">L'unica cartella in cui gli strumenti file possono leggere e scrivere.</p>
      <div id="workspace" className="list">
        {data && !entries.length ? <Empty>Workspace vuota. Prova: "scrivi note.txt: …"</Empty> : null}
        {entries.map((e) => (
          <div className="card" key={e.name}>
            <div className="body">
              <div className="title">{e.name + (e.kind === "dir" ? "/" : "")}</div>
              <div className="sub">{e.size} byte</div>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

export function Audit({ active }: { active: boolean }) {
  const { activity } = useApp();
  const { data, reload } = useViewData(active, () => api("/api/audit?limit=100"), [activity]);
  return (
    <section className={`view${active ? " active" : ""}`} id="view-audit" aria-label="Audit">
      <ViewHead title="Audit" refreshKey="audit" onRefresh={reload} />
      <p className="hint" id="audit-summary">
        {data ? `${data.total} eventi · catena di hash ${data.chain_valid ? "integra ✓" : "ALTERATA ✗ " + data.chain_errors.join("; ")}` : ""}
        {data?.signatures?.enabled ? (
          <span id="audit-signatures">
            {` · firme Ed25519 ${data.signatures.valid ? "valide ✓" : "NON VALIDE ✗ " + data.signatures.errors.join("; ")} (${data.signatures.signed} firmati${data.signatures.unsigned_before_activation ? `, ${data.signatures.unsigned_before_activation} precedenti alla firma` : ""})`}
            <br />
            <span className="sub">Chiave pubblica: <code>{data.signatures.public_key}</code> (conservala fuori dal computer per verifiche indipendenti)</span>
          </span>
        ) : null}
      </p>
      <div id="audit" className="list">
        {data && !data.events.length ? <Empty>Nessun evento.</Empty> : null}
        {(data?.events || []).map((e: any) => (
          <div className="card" key={e.sequence}>
            <div className="body">
              <div className="title">#{e.sequence} {e.action}</div>
              <div className="sub">{fmtTime(e.at)} · {e.actor.kind}:{e.actor.id}{e.error_code ? " · " + e.error_code : ""}</div>
              <div className="sub">hash {e.hash.slice(0, 16)}… ← {e.prev_hash.slice(0, 16)}…</div>
            </div>
            <span className={`badge ${e.outcome}`}>{e.outcome}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

const SEVERITY_LABEL: Record<string, string> = { success: "tutte", info: "informazioni e oltre", warning: "avvisi e conferme", error: "solo errori" };

/** Notifiche sul telefono (ADR-0026): canali ntfy e Telegram. */
function PhoneChannels({ active }: { active: boolean }) {
  const { data, reload } = useViewData(active, () => api("/api/notify/channels"));
  const [kind, setKind] = useState("ntfy");
  const [name, setName] = useState("");
  const [target, setTarget] = useState("");
  const [server, setServer] = useState("");
  const [secret, setSecret] = useState("");
  const [min, setMin] = useState("warning");
  const [body, setBody] = useState(false);
  const items: any[] = data?.items || [];

  const add = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await post("/api/notify/channels", { kind, name, target, server: server || undefined, secret: secret || undefined, min_severity: min, include_body: body });
      setName("");
      setTarget("");
      setServer("");
      setSecret("");
      reload();
    } catch (err) {
      alert((err as Error).message);
    }
  };

  return (
    <>
      <h3 className="sub-head">Notifiche sul telefono</h3>
      <p className="hint">
        Ricevi conferme in attesa, errori e avvisi anche lontano dal computer. Per impostazione si inviano solo titolo e categoria:
        il testo esce dal computer solo se lo scegli. Le conferme si danno sempre da JARVIS.{" "}
        {kind === "ntfy"
          ? "ntfy: installa l'app ntfy, iscriviti a un argomento difficile da indovinare e scrivilo qui (server predefinito ntfy.sh, oppure il tuo)."
          : "Telegram: crea un bot con @BotFather, scrivigli un messaggio e inserisci qui il token del bot e l'identificativo numerico della tua chat."}
      </p>
      <form id="channel-form" className="form" onSubmit={add}>
        <div className="row">
          <label>Servizio
            <select name="kind" value={kind} onChange={(e) => setKind(e.target.value)}>
              <option value="ntfy">ntfy (push)</option>
              <option value="telegram">Telegram</option>
            </select>
          </label>
          <label>Nome <input name="name" maxLength={60} placeholder="Es. Il mio telefono" value={name} onChange={(e) => setName(e.target.value)} /></label>
          <label>{kind === "ntfy" ? "Argomento" : "ID della chat"}
            <input name="target" required value={target} onChange={(e) => setTarget(e.target.value)} placeholder={kind === "ntfy" ? "jarvis-casa-7f3k2x" : "123456789"} />
          </label>
        </div>
        <div className="row">
          <label>{kind === "ntfy" ? "Token di accesso (facoltativo)" : "Token del bot"}
            <input name="secret" type="password" autoComplete="off" required={kind === "telegram"} value={secret} onChange={(e) => setSecret(e.target.value)} />
          </label>
          <label>Server (facoltativo)
            <input name="server" placeholder={kind === "ntfy" ? "https://ntfy.sh" : "https://api.telegram.org"} value={server} onChange={(e) => setServer(e.target.value)} />
          </label>
          <label>Invia
            <select name="min_severity" value={min} onChange={(e) => setMin(e.target.value)}>
              {Object.entries(SEVERITY_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
          </label>
        </div>
        <div className="row">
          <label className="check"><input type="checkbox" name="include_body" checked={body} onChange={(e) => setBody(e.target.checked)} /> Includi il testo della notifica</label>
          <button type="submit" className="primary">Aggiungi canale</button>
        </div>
      </form>
      <div id="channels" className="list">
        {data && !items.length ? <Empty>Nessun canale: le notifiche restano nella campanella di JARVIS.</Empty> : null}
        {items.map((c) => (
          <div className="card" key={c.channel_id}>
            <div className="body">
              <div className="title">{c.name} <span className="sub">{c.kind}</span></div>
              <div className="sub">
                {c.kind === "ntfy" ? `argomento ${c.target}` : `chat ${c.target}`} · {c.server} · {SEVERITY_LABEL[c.min_severity] || c.min_severity} ·{" "}
                {c.include_body ? "con il testo" : "solo titolo"} · {c.has_secret ? "token salvato" : "senza token"}
              </div>
            </div>
            <button className="ghost" onClick={guard(async () => {
              await post(`/api/notify/channels/${encodeURIComponent(c.channel_id)}/test`);
              alert("Notifica di prova inviata: controlla il telefono.");
            })}>Prova</button>
            <button className="danger" onClick={guard(async () => {
              if (!confirm(`Rimuovere il canale «${c.name}»? Il token verrà cancellato.`)) return;
              await api(`/api/notify/channels/${encodeURIComponent(c.channel_id)}`, { method: "DELETE" });
              reload();
            })}>Rimuovi</button>
          </div>
        ))}
      </div>
    </>
  );
}

const PLATFORM: Record<string, string> = { android: "Android", ios: "iOS", desktop: "desktop", web: "browser", other: "altro" };

interface PairedDevice {
  device_id: string;
  name: string;
  platform: string;
  scopes: string[];
  slug?: string;
  capabilities?: string[];
  enrolled_at: string;
  last_seen_at?: string;
  revoked_at?: string;
}

interface Pairing {
  code: string;
  expires_at: string;
  scopes: string[];
}

export function Devices({ active }: { active: boolean }) {
  const { data, error, reload } = useViewData(active, async () => {
    const [devices, nodes] = await Promise.all([
      api<{ items: PairedDevice[]; pairing_urls: string[] }>("/api/devices"),
      api<{ items: { device_id: string; online: boolean }[] }>("/api/nodes"),
    ]);
    return { ...devices, online: new Map(nodes.items.map((node) => [node.device_id, node.online])) };
  });
  const [confirmScope, setConfirmScope] = useState(false);
  const [node, setNode] = useState(false);
  const [selectedAddress, setSelectedAddress] = useState("");
  const [pairing, setPairing] = useState<Pairing | null>(null);
  const [pending, setPending] = useState(false);
  const [now, setNow] = useState(Date.now);
  const [feedback, setFeedback] = useState<{ text: string; failed: boolean } | null>(null);
  const [query, setQuery] = useState("");
  const address = selectedAddress || data?.pairing_urls?.[0] || "";
  const seconds = pairing ? Math.max(0, Math.ceil((Date.parse(pairing.expires_at) - now) / 1000)) : 0;
  const code = pairing ? `${pairing.code.slice(0, 4)}-${pairing.code.slice(4)}` : "";
  const nodeInvite = pairing?.scopes.includes("node");
  const invite = seconds && address && !nodeInvite ? `${address}/pair#code=${encodeURIComponent(code)}` : "";
  const qr = useMemo(() => {
    if (!invite) return "";
    const image = qrcode(0, "M");
    image.addData(invite);
    image.make();
    return image.createDataURL(5, 20);
  }, [invite]);
  const term = query.trim().toLocaleLowerCase("it-IT");
  const filtered = data?.items.filter((item) => `${item.name} ${item.slug || ""} ${PLATFORM[item.platform] || item.platform}`.toLocaleLowerCase("it-IT").includes(term));

  useEffect(() => {
    if (!active || !pairing || !seconds) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [active, pairing, Boolean(seconds)]);
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = window.setInterval(reload, 8000);
    return () => window.clearInterval(timer);
  }, [active, reload]);

  async function generate(event: React.FormEvent) {
    event.preventDefault();
    setPending(true);
    setFeedback(null);
    try {
      const scopes = node ? ["node"] : confirmScope ? ["chat", "confirm"] : ["chat"];
      const next = await post<Pairing>("/api/devices/pairing", { scopes, replace: pairing?.code });
      setNow(Date.now());
      setPairing(next);
    } catch (err) {
      setFeedback({ text: (err as Error).message, failed: true });
    } finally {
      setPending(false);
    }
  }

  async function cancel() {
    if (!pairing) return;
    setPending(true);
    try {
      await post("/api/devices/pairing/cancel", { code: pairing.code });
      setPairing(null);
      setFeedback({ text: "Invito chiuso.", failed: false });
    } catch (err) {
      setFeedback({ text: (err as Error).message, failed: true });
    } finally {
      setPending(false);
    }
  }

  async function copy(value: string) {
    try {
      await navigator.clipboard.writeText(value);
      setFeedback({ text: "Copiato.", failed: false });
    } catch {
      setFeedback({ text: "Seleziona e copia il testo manualmente.", failed: true });
    }
  }

  return (
    <section className={`view${active ? " active" : ""}`} id="view-devices" aria-label="Dispositivi">
      <ViewHead title="Dispositivi" refreshKey="devices" onRefresh={reload} />
      <div className="device-enrol">
        <h3><Icon name="devices" /> Associa un dispositivo</h3>
        <form id="pairing-form" className="form" onSubmit={(event) => void generate(event)}>
          <div className="row">
            <label className="check">
              <input type="checkbox" name="node" checked={node} onChange={(e) => setNode(e.target.checked)} /> Nodo esecutore
            </label>
            <label className="check">
              <input type="checkbox" name="confirm" disabled={node} checked={confirmScope && !node} onChange={(e) => setConfirmScope(e.target.checked)} /> Consenti conferme dal dispositivo
            </label>
            <button type="submit" className="primary" disabled={pending}>{pending ? "Attendi…" : pairing ? "Genera nuovo codice" : "Genera codice di associazione"}</button>
          </div>
          <p className="hint">{node ? "Associa un computer che può eseguire gli strumenti assegnati dal Core." : "Usa JARVIS da un telefono o dal browser di un altro computer."}</p>
          {(data?.pairing_urls?.length || 0) > 1 ? (
            <label>Indirizzo del Core<select value={address} onChange={(event) => setSelectedAddress(event.target.value)}>
              {data?.pairing_urls.map((url) => <option key={url}>{url}</option>)}
            </select></label>
          ) : null}
        </form>
        {data && !address ? <details>
          <summary>Configura l’accesso da altri dispositivi</summary>
          <p className="hint">Configura un proxy HTTPS fidato, ad esempio <code>tailscale serve 8765</code>. Avvia JARVIS con <code>JARVIS_ALLOWED_HOSTS=&lt;nome-host&gt;</code>. L’indirizzo HTTPS comparirà qui al riavvio.</p>
        </details> : null}
        <div className="pairing-code" id="pairing-code" hidden={!pairing}>
          {pairing ? seconds ? (
            <div className="device-invite">
              {qr ? <img width="164" height="164" src={qr} alt="Codice QR per associare il dispositivo" /> : null}
              <div>
                <strong>{code}</strong>
                <p className="hint">Monouso · scade tra {Math.floor(seconds / 60)}:{String(seconds % 60).padStart(2, "0")}</p>
                {nodeInvite ? (
                  <div className="sub node-cmd">
                    <p>Su macOS o Linux esegui <code>./jarvis.sh node {address || "<indirizzo del Core>"} {code}</code></p>
                    <p>Su Windows esegui <code>jarvis.cmd node {address || "<indirizzo del Core>"} {code}</code></p>
                  </div>
                ) : invite ? <p className="hint">Scansiona il QR o invia l’invito al dispositivo da collegare.</p>
                  : <p className="hint">Apri l’indirizzo HTTPS del Core sul dispositivo e inserisci il codice.</p>}
                <div className="row">
                  <button type="button" className="ghost" onClick={() => void copy(code)}>Copia codice</button>
                  {invite ? <button type="button" className="ghost" data-invite={invite} onClick={() => void copy(invite)}>Copia invito</button> : null}
                  <button type="button" className="ghost" disabled={pending} onClick={() => void cancel()}>Annulla invito</button>
                </div>
              </div>
            </div>
          ) : <p className="hint">Codice scaduto. Generane uno nuovo per associare un dispositivo.</p> : null}
        </div>
        {feedback ? <p className={feedback.failed ? "form-error" : "hint"} role={feedback.failed ? "alert" : "status"}>{feedback.text}</p> : null}
      </div>
      <div className="row device-list-head">
        <h3>Dispositivi associati</h3>
        <input type="search" aria-label="Cerca un dispositivo" placeholder="Nome o piattaforma" value={query} onChange={(event) => setQuery(event.target.value)} />
      </div>
      <div id="devices" className="list">
        {error ? <p className="form-error" role="alert">{error}</p> : null}
        {data && !data.items.length ? <Empty>Nessun dispositivo associato.</Empty> : null}
        {data?.items.length && !filtered?.length ? <Empty>Nessun dispositivo corrisponde alla ricerca.</Empty> : null}
        {filtered?.map((d) => (
          <div className="card device-card" key={d.device_id}>
            <span className="device-icon"><Icon name={d.slug ? "node" : "devices"} /></span>
            <div className="body">
              <div className="title">{d.name} <span className="sub">{PLATFORM[d.platform] || d.platform}</span></div>
              {d.slug ? (
                <div className="sub">Nodo {d.slug} · {data?.online.get(d.device_id) ? "In linea" : "Non in linea"} · {(d.capabilities || []).join(", ")}</div>
              ) : (
                <div className="sub">Ultimo accesso {d.last_seen_at ? fmtTime(d.last_seen_at) : "mai"} · {d.scopes.includes("confirm") ? "Chat e conferme" : "Chat"}</div>
              )}
            </div>
            {d.revoked_at ? (
              <span className="badge denied">revocato {fmtTime(d.revoked_at)}</span>
            ) : (
              <button className="danger" onClick={guard(async () => {
                if (!confirm(`Revocare «${d.name}»? Perderà subito l'accesso.`)) return;
                await post(`/api/devices/${encodeURIComponent(d.device_id)}/revoke`);
                reload();
              })}>Revoca</button>
            )}
          </div>
        ))}
      </div>
      <PhoneChannels active={active} />
    </section>
  );
}
