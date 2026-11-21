import { useState } from "react";
import { api, post } from "../lib/api";
import { useApp, useViewData } from "../state";

interface Routine { id: string; name: string; kind: "audit" | "reminder"; minutes: number; last_run?: number }
interface Status { enabled: boolean; running: boolean; items: Routine[]; image: string; docker_connected?: boolean; container_created?: boolean; error?: string }

export function N8nPanel({ active }: { active: boolean }) {
  const { activity, notifTick } = useApp();
  const { data, setData, error, reload } = useViewData(active, () => api<Status>("/api/n8n"), [activity, notifTick]);
  const [name, setName] = useState("");
  const [kind, setKind] = useState("reminder");
  const [minutes, setMinutes] = useState(15);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  async function run(action: () => Promise<Status>) {
    setBusy(true);
    setFailure("");
    try { setData(await action()); }
    catch (err) { setFailure((err as Error).message); }
    finally { setBusy(false); }
  }

  return (
    <details className="panel n8n-panel">
      <summary>Container n8n · {data?.running ? "in esecuzione" : data?.enabled ? data.docker_connected === false ? "Docker non disponibile" : "fermo" : "da abilitare"}</summary>
      {error || failure || data?.error ? <p className="form-error" role="alert">{failure || error || data?.error}</p> : null}
      {!data?.enabled ? (
        <div>
          <p className="hint">Usa Docker locale per eseguire le routine di JARVIS in un container isolato.</p>
          <button className="ghost" type="button" disabled={busy || !data} onClick={() => void run(() => post("/api/n8n", { enable: true }))}>Abilita n8n</button>
        </div>
      ) : (
        <>
          <div className="row">
            {data.docker_connected === false ? <p className="hint">Avvia Docker sul computer che ospita il Core.</p> : null}
            <button className="ghost" type="button" disabled={busy} onClick={reload}>Ricontrolla Docker</button>
            <button className="ghost" type="button" disabled={busy || data.docker_connected === false} onClick={() => void run(() => post("/api/n8n", { running: !data.running }))}>
              {busy ? "Operazione in corso…" : data.running ? "Ferma n8n" : "Avvia n8n"}
            </button>
          </div>
          {data.running ? (
            <form className="form" onSubmit={(event) => {
              event.preventDefault();
              void run(async () => {
                const result = await post<Status>("/api/n8n/jobs", { name, kind, minutes });
                setName("");
                return result;
              });
            }}>
              <div className="row">
                <label>Nome della routine <input maxLength={80} required value={name} onChange={(e) => setName(e.target.value)} /></label>
                <label>Tipo
                  <select value={kind} onChange={(e) => setKind(e.target.value)}>
                    <option value="reminder">Promemoria</option><option value="audit">Verifica audit</option>
                  </select>
                </label>
                <label>Intervallo in minuti <input type="number" min={1} max={59} required value={minutes} onChange={(e) => setMinutes(e.target.valueAsNumber)} /></label>
                <button className="primary" type="submit" disabled={busy}>Attiva routine</button>
              </div>
            </form>
          ) : null}
          {data.items.map((routine) => (
            <div className="row" key={routine.id}>
              <span>{routine.name} · ogni {routine.minutes} min · {routine.last_run ? "eseguita" : "prima esecuzione in attesa"}</span>
              <button className="ghost small" type="button" disabled={busy || !data.running}
                onClick={() => void run(() => api(`/api/n8n/jobs/${encodeURIComponent(routine.id)}`, { method: "DELETE" }))}>Disattiva</button>
            </div>
          ))}
        </>
      )}
    </details>
  );
}
