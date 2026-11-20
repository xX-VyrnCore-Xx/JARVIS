import { Icon } from "../components/Icon";
import { api } from "../lib/api";
import { useApp, useViewData } from "../state";
import { ViewHead } from "./Simple";
import { Stat } from "./System";

interface Posture {
  audit_chain_valid: boolean;
  audit_events: number;
  signatures: { enabled: boolean; valid: boolean };
  secrets_store: string;
  devices_active: number;
  pending_confirmations: number;
  web_enabled: boolean;
  web_private_addresses: boolean;
  shell_enabled: boolean;
}

export function Security({ active }: { active: boolean }) {
  const { show, notifTick, activity } = useApp();
  const { data, error, reload } = useViewData(active, () => api<Posture>("/api/security"), [notifTick, activity]);
  return (
    <section className={`view${active ? " active" : ""}`} id="view-security" aria-label="Centro sicurezza">
      <ViewHead title="Centro sicurezza" refreshKey="security" onRefresh={reload} />
      {error ? <p className="form-error" role="alert">{error}</p> : null}
      {data ? (
        <div className="grid">
          <Stat k="Registro di audit" v={data.audit_chain_valid ? "Catena integra" : "Catena da verificare"}
            d={`${data.audit_events} eventi · ${data.signatures.enabled ? data.signatures.valid ? "firme valide" : "firme da verificare" : "firme disattivate"}`} />
          <Stat k="Credenziali" v={data.secrets_store === "keychain" ? "Portachiavi di sistema" : "Archivio locale"}
            d="Le credenziali sono conservate cifrate dal Core." />
          <Stat k="Dispositivi associati" v={data.devices_active} d="L’accesso si può revocare in Dispositivi." />
          <Stat k="Approvazioni aperte" v={data.pending_confirmations} d="Controlla la destinazione prima di approvare." />
          <Stat k="Accesso Web" v={data.web_enabled ? "Attivo" : "Disattivato"}
            d={data.web_private_addresses ? "La configurazione consente indirizzi privati." : "Gli indirizzi privati sono bloccati."} />
          <Stat k="Terminale" v={data.shell_enabled ? "Attivo con conferma" : "Disattivato"} />
        </div>
      ) : null}
      <div className="row security-actions">
        <button className="ghost" onClick={() => show("audit")}><Icon name="audit" /> Apri registro</button>
        <button className="ghost" onClick={() => show("devices")}><Icon name="devices" /> Gestisci accessi</button>
        <button className="ghost" onClick={() => show("chat")}><Icon name="chat" /> Apri conferme</button>
      </div>
    </section>
  );
}
