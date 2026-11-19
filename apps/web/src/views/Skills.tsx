import { useState } from "react";
import { api, guard, post } from "../lib/api";
import { useViewData } from "../state";
import { Empty, ViewHead } from "./Simple";

export interface Skill {
  id: string;
  name: string;
  description: string;
  triggers: string[];
  instructions: string;
  tools: string[];
  enabled: boolean;
  builtin: boolean;
}

interface Draft {
  id?: string;
  name: string;
  description: string;
  triggers: string;
  instructions: string;
  tools: string;
  enabled: boolean;
}

const EMPTY: Draft = { name: "", description: "", triggers: "", instructions: "", tools: "", enabled: true };
const split = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);
const body = (d: Draft) => ({
  name: d.name, description: d.description, instructions: d.instructions,
  triggers: split(d.triggers), tools: split(d.tools), enabled: d.enabled,
});

/** Competenze (ADR-0048): procedure riutilizzabili attivate da parole chiave o dal nome. */
export function Skills({ active }: { active: boolean }) {
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState("");
  const { data, reload } = useViewData(active, () => api<{ items: Skill[]; max_active: number }>("/api/skills"), []);
  const items = data?.items || [];
  const edit = (s: Skill) =>
    setDraft({ id: s.id, name: s.name, description: s.description, triggers: s.triggers.join(", "),
               instructions: s.instructions, tools: s.tools.join(", "), enabled: s.enabled });
  const save = async () => {
    if (!draft) return;
    try {
      if (draft.id) await api(`/api/skills/${encodeURIComponent(draft.id)}`, { method: "PUT", body: JSON.stringify(body(draft)) });
      else await post("/api/skills", body(draft));
      setDraft(null);
      setError("");
      reload();
    } catch (e) {
      setError((e as Error).message);
    }
  };
  const set = (k: keyof Draft) => (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
    setDraft((d) => d && { ...d, [k]: k === "enabled" ? (e.target as HTMLInputElement).checked : e.target.value });

  return (
    <section className={`view${active ? " active" : ""}`} id="view-skills" aria-label="Competenze">
      <ViewHead title="Competenze" refreshKey="skills" onRefresh={reload} />
      <p className="hint">
        Procedure che JARVIS segue quando una richiesta contiene una parola di attivazione o il nome della competenza («usa Spese: …»).
        Al massimo {data?.max_active ?? 2} per richiesta. Gli strumenti suggeriti vengono offerti per primi, ma policy, conferme e audit
        restano quelli di sempre.
      </p>
      <div className="memory-tools">
        <button id="skill-new" onClick={() => { setDraft({ ...EMPTY }); setError(""); }}>Nuova competenza</button>
      </div>
      {draft ? (
        <form className="card skill-form" id="skill-form" onSubmit={(e) => { e.preventDefault(); save(); }}>
          <label>Nome<input name="name" required maxLength={80} value={draft.name} onChange={set("name")} /></label>
          <label>Descrizione<input name="description" maxLength={300} value={draft.description} onChange={set("description")} /></label>
          <label>Parole di attivazione (separate da virgole)<input name="triggers" value={draft.triggers} onChange={set("triggers")}
            placeholder="itinerario, viaggio a" /></label>
          <label>Istruzioni<textarea name="instructions" required maxLength={4000} rows={6} value={draft.instructions} onChange={set("instructions")}
            placeholder="Come deve procedere JARVIS quando la competenza è attiva" /></label>
          <label>Strumenti suggeriti (nomi del catalogo, separati da virgole)<input name="tools" value={draft.tools} onChange={set("tools")}
            placeholder="weather.forecast, web.search" /></label>
          <label className="check"><input type="checkbox" name="enabled" checked={draft.enabled} onChange={set("enabled")} /> Attiva</label>
          {error ? <p className="error" role="alert">{error}</p> : null}
          <div className="row">
            <button type="submit">Salva</button>
            <button type="button" className="ghost" onClick={() => setDraft(null)}>Annulla</button>
          </div>
        </form>
      ) : null}
      <div id="skills" className="list">
        {data && !items.length ? <Empty>Nessuna competenza.</Empty> : null}
        {items.map((s) => (
          <div className={`card skill${s.enabled ? "" : " off"}`} data-skill={s.id} key={s.id}>
            <div className="body">
              <span className={`badge ${s.enabled ? "ok" : ""}`}>{s.enabled ? "attiva" : "disattivata"}</span>{" "}
              {s.builtin ? <span className="badge">inclusa</span> : null}
              <div className="title">{s.name}</div>
              <div className="desc">{s.description}</div>
              <div className="sub">
                {s.triggers.length ? `attivazione: ${s.triggers.join(", ")}` : "solo per nome"}
                {s.tools.length ? ` · strumenti: ${s.tools.join(", ")}` : ""}
              </div>
            </div>
            <button className="ghost" data-action="toggle" onClick={guard(async () => {
              await api(`/api/skills/${encodeURIComponent(s.id)}`, { method: "PUT", body: JSON.stringify({ ...s, enabled: !s.enabled }) });
              reload();
            })}>
              {s.enabled ? "Disattiva" : "Attiva"}
            </button>
            <button className="ghost" data-action="edit" onClick={() => edit(s)}>Modifica</button>
            {s.builtin ? null : (
              <button className="danger" data-action="delete" onClick={guard(async () => {
                if (!confirm(`Eliminare la competenza «${s.name}»?`)) return;
                await api(`/api/skills/${encodeURIComponent(s.id)}`, { method: "DELETE" });
                reload();
              })}>
                Elimina
              </button>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}
