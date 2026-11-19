import { useEffect, useRef, useState } from "react";
import { Icon } from "../components/Icon";
import { api, post } from "../lib/api";
import { useApp, useViewData } from "../state";
import { Empty, ViewHead } from "./Simple";

interface Operation {
  tool: string;
  summary: string;
  method: string;
  effects: string;
  risk: string;
  input_schema: {
    required?: string[];
    properties?: Record<string, { type?: unknown; description?: string }>;
  };
}

interface Connector {
  id: string;
  name: string;
  version: string;
  description: string;
  base_url: string;
  auth: string;
  authorized: boolean;
  webhook: boolean;
  webhook_path: string;
  operations: Operation[];
}

function OperationDialog({ operation, onClose }: { operation: Operation; onClose: () => void }) {
  const { show, sendRef, thinking } = useApp();
  const dialog = useRef<HTMLDialogElement>(null);
  const [parameters, setParameters] = useState("{}");
  const [error, setError] = useState("");
  const required = operation.input_schema.required || [];
  const fields = Object.entries(operation.input_schema.properties || {});
  const confirmation = operation.effects === "write_external" || ["high", "critical"].includes(operation.risk);

  useEffect(() => { dialog.current?.showModal(); }, []);

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (thinking) return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(parameters);
    } catch {
      setError("Inserisci un oggetto JSON valido.");
      return;
    }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      setError("I parametri devono essere un oggetto JSON.");
      return;
    }
    const missing = required.filter((name) => !Object.prototype.hasOwnProperty.call(parsed, name));
    if (missing.length) {
      setError(`Parametri obbligatori mancanti: ${missing.join(", ")}.`);
      return;
    }
    const [, id, name] = operation.tool.split(".");
    const command = `connettore ${id} ${name} ${JSON.stringify(parsed)}`;
    if (Array.from(command).length > 8000) {
      setError("Riduci la lunghezza dei parametri a meno di 8.000 caratteri complessivi.");
      return;
    }
    dialog.current?.close();
    show("chat");
    sendRef.current(command);
  }

  return (
    <dialog className="setup connector-dialog" ref={dialog} onClose={onClose} aria-labelledby="operation-title">
      <form onSubmit={submit}>
        <h2 id="operation-title">{operation.summary}</h2>
        <p className="hint"><code>{operation.tool}</code> · {operation.method}{confirmation ? " · Richiede conferma in chat" : ""}</p>
        {fields.length ? (
          <dl className="operation-parameters">
            {fields.map(([name, field]) => (
              <div key={name}>
                <dt><code>{name}</code></dt>
                <dd>{[typeof field?.type === "string" ? field.type : "", required.includes(name) ? "obbligatorio" : "facoltativo", field?.description].filter(Boolean).join(" · ")}</dd>
              </div>
            ))}
          </dl>
        ) : null}
        <label className="operation-input">Parametri JSON
          <textarea rows={5} value={parameters} spellCheck={false} onChange={(e) => { setParameters(e.target.value); setError(""); }} />
        </label>
        {error ? <p className="form-error" role="alert">{error}</p> : null}
        <div className="row operation-actions">
          <button type="button" className="ghost" onClick={() => dialog.current?.close()}>Annulla</button>
          <button type="submit" className="primary" disabled={thinking}>Invia alla chat</button>
        </div>
      </form>
    </dialog>
  );
}

function ConnectorCard({ connector: c, reload, select }: { connector: Connector; reload: () => void; select: (op: Operation) => void }) {
  const cred = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [oauth, setOAuth] = useState(false);
  const [authorizeUrl, setAuthorizeUrl] = useState("");
  const base = `/api/connectors/${encodeURIComponent(c.id)}`;

  async function act(action: () => Promise<void>) {
    setBusy(true);
    setError("");
    try {
      await action();
      if (cred.current) cred.current.value = "";
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <article className="card connector-card" data-connector={c.id}>
      <header className="connector-head">
        <Icon name="connectors" className="connector-mark" />
        <div className="body">
          <div className="title">{c.name} <span className="sub">v{c.version}</span></div>
          <div className="sub">{c.description}</div>
        </div>
        <span className={`badge ${c.authorized ? "succeeded" : "pending"}`}>{c.authorized ? "autorizzato" : "da autorizzare"}</span>
      </header>
      <div className="sub connector-endpoint">{c.base_url} · {c.auth}{c.webhook ? ` · webhook ${c.webhook_path}` : ""}</div>
      <ul className="connector-operations">
        {c.operations.map((o) => (
          <li key={o.tool}>
            <span className="operation-method">{o.method}</span>
            <div className="body">
              <div>{o.summary}</div>
              <div className="sub"><code>{o.tool}</code>{o.effects === "write_external" || ["high", "critical"].includes(o.risk) ? " · Richiede conferma" : ""}</div>
            </div>
            <button type="button" className="ghost small" disabled={!c.authorized || busy} onClick={() => select(o)} aria-label={`Prepara ${o.summary}`}>Prepara</button>
          </li>
        ))}
      </ul>
      <div className="row">
        {c.auth === "oauth2" ? (
          <button className="primary" disabled={busy} onClick={() => { setOAuth(!oauth); setAuthorizeUrl(""); setError(""); }}>
            {c.authorized ? "Accedi di nuovo" : "Accedi"}
          </button>
        ) : c.auth !== "none" || !c.authorized ? (
          <form className="row" onSubmit={(e) => {
            e.preventDefault();
            void act(async () => {
              await post(`${base}/authorize`, { credential: cred.current?.value || "" });
              reload();
            });
          }}>
            {c.auth !== "none" ? <input ref={cred} type="password" required autoComplete="off" placeholder="Token o chiave API" aria-label={`Credenziale per ${c.name}`} /> : null}
            <button className="primary" type="submit" disabled={busy}>{c.authorized ? "Aggiorna credenziale" : "Autorizza"}</button>
          </form>
        ) : null}
        {c.authorized ? <button className="ghost" disabled={busy} onClick={() => void act(async () => {
          await post(`${base}/revoke`);
          setOAuth(false);
          setAuthorizeUrl("");
          reload();
        })}>Revoca</button> : null}
        <button className="danger" disabled={busy} onClick={() => {
          if (confirm(`Disinstallare ${c.name}? Credenziale e webhook verranno cancellati.`)) void act(async () => {
            await api(base, { method: "DELETE" });
            reload();
          });
        }}>Disinstalla</button>
      </div>
      {oauth ? (
        <form className="row" onSubmit={(e) => {
          e.preventDefault();
          setAuthorizeUrl("");
          void act(async () => {
            const { authorize_url } = await post<{ authorize_url: string }>(`${base}/oauth/start`, { client_secret: cred.current?.value || undefined });
            setAuthorizeUrl(authorize_url);
          });
        }}>
          <input ref={cred} type="password" autoComplete="off" placeholder="Segreto del client" aria-label={`Segreto del client per ${c.name}`}
            title="Inseriscilo se richiesto dal servizio, per esempio Google." />
          <button className="ghost" type="submit" disabled={busy}>Prepara accesso</button>
          {authorizeUrl ? <a href={authorizeUrl} target="_blank" rel="noopener noreferrer">Apri la pagina di accesso</a> : null}
        </form>
      ) : null}
      {error ? <p className="form-error" role="alert">{error}</p> : null}
    </article>
  );
}

interface Template {
  id: string; name: string; provider: string; category: string; auth: string; description: string;
  setup: string; setup_url: string; needs_client_id: boolean; needs_client_secret: boolean; operations: string[];
}

/** Catalogo dei connettori pronti (ADR-0047): Gmail, Calendar, Outlook, GitHub, Todoist, Slack. */
function TemplateCatalog({ installed, reload }: { installed: string[]; reload: () => void }) {
  const { data } = useViewData(true, () => api<{ templates: Template[] }>("/api/connectors/templates"));
  const [clientIds, setClientIds] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  if (!data) return null;
  return (
    <div className="grid catalog" id="connector-catalog">
      {data.templates.map((t) => {
        const done = installed.includes(t.id);
        return (
          <div className="card template" key={t.id} data-template={t.id}>
            <div className="body">
              <div className="title">{t.name} <span className="sub">{t.provider} · {t.category}</span></div>
              <p className="desc">{t.description}</p>
              <details>
                <summary>Come configurarlo</summary>
                <p className="hint">{t.setup} <a href={t.setup_url} target="_blank" rel="noopener noreferrer">Apri la console</a></p>
                <p className="hint">Operazioni: {t.operations.join(", ")}. Invii e creazioni chiedono sempre conferma.</p>
              </details>
              {done ? null : (
                <div className="row">
                  {t.needs_client_id ? (
                    <input name="client_id" placeholder="ID client OAuth" aria-label={`ID client per ${t.name}`} value={clientIds[t.id] || ""}
                      onChange={(e) => setClientIds({ ...clientIds, [t.id]: e.target.value })} />
                  ) : null}
                  <button type="button" className="primary" disabled={busy === t.id || (t.needs_client_id && !clientIds[t.id])}
                    onClick={async () => {
                      setBusy(t.id);
                      try {
                        await post(`/api/connectors/templates/${encodeURIComponent(t.id)}`, { client_id: clientIds[t.id] || undefined });
                        reload();
                      } catch (e) {
                        alert((e as Error).message);
                      } finally {
                        setBusy(null);
                      }
                    }}>
                    Aggiungi
                  </button>
                </div>
              )}
            </div>
            <span className={`badge ${done ? "succeeded" : "pending"}`}>{done ? "aggiunto" : t.auth === "oauth2" ? "OAuth" : "token"}</span>
          </div>
        );
      })}
    </div>
  );
}

export function Connectors({ active }: { active: boolean }) {
  const { data, error, reload } = useViewData(active, () => api<{ items: Connector[] }>("/api/connectors"));
  const [operation, setOperation] = useState<Operation | null>(null);
  const [manifest, setManifest] = useState("");
  const [secret, setSecret] = useState<string | null>(null);
  const [installError, setInstallError] = useState("");
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    if (!active) return;
    const refresh = () => { if (document.visibilityState === "visible") reload(); };
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    return () => {
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", refresh);
    };
  }, [active, reload]);

  async function install(e: React.FormEvent) {
    e.preventDefault();
    setInstallError("");
    let parsed: unknown;
    try {
      parsed = JSON.parse(manifest);
    } catch {
      setInstallError("Il manifest non è JSON valido.");
      return;
    }
    setInstalling(true);
    try {
      const out = await post<{ webhook_secret?: string }>("/api/connectors", parsed);
      setManifest("");
      setSecret(out.webhook_secret || null);
      reload();
    } catch (err) {
      setInstallError((err as Error).message);
    } finally {
      setInstalling(false);
    }
  }

  return (
    <section className={`view${active ? " active" : ""}`} id="view-connectors" aria-label="Connettori">
      <ViewHead title="Connettori" refreshKey="connectors" onRefresh={reload} />
      <p className="hint">Collega servizi esterni tramite un manifest <code>jarvis.connector/1</code>. Le scritture verso l’esterno richiedono conferma in chat.</p>
      <h3 className="sub-head">Catalogo</h3>
      {active ? <TemplateCatalog installed={data?.items.map((c) => c.id) || []} reload={reload} /> : null}
      <h3 className="sub-head">Installati</h3>
      <div id="connectors" className="list">
        {error ? <p className="form-error" role="alert">{error}</p> : null}
        {!data && !error ? <p className="hint" role="status">Caricamento dei connettori…</p> : null}
        {data && !data.items.length ? <Empty>Nessun connettore installato.</Empty> : null}
        {data?.items.map((c) => <ConnectorCard key={c.id} connector={c} reload={reload} select={setOperation} />)}
      </div>
      <form id="connector-form" className="form" onSubmit={install}>
        <label>Manifest del connettore (JSON)
          <textarea name="manifest" rows={6} required spellCheck={false} value={manifest} onChange={(e) => setManifest(e.target.value)}
            placeholder='{"contract": "jarvis.connector/1", "id": "meteo", ...}' />
        </label>
        <div className="row"><button type="submit" className="primary" disabled={installing}>{installing ? "Installazione…" : "Installa"}</button></div>
        {installError ? <p className="form-error" role="alert">{installError}</p> : null}
        <p className="hint" id="connector-secret" hidden={!secret}>
          {secret ? <>Segreto del webhook (mostrato una sola volta). Usalo per firmare <code>X-Jarvis-Signature</code> con HMAC-SHA256.<br /><code>{secret}</code></> : null}
        </p>
      </form>
      {active && operation ? <OperationDialog operation={operation} onClose={() => setOperation(null)} /> : null}
    </section>
  );
}
