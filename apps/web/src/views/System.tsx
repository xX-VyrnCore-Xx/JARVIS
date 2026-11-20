import { useState } from "react";
import { CloudSettings } from "../components/CloudSettings";
import { api, AUTH, download, fmtTime, fmtUptime, post } from "../lib/api";
import { useViewData } from "../state";
import { ViewHead } from "./Simple";

export function Stat({ k, v, d }: { k: string; v: React.ReactNode; d?: React.ReactNode }) {
  return (
    <div className="stat">
      <div className="k">{k}</div>
      <div className="v">{v}</div>
      {d ? <div className="d">{d}</div> : null}
    </div>
  );
}

/** Aggiornamenti (ADR-0034): spenti per impostazione, canali Stable e Beta, controllo manuale. */
function UpdatesPanel({ active }: { active: boolean }) {
  const { data, reload, setData } = useViewData(active, () => api("/api/updates"));
  const [busy, setBusy] = useState(false);
  if (!data) return null;
  const save = async (enabled: boolean, channel: string) => {
    setBusy(true);
    try {
      setData(await post("/api/updates", { enabled, channel }));
    } catch (e) {
      alert((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const check = async () => {
    setBusy(true);
    try {
      setData(await post("/api/updates/check"));
    } catch (e) {
      alert((e as Error).message);
    } finally {
      setBusy(false);
      reload();
    }
  };
  return (
    <div className="backup-form" id="updates">
      <p className="hint">
        Versione installata: <b>v{data.current}</b>. Il controllo contatta GitHub una volta al giorno solo se lo attivi e non invia dati
        tuoi; gli aggiornamenti non si installano da soli.
      </p>
      <div className="row">
        <label className="check">
          <input type="checkbox" name="updates-enabled" disabled={busy} checked={data.enabled} onChange={(e) => save(e.target.checked, data.channel)} /> Controlla gli
          aggiornamenti ogni giorno
        </label>
        <label>Canale
          <select name="updates-channel" disabled={busy} value={data.channel} onChange={(e) => save(data.enabled, e.target.value)}>
            <option value="stable">Stable</option>
            <option value="beta">Beta (anche le versioni di prova)</option>
          </select>
        </label>
        <button className="ghost" id="updates-check" type="button" disabled={busy} onClick={check}>{busy ? "Controllo…" : "Controlla ora"}</button>
      </div>
      <p className="sub" id="updates-status">
        {data.error ? `Ultimo controllo non riuscito: ${data.error}` :
          data.update_available && data.latest ? (
            <>Disponibile <b>{data.latest.tag}</b>: <a href={data.latest.url} target="_blank" rel="noopener noreferrer">pagina della release</a></>
          ) : data.checked_at ? `Nessun aggiornamento (ultimo controllo ${fmtTime(data.checked_at)}).` : "Mai controllato."}
      </p>
    </div>
  );
}

/** Database locale (ADR-0037): integrità rapida, versione dello schema, dimensione e righe principali. */
function DatabasePanel({ active }: { active: boolean }) {
  const { data } = useViewData(active, () => api<{
    ok: boolean; check: string; schema_version: number; size_bytes: number; rows: Record<string, number>;
  }>("/api/database"));
  if (!data) return null;
  const mb = (data.size_bytes / 1048576).toFixed(data.size_bytes < 10485760 ? 2 : 1);
  const r = data.rows;
  return (
    <div className="grid" id="database">
      <Stat k="Integrità" v={data.ok ? "verificata ✓" : "da controllare ✗"} d={data.ok ? "SQLite quick_check" : data.check} />
      <Stat k="Dimensione" v={`${mb} MB`} d={`schema v${data.schema_version} · manutenzione ogni 6 ore`} />
      <Stat k="Contenuti" v={`${r.memory} ricordi · ${r.messages} messaggi`} d={`${r.tasks} attività · ${r.workflows} automazioni · ${r.notifications} notifiche`} />
    </div>
  );
}

export function System({ active, onSetup }: { active: boolean; onSetup: () => void }) {
  const { data, reload } = useViewData(active, async () => {
    const [h, m, bk, hist] = await Promise.all([
      fetch("/api/health", { headers: AUTH }).then((r) => r.json()),
      api("/api/metrics"),
      api("/api/backups"),
      api("/api/metrics/history?hours=24"),
    ]);
    return { h, m, bk, hist };
  });
  const [pass, setPass] = useState("");
  const [again, setAgain] = useState("");
  const [busy, setBusy] = useState(false);
  const [verifyFile, setVerifyFile] = useState("");
  const [verifyPass, setVerifyPass] = useState("");
  const [verifyBusy, setVerifyBusy] = useState(false);
  const [verification, setVerification] = useState<{ text: string; failed: boolean } | null>(null);

  async function verifyBackup(e: React.FormEvent) {
    e.preventDefault();
    setVerifyBusy(true);
    setVerification(null);
    try {
      const result = await post<{ files: number; created_at: string; audit_chain_valid: boolean }>("/api/backups/verify", {
        file: verifyFile, passphrase: verifyPass,
      });
      setVerification({
        text: `Archivio e database verificati · ${result.files} file · ${fmtTime(result.created_at)} · audit ${result.audit_chain_valid ? "integro" : "da controllare"}`,
        failed: false,
      });
    } catch (e) {
      setVerification({ text: (e as Error).message, failed: true });
    } finally {
      setVerifyPass("");
      setVerifyBusy(false);
    }
  }

  const backup = async (e: React.FormEvent) => {
    e.preventDefault();
    if (pass !== again) return alert("Le due passphrase non coincidono.");
    setBusy(true);
    try {
      const out = await post("/api/backup", { passphrase: pass });
      setPass("");
      setAgain("");
      alert(`Backup creato: ${out.file} (${out.files} file, audit ${out.audit.chain_valid ? "integro" : "da verificare"}).`);
      reload();
    } catch (err) {
      alert((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const h = data?.h, m = data?.m;
  const b = h?.ai?.gateway?.backend;
  const v = h?.ai?.gateway?.voice;
  const c = m?.counters || {};
  const rows: [string, any][] = [];
  for (const [group, series] of Object.entries<any>(m?.series || {})) for (const [name, x] of Object.entries<any>(series)) rows.push([`${group} · ${name}`, x]);
  const total = (s: any, k: string) => Object.values<any>(s?.http || {}).reduce((a, x) => a + (x[k] || 0), 0);
  const history: any[] = data?.hist.items || [];

  return (
    <section className={`view${active ? " active" : ""}`} id="view-system" aria-label="Sistema">
      <ViewHead title="Sistema" refreshKey="system" onRefresh={reload} />
      <p className="hint">
        Stato dei componenti e latenze misurate dall'avvio. I log strutturati sono in <code>data/logs/core.jsonl</code> e non contengono i tuoi
        messaggi.
      </p>
      <div id="system-cards" className="grid">
        {h && m ? (
          <>
            <Stat k="Core" v={`v${h.core.version} · attivo da ${fmtUptime(m.uptime_s)}`} d={[h.core.secrets_store ? `chiave dei segreti: ${h.core.secrets_store === "keychain" ? "portachiavi" : "file"}` : "", `memoria: ${h.core.memory_embeddings ? "sinonimi con " + h.core.memory_embeddings : "ricerca locale"}`].filter(Boolean).join(" · ")} />
            <Stat k="AI" v={b ? `${b.name} · ${b.model}` : "non disponibile"} d={b && b.degraded_reason ? b.degraded_reason : h.ai.status === "up" ? "worker attivo" : (h.ai.error || {}).message} />
            <Stat k="Voce" v={v && v.available ? `${v.stt.name} · ${v.tts ? v.tts.name : "senza sintesi"}` : "non disponibile"} d={v ? (v.available ? `attivazione: ${v.wake}` : (v.reasons || []).join("; ")) : ""} />
            <Stat k="Web" v={h.web && h.web.enabled ? `attivo · ${h.web.provider}` : "disattivato"} d={h.web ? h.web.provider_url : ""} />
            <Stat k="Audit" v={`${h.audit.events} eventi`} d={h.audit.chain_valid ? "catena integra ✓" : "catena ALTERATA ✗"} />
            <Stat k="Token del modello" v={`${c["ai.tokens.prompt"] || 0} in ingresso · ${c["ai.tokens.output"] || 0} generati`} d={`budget del contesto (ADR-0024) · ${c["ai.tools.sent"] || 0} descrizioni di strumenti inviate`} />
            <Stat k="Worker" v={`${c["worker.spawned"] || 0} avvii · ${c["worker.crashed"] || 0} arresti anomali`} d={`${c["delegation.failed"] || 0} deleghe fallite`} />
          </>
        ) : null}
      </div>
      <div className="row"><button className="ghost" type="button" id="open-setup" onClick={onSetup}>Configura voce e modello AI</button></div>
      <CloudSettings active={active} />
      <h3 className="sub-head">Database locale</h3>
      <DatabasePanel active={active} />
      <h3 className="sub-head">Aggiornamenti</h3>
      <UpdatesPanel active={active} />
      <h3 className="sub-head">Backup cifrato</h3>
      <form id="backup-form" className="form backup-form" onSubmit={backup}>
        <p className="hint">
          Copia cifrata di conversazioni, memoria, automazioni, workspace e audit. Senza passphrase non si può aprire: conservala con cura.
          Ripristino (a JARVIS chiuso): <code>./jarvis.sh restore &lt;file.jbak&gt;</code>.
        </p>
        <div className="row">
          <label>Passphrase <input type="password" name="pass" minLength={12} required autoComplete="new-password" value={pass} onChange={(e) => setPass(e.target.value)} /></label>
          <label>Ripeti <input type="password" name="again" minLength={12} required autoComplete="new-password" value={again} onChange={(e) => setAgain(e.target.value)} /></label>
          <button type="submit" className="primary" disabled={busy}>{busy ? "Cifratura…" : "Crea backup"}</button>
        </div>
      </form>
      <div id="backup-list" className="list">
        {data ? (data.bk.items.length ? data.bk.items.slice(0, 5).map((x: { file: string; bytes: number }) => (
          <div className="row" key={x.file}>
            <span className="sub">{x.file} · {(x.bytes / 1024).toFixed(1)} KiB</span>
            <button type="button" className="ghost small" disabled={verifyBusy} onClick={() => {
              setVerifyFile(x.file);
              setVerifyPass("");
              setVerification(null);
            }}>Verifica backup</button>
          </div>
        )) : <div className="sub">Nessun backup in {data.bk.dir}</div>) : null}
      </div>
      {verifyFile ? (
        <form id="verify-backup" className="form backup-form" onSubmit={verifyBackup}>
          <p className="hint">Controlla passphrase, archivio, database e audit di <code>{verifyFile}</code> senza ripristinarlo.</p>
          <div className="row">
            <label>Passphrase del backup <input type="password" required autoComplete="off" value={verifyPass} onChange={(e) => setVerifyPass(e.target.value)} /></label>
            <button type="submit" className="primary" disabled={verifyBusy}>{verifyBusy ? "Verifica…" : "Verifica ora"}</button>
          </div>
          {verification ? <p role={verification.failed ? "alert" : "status"} className={verification.failed ? "form-error" : "hint"}>{verification.text}</p> : null}
        </form>
      ) : null}
      {data?.bk.rollback?.length ? (
        <div className="backup-form">
          <h4>Copie prima degli aggiornamenti</h4>
          <p className="hint">Per tornare alla versione precedente, esci da JARVIS ed esegui <code>./jarvis.sh rollback</code> (Windows <code>jarvis.cmd rollback</code>).</p>
          <ul>{data.bk.rollback.map((name: string) => <li key={name}><code>{name}</code></li>)}</ul>
        </div>
      ) : null}
      <h3 className="sub-head">Latenze</h3>
      <div className="table-wrap">
        <table id="metrics-table" className="metrics">
          <thead><tr><th scope="col">Serie</th><th scope="col">Richieste</th><th scope="col">Errori</th><th scope="col">p50</th><th scope="col">p95</th><th scope="col">max</th></tr></thead>
          <tbody>
            {data ? (rows.length ? rows.map(([name, x]) => (
              <tr key={name}>
                <td>{name}</td><td>{String(x.count)}</td><td className={x.errors ? "bad" : ""}>{String(x.errors)}</td>
                <td>{x.p50_ms} ms</td><td>{x.p95_ms} ms</td><td>{x.max_ms} ms</td>
              </tr>
            )) : <tr><td colSpan={6}>Nessuna misura ancora.</td></tr>) : null}
          </tbody>
        </table>
      </div>
      <h3 className="sub-head">Storico (ultime 24 ore)</h3>
      <p className="hint">Un'istantanea ogni 5 minuti e alla chiusura, conservata per 7 giorni. Contiene solo conteggi e latenze, mai i tuoi contenuti.</p>
      <div className="row export-row">
        <button className="ghost" id="export-json" type="button" onClick={() => download("/api/metrics/export", "jarvis-metrics.json")}>Esporta JSON</button>
        <button className="ghost" id="export-prom" type="button" onClick={() => download("/api/metrics/export?format=prometheus", "jarvis-metrics.prom")}>Esporta Prometheus</button>
      </div>
      <div className="table-wrap">
        <table id="history-table" className="metrics">
          <thead><tr><th scope="col">Ora</th><th scope="col">Attivo da</th><th scope="col">Richieste HTTP</th><th scope="col">Errori</th><th scope="col">p95 chat</th></tr></thead>
          <tbody>
            {data ? (history.length ? history.slice(-48).reverse().map((x) => (
              <tr key={x.at}>
                <td>{fmtTime(x.at)}</td><td>{fmtUptime(x.uptime_s || 0)}</td><td>{String(total(x.series, "count"))}</td>
                <td>{String(total(x.series, "errors"))}</td>
                <td>{x.series?.http?.["POST /api/chat"] ? `${x.series.http["POST /api/chat"].p95_ms} ms` : "—"}</td>
              </tr>
            )) : <tr><td colSpan={5}>Ancora nessuna istantanea: la prima viene salvata entro 5 minuti dall'avvio.</td></tr>) : null}
          </tbody>
        </table>
      </div>
    </section>
  );
}
