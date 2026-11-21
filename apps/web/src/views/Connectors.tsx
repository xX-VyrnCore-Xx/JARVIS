import { useEffect, useRef, useState } from "react";
import { initialParameters, missingParameters, OperationFields } from "../components/OperationFields";
import type { ParameterSchema, Parameters } from "../components/OperationFields";
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
  input_schema: ParameterSchema;
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
  const [parameters, setParameters] = useState<Parameters>(() => initialParameters(operation.input_schema));
  const [error, setError] = useState("");
  const confirmation = operation.effects === "write_external" || ["high", "critical"].includes(operation.risk);

  useEffect(() => { dialog.current?.showModal(); }, []);

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (thinking) return;
    const missing = missingParameters(operation.input_schema, parameters);
    if (missing.length) {
      setError(`Parametri obbligatori mancanti: ${missing.join(", ")}.`);
      return;
    }
    const [, id, name] = operation.tool.split(".");
    const command = `connettore ${id} ${name} ${JSON.stringify(parameters)}`;
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
        <OperationFields schema={operation.input_schema} values={parameters} onChange={(values) => { setParameters(values); setError(""); }} />
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

function TemplateCatalog({ installed, reload }: { installed: string[]; reload: () => void }) {
  const { data, error: loadError, reload: reloadCatalog } = useViewData(true, () => api<{ templates: Template[] }>("/api/connectors/templates"));
  const [clientIds, setClientIds] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [feedback, setFeedback] = useState<{ text: string; failed: boolean } | null>(null);
  const search = query.trim().toLocaleLowerCase("it-IT");
  const visible = data?.templates.filter((template) =>
    `${template.name} ${template.provider} ${template.category} ${template.description}`.toLocaleLowerCase("it-IT").includes(search)
  ) || [];

  async function install(template: Template) {
    setBusy(template.id);
    setFeedback(null);
    try {
      const item = await post<Connector>(`/api/connectors/templates/${encodeURIComponent(template.id)}`, {
        client_id: clientIds[template.id]?.trim() || undefined,
      });
      setFeedback({ text: `${item.name} installato.${item.authorized ? "" : " Completa l’autorizzazione in Installati."}`, failed: false });
      reload();
    } catch (error) {
      setFeedback({ text: (error as Error).message, failed: true });
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      <div className="row">
        <input type="search" placeholder="Cerca per nome o categoria" aria-label="Cerca nel catalogo" value={query} onChange={(event) => setQuery(event.target.value)} />
        {loadError ? <button type="button" className="ghost" onClick={reloadCatalog}>Riprova</button> : null}
      </div>
      {loadError ? <p className="form-error" role="alert">{loadError}</p> : null}
      {feedback ? <p className={feedback.failed ? "form-error" : "hint"} role={feedback.failed ? "alert" : "status"}>{feedback.text}</p> : null}
      {data && visible.length === 0 ? <p className="hint">Nessun connettore corrisponde alla ricerca.</p> : null}
      <div className="grid catalog" id="connector-catalog">
        {visible.map((t) => {
          const done = installed.includes(t.id);
          return (
            <div className="card template" key={t.id} data-template={t.id}>
              <div className="body">
                <div className="title">{t.name} <span className="sub">{t.category}</span></div>
                <p className="desc">{t.description}</p>
                <details>
                  <summary>Configurazione e operazioni</summary>
                  <p className="hint">{t.setup} <a href={t.setup_url} target="_blank" rel="noopener noreferrer">Documentazione</a></p>
                  <p className="hint">{t.operations.map((name) => name.replaceAll("_", " ")).join(", ")}</p>
                </details>
                {done ? null : (
                  <div className="row">
                    {t.needs_client_id ? (
                      <input name="client_id" placeholder="ID client OAuth" aria-label={`ID client per ${t.name}`} value={clientIds[t.id] || ""}
                        onChange={(e) => setClientIds({ ...clientIds, [t.id]: e.target.value })} />
                    ) : null}
                    <button type="button" className="primary" disabled={busy !== null || (t.needs_client_id && !clientIds[t.id]?.trim())}
                      onClick={() => void install(t)}>{busy === t.id ? "Installazione…" : "Installa"}</button>
                  </div>
                )}
              </div>
              <span className={`badge ${done ? "succeeded" : "pending"}`}>
                {done ? "installato" : t.auth === "oauth2" ? "OAuth" : t.auth === "none" ? "Accesso pubblico" : "Token"}
              </span>
            </div>
          );
        })}
      </div>
    </>
  );
}

type GuidedParameter = { id: number; name: string; type: string; required: boolean };

function GuidedConnector({ reload }: { reload: () => void }) {
  const [parameters, setParameters] = useState<GuidedParameter[]>([]);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<{ text: string; failed: boolean } | null>(null);
  const nextId = useRef(1);

  function editParameter(id: number, update: Partial<GuidedParameter>) {
    setParameters((items) => items.map((item) => item.id === id ? { ...item, ...update } : item));
  }

  async function install(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const fields = new FormData(form);
    const field = (name: string) => String(fields.get(name) || "").trim();
    setBusy(true);
    setStatus(null);
    try {
      const base = new URL(field("base"));
      if (!["https:", "http:"].includes(base.protocol) || base.username || base.password || base.search || base.hash) {
        throw new Error("Usa un URL API senza credenziali, parametri o frammento.");
      }
      const path = field("path");
      const pathNames = new Set([...path.matchAll(/\{([a-zA-Z][a-zA-Z0-9_]*)\}/g)].map((match) => match[1]));
      const properties = Object.fromEntries([...pathNames].map((name) => [name, { type: "string" }]));
      const required = [...pathNames];
      for (const parameter of parameters) {
        if (Object.hasOwn(properties, parameter.name)) {
          throw new Error(`Il parametro ${parameter.name} è già dichiarato.`);
        }
        Object.defineProperty(properties, parameter.name, { value: { type: parameter.type }, enumerable: true });
        if (parameter.required) required.push(parameter.name);
      }
      const name = field("name");
      const method = field("method");
      const item = await post<Connector>("/api/connectors/new", {
        contract: "jarvis.connector/1", id: field("id"), name, description: field("summary"),
        version: "1.0.0", base_url: base.href, allowed_hosts: [base.hostname],
        auth: { type: field("auth") },
        operations: [{
          name: "esegui", summary: field("summary"), method, path,
          effects: method === "GET" ? "read" : "write_external",
          input_schema: { type: "object", properties, required, additionalProperties: false },
        }],
      });
      form.reset();
      setParameters([]);
      setStatus({ text: `${item.name} installato.`, failed: false });
      reload();
    } catch (error) {
      setStatus({ text: (error as Error).message, failed: true });
    } finally {
      setBusy(false);
    }
  }

  return (
    <details className="connector-install">
      <summary>Configura un altro servizio API</summary>
      <form className="form" id="guided-connector-form" onSubmit={(event) => void install(event)}>
        <div className="row">
          <label>Nome<input name="name" required maxLength={100} placeholder="Stato servizi" /></label>
          <label>ID connettore<input name="id" required pattern="[a-z][a-z0-9_]{1,31}" placeholder="stato_servizi" /></label>
          <label>Autenticazione<select name="auth"><option value="none">Nessuna</option><option value="bearer">Token Bearer</option></select></label>
        </div>
        <label>URL base dell’API<input name="base" type="url" required maxLength={500} placeholder="https://api.esempio.it/v1" /></label>
        <div className="row">
          <label>Metodo<select name="method"><option>GET</option><option>POST</option></select></label>
          <label>Percorso<input name="path" required maxLength={300} placeholder="/servizi/{nome}" /></label>
          <label>Descrizione operazione<input name="summary" required maxLength={300} placeholder="Leggi lo stato del servizio" /></label>
        </div>
        <p className="hint">I nomi fra parentesi graffe nel percorso diventano campi obbligatori. Per POST, i campi aggiuntivi formano il corpo della richiesta.</p>
        {parameters.map((item) => (
          <div className="row" key={item.id}>
            <label>Parametro<input required pattern="[a-zA-Z][a-zA-Z0-9_]{0,63}" value={item.name}
              onChange={(event) => editParameter(item.id, { name: event.target.value })} /></label>
            <label>Tipo<select value={item.type} onChange={(event) => editParameter(item.id, { type: event.target.value })}>
              <option value="string">Testo</option><option value="number">Numero</option>
              <option value="integer">Intero</option><option value="boolean">Sì / no</option>
            </select></label>
            <label className="check"><input type="checkbox" checked={item.required} onChange={(event) => editParameter(item.id, { required: event.target.checked })} /> Obbligatorio</label>
            <button type="button" className="ghost" onClick={() => setParameters((items) => items.filter(({ id }) => id !== item.id))}>Rimuovi</button>
          </div>
        ))}
        <div className="row">
          <button type="button" className="ghost" disabled={parameters.length >= 20}
            onClick={() => setParameters((items) => [...items, { id: nextId.current++, name: "", type: "string", required: false }])}>Aggiungi parametro</button>
          <button type="submit" className="primary" disabled={busy}>{busy ? "Installazione…" : "Installa"}</button>
        </div>
        {status ? <p role={status.failed ? "alert" : "status"} className={status.failed ? "form-error" : "hint"}>{status.text}</p> : null}
      </form>
    </details>
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
      <p className="hint">Installa un servizio dal catalogo o configura la sua API. Le scritture verso l’esterno richiedono conferma in chat.</p>
      <h3 className="sub-head">Catalogo</h3>
      {active ? <TemplateCatalog installed={data?.items.map((c) => c.id) || []} reload={reload} /> : null}
      <GuidedConnector reload={reload} />
      <h3 className="sub-head">Installati</h3>
      <div id="connectors" className="list">
        {error ? <p className="form-error" role="alert">{error}</p> : null}
        {!data && !error ? <p className="hint" role="status">Caricamento dei connettori…</p> : null}
        {data && !data.items.length ? <Empty>Nessun connettore installato.</Empty> : null}
        {data?.items.map((c) => <ConnectorCard key={c.id} connector={c} reload={reload} select={setOperation} />)}
      </div>
      <details className="connector-install" id="connector-advanced">
        <summary>Importa manifest avanzato</summary>
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
      </details>
      {active && operation ? <OperationDialog operation={operation} onClose={() => setOperation(null)} /> : null}
    </section>
  );
}
