import { N8nPanel } from "../components/N8nPanel";
import { useEffect, useRef, useState } from "react";
import { api, fmtTime, post } from "../lib/api";
import { useApp, useViewData } from "../state";
import { Empty, ViewHead } from "./Simple";

interface Tool {
  name: string;
  summary: string;
  risk: string;
  workflow_step?: boolean;
  input_schema?: { properties?: Record<string, unknown>; required?: string[] };
}
interface StepDraft {
  key: number;
  tool: string;
  args: Record<string, string>;
}
interface Workflow {
  workflow_id: string;
  id: string;
  name: string;
  version: number;
  enabled: boolean;
  trigger: { kind: string; at?: string; every_seconds?: number };
  trigger_text: string;
  steps: { tool: string; arguments: Record<string, unknown> }[];
  on_failure: string;
  next_run?: string;
  last_run?: { status: string; started_at: string };
}

let toolCache: Tool[] | null = null;
const loadTools = async () => (toolCache ??= (await api<{ items: Tool[] }>("/api/tools")).items.filter((t) => t.workflow_step));

function StepEditor({ step, tools, onChange, onRemove }: {
  step: StepDraft; tools: Tool[]; onChange: (s: StepDraft) => void; onRemove: () => void;
}) {
  const tool = tools.find((t) => t.name === step.tool) || tools[0];
  const props = Object.keys(tool?.input_schema?.properties || {});
  const required = tool?.input_schema?.required || [];
  return (
    <div className="step-box">
      <div className="head">
        <select aria-label="Strumento" value={step.tool} onChange={(e) => onChange({ ...step, tool: e.target.value, args: {} })}>
          {tools.map((t) => <option key={t.name} value={t.name}>{t.name} — {t.summary}</option>)}
        </select>
        <button type="button" className="ghost danger" aria-label="Rimuovi passo" onClick={onRemove}>✕</button>
      </div>
      <div className="form">
        {tool && (tool.risk === "high" || tool.risk === "critical") ? (
          <span className="risk-high">Rischio alto: approvandolo ora, verrà eseguito in background senza ulteriori conferme.</span>
        ) : null}
        {props.map((k) => {
          const common = {
            "data-arg": k,
            value: step.args[k] ?? "",
            onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => onChange({ ...step, args: { ...step.args, [k]: e.target.value } }),
          };
          return (
            <label key={k}>
              {k}{required.includes(k) ? " *" : ""}
              {k === "content" ? <textarea rows={3} {...common} /> : <input required={required.includes(k)} {...common} />}
            </label>
          );
        })}
      </div>
    </div>
  );
}

function Runs({ w }: { w: Workflow }) {
  const [runs, setRuns] = useState<any[] | null>(null);
  useEffect(() => {
    api(`/api/workflows/${w.workflow_id}/runs`).then((d) => setRuns(d.items)).catch((e) => alert(e.message));
  }, [w.workflow_id]);
  if (!runs) return null;
  return (
    <>
      {runs.length ? runs.map((r) => (
        <div className="sub" key={r.run_id}>
          {`${fmtTime(r.started_at)} · v${r.workflow_version} · ${r.trigger.kind} · ${r.status} · ` +
            r.steps.map((s: any) => `${s.tool}:${s.status}${s.preapproved ? " (pre-approvato)" : ""}${s.error_code ? " " + s.error_code : ""}`).join(", ")}
        </div>
      )) : <div className="sub">Nessuna esecuzione.</div>}
    </>
  );
}

export function Automations({ active }: { active: boolean }) {
  const { refreshStatus, activity } = useApp();
  const { data, reload } = useViewData(active, () => api<{ items: Workflow[] }>("/api/workflows"), [activity]);
  const [tools, setTools] = useState<Tool[]>([]);
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<Workflow | null>(null);
  const [name, setName] = useState("");
  const [kind, setKind] = useState("interval");
  const [every, setEvery] = useState("60");
  const [at, setAt] = useState("08:00");
  const [cont, setCont] = useState(false);
  const [steps, setSteps] = useState<StepDraft[]>([]);
  const [showRuns, setShowRuns] = useState<Record<string, boolean>>({});
  const nextKey = useRef(1);
  const details = useRef<HTMLDetailsElement>(null);

  const newStep = (tool?: string, args: Record<string, string> = {}): StepDraft => ({ key: nextKey.current++, tool: tool || tools[0]?.name || "", args });

  useEffect(() => {
    if (active && !tools.length) loadTools().then(setTools).catch(() => {});
  }, [active, tools.length]);

  useEffect(() => {
    if (open && !steps.length && tools.length && !editing) setSteps([newStep()]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, tools.length]);

  const reset = () => {
    setEditing(null);
    setName("");
    setKind("interval");
    setEvery("60");
    setAt("08:00");
    setCont(false);
    setSteps([]);
    setOpen(false);
  };

  const edit = async (w: Workflow) => {
    const all = tools.length ? tools : await loadTools();
    setTools(all);
    setEditing(w);
    setOpen(true);
    setName(w.name);
    setKind(w.trigger.kind);
    if (w.trigger.kind === "daily") setAt(w.trigger.at || "08:00");
    if (w.trigger.kind === "interval") setEvery(String(Math.round((w.trigger.every_seconds || 3600) / 60)));
    if (w.trigger.kind === "once") setEvery(String(Math.max(1, Math.round((new Date(w.trigger.at!).getTime() - Date.now()) / 60000))));
    setCont(w.on_failure === "continue");
    setSteps(w.steps.map((s) => ({
      key: nextKey.current++, tool: s.tool,
      args: Object.fromEntries(Object.entries(s.arguments || {}).map(([k, v]) => [k, String(v)])),
    })));
    details.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    let trigger: any =
      kind === "daily" ? { kind, at }
        : kind === "once" ? { kind, at: new Date(Date.now() + Number(every) * 60000).toISOString().replace(/\.\d{3}Z$/, "Z") }
          : { kind, every_seconds: Number(every) * 60 };
    const body = {
      name,
      trigger,
      steps: steps.map((s) => ({ tool: s.tool, arguments: Object.fromEntries(Object.entries(s.args).filter(([, v]) => v !== "")) })),
      on_failure: cont ? "continue" : "stop",
    };
    if (!body.steps.length) return alert("Aggiungi almeno un passo.");
    const list = body.steps.map((s, i) => `${i + 1}. ${s.tool} ${JSON.stringify(s.arguments)}`).join("\n");
    const what = editing ? `la nuova versione (${editing.version + 1}) di «${name}»` : `l'automazione «${name}»`;
    if (!confirm(`Approvi ${what}?\n\n${list}\n\nI passi verranno eseguiti in background con la policy vigente.`)) return;
    try {
      if (editing) {
        if (editing.trigger.kind === "once" && kind === "once" && Math.abs(new Date(trigger.at).getTime() - new Date(editing.trigger.at!).getTime()) < 60000)
          body.trigger = editing.trigger;
        await api(`/api/workflows/${editing.workflow_id}`, { method: "PUT", body: JSON.stringify(body) });
      } else {
        await post("/api/workflows", body);
      }
      reset();
      reload();
    } catch (err) {
      alert((err as Error).message);
    }
  };

  const act = (fn: () => Promise<unknown>) => async () => {
    try {
      await fn();
      reload();
      refreshStatus();
    } catch (e) {
      if ((e as Error).message !== "Annullato") alert((e as Error).message);
    }
  };

  const items = data?.items || [];
  return (
    <section className={`view${active ? " active" : ""}`} id="view-automations" aria-label="Automazioni">
      <ViewHead title="Automazioni" refreshKey="automations" onRefresh={reload} />
      <p className="hint">
        Attività pianificate che sopravvivono al riavvio. A ogni esecuzione la policy viene ricontrollata; i passi a rischio alto partono
        solo perché li hai approvati alla creazione.
      </p>
      <N8nPanel active={active} />
      <details className="panel" id="wf-new" ref={details} open={open} onToggle={(e) => setOpen((e.target as HTMLDetailsElement).open)}>
        <summary>{editing ? `Modifica «${editing.name}» (versione ${editing.version} → ${editing.version + 1})` : "Nuova automazione"}</summary>
        <form id="wf-form" className="form" onSubmit={submit}>
          <label>Nome <input name="name" required maxLength={200} placeholder="Es. Battito orario" value={name} onChange={(e) => setName(e.target.value)} /></label>
          <div className="row">
            <label>
              Quando
              <select name="kind" value={kind} onChange={(e) => setKind(e.target.value)}>
                <option value="interval">Ogni N minuti</option>
                <option value="daily">Ogni giorno all'ora</option>
                <option value="once">Una volta, tra N minuti</option>
              </select>
            </label>
            <label id="wf-every" hidden={kind === "daily"}>Minuti <input name="every" type="number" min={1} value={every} onChange={(e) => setEvery(e.target.value)} /></label>
            <label id="wf-at" hidden={kind !== "daily"}>Ora locale <input name="at" type="time" value={at} onChange={(e) => setAt(e.target.value)} /></label>
          </div>
          <div id="wf-steps">
            {steps.map((s, i) => (
              <StepEditor key={s.key} step={s} tools={tools}
                onChange={(n) => setSteps((list) => list.map((x, j) => (j === i ? n : x)))}
                onRemove={() => setSteps((list) => list.filter((_, j) => j !== i))} />
            ))}
          </div>
          <div className="row">
            <button type="button" className="ghost" id="wf-add-step" onClick={() => setSteps((l) => [...l, newStep()])}>+ Aggiungi passo</button>
            <label className="check"><input type="checkbox" name="continue" checked={cont} onChange={(e) => setCont(e.target.checked)} /> Continua anche se un passo fallisce</label>
          </div>
          <button type="submit" className="primary">{editing ? "Salva nuova versione" : "Crea automazione"}</button>
        </form>
      </details>
      <div id="automations" className="list">
        {data && !items.length ? <Empty>Nessuna automazione. Creane una qui sopra o scrivi in chat: "ogni ora info sistema".</Empty> : null}
        {items.map((w) => (
          <div className="card" key={w.workflow_id}>
            <div className="body">
              <div className="title">{w.name} <span className="sub">v{w.version}</span></div>
              <div className="sub">[{w.id}] {w.trigger_text} · {w.steps.map((s) => s.tool).join(" → ")}</div>
              <div className="sub">prossima: {w.next_run || "—"} · ultima: {w.last_run ? `${w.last_run.status} (${fmtTime(w.last_run.started_at)})` : "mai"}</div>
              <div className="actions-row">
                <button className="ghost" onClick={act(() => post(`/api/workflows/${w.workflow_id}/run`))}>Esegui ora</button>
                <button className="ghost" onClick={act(() => post(`/api/workflows/${w.workflow_id}/enabled`, { enabled: !w.enabled }))}>
                  {w.enabled ? "Sospendi" : "Riattiva"}
                </button>
                <button className="ghost" onClick={() => edit(w).catch((e) => alert(e.message))}>Modifica</button>
                <button className="ghost" onClick={() => setShowRuns((s) => ({ ...s, [w.workflow_id]: !s[w.workflow_id] }))}>Cronologia</button>
                <button className="ghost danger" onClick={act(async () => {
                  if (!confirm(`Eliminare «${w.name}»?`)) throw new Error("Annullato");
                  await api(`/api/workflows/${w.workflow_id}`, { method: "DELETE" });
                })}>
                  Elimina
                </button>
              </div>
              <div className="runs">{showRuns[w.workflow_id] ? <Runs key={JSON.stringify(w.last_run)} w={w} /> : null}</div>
            </div>
            <span className={`badge ${w.enabled ? (w.last_run ? w.last_run.status : "allowed") : "cancelled"}`}>{w.enabled ? "attiva" : "sospesa"}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
