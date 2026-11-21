import { useState } from "react";
import { api, post } from "../lib/api";
import { useViewData } from "../state";

export interface CloudProvider {
  id: "ollama_cloud" | "nvidia_nim";
  routing: { fast: string; reasoning: string; coding: string; long?: string };
  configured: boolean;
  effective_rpm: number | null;
  cooldown_seconds: number;
}
export const CLOUD_LABELS = { ollama_cloud: "Ollama Cloud", nvidia_nim: "NVIDIA NIM" };

function ProviderSettings({ provider, onChange }: { provider: CloudProvider; onChange: (value: { providers: CloudProvider[] }) => void }) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [failed, setFailed] = useState(false);

  async function save(remove = false) {
    setBusy(true);
    setMessage("");
    try {
      onChange(await post("/api/cloud", { provider: provider.id, key: key.trim() || undefined, remove_key: remove }));
      setKey("");
      setFailed(false);
      setMessage(remove ? "Chiave rimossa." : "Impostazioni salvate.");
    } catch (error) {
      setFailed(true);
      setMessage((error as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="form backup-form cloud-settings" onSubmit={(e) => { e.preventDefault(); void save(); }}>
      <h4>{CLOUD_LABELS[provider.id]} · {provider.configured ? "chiave configurata" : "da configurare"}</h4>
      <div className="row">
        <label>Chiave API
          <input type="password" required={!provider.configured} autoComplete="off" value={key} onChange={(e) => setKey(e.target.value)} />
        </label>
        <button className="primary" type="submit" disabled={busy}>Salva</button>
        {provider.configured ? <button className="ghost" type="button" disabled={busy} onClick={() => void save(true)}>Rimuovi chiave</button> : null}
      </div>
      {provider.routing ? <details>
        <summary>Modelli scelti in automatico</summary>
        <ul>
          <li>Risposte rapide · {provider.routing.fast}</li>
          <li>Analisi e contesto lungo · {provider.routing.reasoning}</li>
          <li>Sviluppo · {provider.routing.coding}</li>
          {provider.routing.long ? <li>Contesto lungo · {provider.routing.long}</li> : null}
        </ul>
      </details> : null}
      {provider.effective_rpm ? <small className="sub">Massimo {provider.effective_rpm} richieste al minuto da questo Core.</small> : null}
      {provider.cooldown_seconds > 0 ? <small className="sub">In pausa dopo un limite del provider. Attesa indicata al controllo: {provider.cooldown_seconds} s.</small> : null}
      {message ? <p role={failed ? "alert" : "status"} className={failed ? "form-error" : "hint"}>{message}</p> : null}
    </form>
  );
}

export function CloudSettings({ active }: { active: boolean }) {
  const { data, error, setData } = useViewData(active, () => api<{ providers: CloudProvider[] }>("/api/cloud"));
  return (
    <>
      <h3 className="sub-head">Modelli cloud</h3>
      <p className="hint">Scegli il provider in chat. JARVIS seleziona il modello in base all’attività e alla lunghezza del contesto. Riceve il messaggio corrente e gli schemi degli strumenti disponibili. In Chat puoi condividere fino a otto scambi testuali con lo stesso provider. Memoria e risultati degli strumenti restano sul dispositivo.</p>
      {error ? <p className="form-error" role="alert">{error}</p> : null}
      {data?.providers.map((provider) => <ProviderSettings key={provider.id} provider={provider} onChange={setData} />)}
    </>
  );
}
