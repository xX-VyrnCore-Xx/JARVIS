// Viste elenco: Attività, Workspace, Audit, Dispositivi e Connettori.

import { useRef, useState } from "react";
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

export function Tasks({ active }: { active: boolean }) {
  const { activity } = useApp();
  const { data, reload } = useViewData(active, () => api("/api/tasks?limit=50"), [activity]);
  const items: any[] = data?.items || [];
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
              <div className="sub">{t.task_id} · {fmtTime(t.created_at)}</div>
              <ol className="history">
                {t.history.map((h: any, i: number) => <li key={i}>{h.to} — {h.reason || ""}</li>)}
              </ol>
              {t.error ? <div className="sub">errore: {t.error.code} — {t.error.message}</div> : null}
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
  const { data, reload } = useViewData(active, () => api("/api/workspace"), [activity]);
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

export function Devices({ active }: { active: boolean }) {
  const { data, reload } = useViewData(active, async () => {
    const [devices, nodes] = await Promise.all([api("/api/devices"), api("/api/nodes")]);
    return { devices: devices.items as any[], online: new Map<string, boolean>(nodes.items.map((n: any) => [n.device_id, n.online])) };
  });
  const [confirmScope, setConfirmScope] = useState(false);
  const [node, setNode] = useState(false);
  const [pairing, setPairing] = useState<{ code: string; expires: string; node: boolean } | null>(null);

  const generate = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      const scopes = node ? ["node"] : confirmScope ? ["chat", "confirm"] : ["chat"];
      const out = await post("/api/devices/pairing", { scopes });
      setPairing({ code: `${out.code.slice(0, 4)}-${out.code.slice(4)}`, expires: out.expires_at, node });
    } catch (err) {
      alert((err as Error).message);
    }
  };

  return (
    <section className={`view${active ? " active" : ""}`} id="view-devices" aria-label="Dispositivi">
      <ViewHead title="Dispositivi" refreshKey="devices" onRefresh={reload} />
      <p className="hint">
        Usa JARVIS dal telefono senza esporlo su Internet: pubblicalo con un proxy fidato verso questo computer (per esempio{" "}
        <code>tailscale serve 8765</code>), avvia JARVIS con <code>JARVIS_ALLOWED_HOSTS=&lt;nome-host&gt;</code>, apri l'indirizzo sul
        telefono e inserisci il codice. Dal telefono puoi chattare, vedere attività e notifiche e, se lo consenti, confermare azioni; il
        resto si gestisce solo da qui.
      </p>
      <form id="pairing-form" className="form" onSubmit={generate}>
        <div className="row">
          <label className="check">
            <input type="checkbox" name="confirm" checked={confirmScope} onChange={(e) => setConfirmScope(e.target.checked)} /> Il dispositivo
            può confermare azioni rischiose
          </label>
          <label className="check">
            <input type="checkbox" name="node" checked={node} onChange={(e) => setNode(e.target.checked)} /> Nodo esecutore (un altro
            computer che esegue strumenti per JARVIS)
          </label>
          <button type="submit" className="primary">Genera codice di associazione</button>
        </div>
        <p className="pairing-code" id="pairing-code" hidden={!pairing}>
          {pairing ? (
            <>
              <strong>{pairing.code}</strong> valido fino alle {new Date(pairing.expires).toLocaleTimeString("it-IT")}, una sola volta.
              {pairing.node ? (
                <span className="sub node-cmd">
                  {" "}Sull'altro computer, nella cartella di JARVIS: <code>./jarvis.sh node &lt;indirizzo del Core&gt; {pairing.code}</code> (Windows:{" "}
                  <code>jarvis.cmd node &lt;indirizzo&gt; {pairing.code}</code>). L'indirizzo è quello del proxy fidato, per esempio
                  https://&lt;computer&gt;.ts.net.
                </span>
              ) : null}
            </>
          ) : null}
        </p>
      </form>
      <div id="devices" className="list">
        {data && !data.devices.length ? <Empty>Nessun dispositivo associato.</Empty> : null}
        {(data?.devices || []).map((d) => (
          <div className="card" key={d.device_id}>
            <div className="body">
              <div className="title">{d.name} <span className="sub">{PLATFORM[d.platform] || d.platform}</span></div>
              {d.slug ? (
                <div className="sub">
                  nodo esecutore «{d.slug}» · {data!.online.get(d.device_id) ? "in linea" : "non in linea"} · strumenti:{" "}
                  {(d.capabilities || []).join(", ")} · in chat: «nodo {d.slug} system.info»
                </div>
              ) : (
                <div className="sub">
                  associato {fmtTime(d.enrolled_at)} · ultimo accesso {d.last_seen_at ? fmtTime(d.last_seen_at) : "mai"} · permessi:{" "}
                  {d.scopes.join(", ")}
                </div>
              )}
            </div>
            {d.revoked_at ? (
              <span className="badge denied">revocato {fmtTime(d.revoked_at)}</span>
            ) : (
              <button className="danger" onClick={guard(async () => {
                if (!confirm(`Revocare «${d.name}»? Perderà subito l'accesso.`)) return;
                await post(`/api/devices/${encodeURIComponent(d.device_id)}/revoke`);
                reload();
              })}>
                Revoca
              </button>
            )}
          </div>
        ))}
      </div>
      <PhoneChannels active={active} />
    </section>
  );
}

function ConnectorCard({ c, reload }: { c: any; reload: () => void }) {
  const cred = useRef<HTMLInputElement>(null);
  const act = (path: string, method: string, body?: () => unknown) =>
    guard(async () => {
      await api(path, { method, body: body ? JSON.stringify(body()) : undefined });
      reload();
    });
  const id = encodeURIComponent(c.id);
  return (
    <div className="card">
      <div className="body">
        <div className="title">{c.name} <span className="sub">{c.id} · v{c.version}</span></div>
        <div className="sub">{c.base_url} · autenticazione {c.auth}{c.webhook ? ` · webhook ${c.webhook_path}` : ""}</div>
        <ul className="history">
          {(c.operations || []).map((o: any) => (
            <li key={o.tool}>
              {o.tool} — {o.summary} ({o.method}, rischio {o.risk}{o.effects === "write_external" ? ", con conferma" : ""})
            </li>
          ))}
        </ul>
        <div className="row">
          {c.auth === "none" || c.auth === "oauth2" ? null : (
            <input ref={cred} type="password" autoComplete="off" placeholder="token o chiave API" aria-label={`Credenziale per ${c.name}`} />
          )}
          {c.auth === "oauth2" ? (
            <button className="primary" onClick={guard(async () => {
              // OAuth (ADR-0027): la pagina di consenso si apre nel browser; il servizio torna a JARVIS da solo.
              const secret = prompt("Segreto del client, solo se il servizio lo richiede (altrimenti lascia vuoto):", "");
              if (secret === null) return;
              const { authorize_url } = await post(`/api/connectors/${id}/oauth/start`, { client_secret: secret || undefined });
              window.open(authorize_url, "_blank", "noopener");
              alert("Completa l'accesso nella pagina del servizio, poi premi «Aggiorna».");
            })}>
              {c.authorized ? "Accedi di nuovo" : "Accedi"}
            </button>
          ) : (
            <button className="primary" onClick={act(`/api/connectors/${id}/authorize`, "POST", () => ({ credential: cred.current?.value || "" }))}>
              {c.authorized ? "Aggiorna credenziale" : "Autorizza"}
            </button>
          )}
          {c.authorized ? <button className="ghost" onClick={act(`/api/connectors/${id}/revoke`, "POST")}>Revoca</button> : null}
          <button className="danger" onClick={() => {
            if (confirm(`Disinstallare ${c.name}? Credenziale e webhook verranno cancellati.`)) act(`/api/connectors/${id}`, "DELETE")();
          }}>
            Disinstalla
          </button>
        </div>
      </div>
      <span className={`badge ${c.authorized ? "succeeded" : "pending"}`}>{c.authorized ? "autorizzato" : "da autorizzare"}</span>
    </div>
  );
}

export function Connectors({ active }: { active: boolean }) {
  const { data, reload } = useViewData(active, () => api("/api/connectors"));
  const [manifest, setManifest] = useState("");
  const [secret, setSecret] = useState<string | null>(null);
  const items: any[] = data?.items || [];
  const install = async (e: React.FormEvent) => {
    e.preventDefault();
    let parsed: unknown;
    try {
      parsed = JSON.parse(manifest);
    } catch {
      alert("Il manifest non è JSON valido");
      return;
    }
    try {
      const out = await post("/api/connectors", parsed);
      setManifest("");
      setSecret(out.webhook_secret || null);
      reload();
    } catch (err) {
      alert((err as Error).message);
    }
  };
  return (
    <section className={`view${active ? " active" : ""}`} id="view-connectors" aria-label="Connettori">
      <ViewHead title="Connettori" refreshKey="connectors" onRefresh={reload} />
      <p className="hint">
        Servizi esterni descritti da un manifest <code>jarvis.connector/1</code>. Le credenziali restano cifrate nel Core e non vengono mai
        mostrate; le scritture verso l'esterno chiedono sempre conferma. In chat: «connettore &lt;id&gt; &lt;operazione&gt; chiave=valore».
      </p>
      <div id="connectors" className="list">
        {data && !items.length ? <Empty>Nessun connettore installato.</Empty> : null}
        {items.map((c) => <ConnectorCard key={c.id} c={c} reload={reload} />)}
      </div>
      <form id="connector-form" className="form" onSubmit={install}>
        <label>
          Manifest del connettore (JSON)
          <textarea name="manifest" rows={8} required spellCheck={false} value={manifest} onChange={(e) => setManifest(e.target.value)}
            placeholder='{"contract": "jarvis.connector/1", "id": "meteo", ...}' />
        </label>
        <div className="row"><button type="submit" className="primary">Installa</button></div>
        <p className="hint" id="connector-secret" hidden={!secret}>
          {secret ? `Segreto del webhook (mostrato una sola volta, usalo per firmare X-Jarvis-Signature con HMAC-SHA256): ${secret}` : ""}
        </p>
      </form>
    </section>
  );
}
