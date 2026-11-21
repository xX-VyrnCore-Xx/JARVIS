import { useEffect, useState } from "react";
import { api, fmtTime, guard, post } from "../lib/api";
import { useApp, useViewData } from "../state";
import { Empty, ViewHead } from "./Simple";

export interface MemoryItem {
  id: string;
  text: string;
  kind: "semantic" | "procedural" | "episodic";
  created_at: string;
  expires_at?: string | null;
  score?: number;
}
interface Graph {
  nodes: { id: string; label: string }[];
  edges: { memory_id: string; from: string; relation: string; to: string }[];
}

const KINDS: [string, string][] = [["", "Tutto"], ["semantic", "Fatti"], ["procedural", "Preferenze"], ["episodic", "Episodi"]];
const LABELS: Record<string, string> = { semantic: "fatto", procedural: "preferenza", episodic: "episodio" };

/** Grafo radiale: il proprietario al centro, le entità collegate attorno. */
export function MemoryGraph({ graph, compact = false }: { graph: Graph; compact?: boolean }) {
  const { nodes, edges } = graph;
  if (!edges.length) return <p className="empty">Nessuna relazione ancora. Prova: «ricorda che mia sorella si chiama Anna».</p>;
  const W = 640, H = compact ? 360 : 420, cx = W / 2, cy = H / 2;
  const center = nodes.find((n) => n.id === "io") || nodes[0];
  const others = nodes.filter((n) => n !== center);
  const pos: Record<string, [number, number]> = { [center.id]: [cx, cy] };
  others.forEach((n, i) => {
    const a = (2 * Math.PI * i) / others.length - Math.PI / 2;
    const r = (others.length > 10 && i % 2 ? 175 : 140) * (compact ? 0.85 : 1);
    pos[n.id] = [cx + r * Math.cos(a), cy + r * 0.82 * Math.sin(a)];
  });
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="graph" role="img" aria-label={`Grafo con ${nodes.length} entità e ${edges.length} relazioni`}>
      <circle cx={cx} cy={cy} r={140} className="orbit" />
      <circle cx={cx} cy={cy} r={175} className="orbit" />
      {edges.map((e, i) => {
        const [x1, y1] = pos[e.from], [x2, y2] = pos[e.to];
        return (
          <g key={i}>
            <line x1={x1} y1={y1} x2={x2} y2={y2} className="edge" />
            <text x={(x1 + x2) / 2} y={(y1 + y2) / 2 - 4} className="edge-label" textAnchor="middle">{e.relation}</text>
          </g>
        );
      })}
      {nodes.map((n) => {
        const [x, y] = pos[n.id];
        const owner = n === center;
        return (
          <g key={n.id}>
            <circle cx={x} cy={y} r={owner ? 22 : 15} className={owner ? "node owner" : "node"} />
            <text x={x} y={y + (owner ? 38 : 30)} className="node-label" textAnchor="middle">{n.label}</text>
          </g>
        );
      })}
    </svg>
  );
}

export function Memory({ active }: { active: boolean }) {
  const { toast, activity } = useApp();
  const [kind, setKind] = useState("");
  const [q, setQ] = useState("");
  const [debounced, setDebounced] = useState("");
  const [graphOpen, setGraphOpen] = useState(false);
  const [graph, setGraph] = useState<Graph | null>(null);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(q.trim()), 250);
    return () => clearTimeout(t);
  }, [q]);

  const { data, reload } = useViewData(
    active,
    () => {
      const params = new URLSearchParams();
      if (kind) params.set("kind", kind);
      if (debounced) params.set("q", debounced);
      return api<{ items: MemoryItem[] }>(`/api/memory${params.toString() ? "?" + params : ""}`);
    },
    [kind, debounced, activity],
    6000,
  );

  useEffect(() => {
    if (active && graphOpen) api<Graph>("/api/memory/graph").then(setGraph).catch(() => {});
  }, [active, graphOpen, data]);

  const items = data?.items || [];
  return (
    <section className={`view${active ? " active" : ""}`} id="view-memory" aria-label="Memoria">
      <ViewHead title="Memoria" refreshKey="memory" onRefresh={reload} />
      <p className="hint">
        Ciò che JARVIS ricorda di te: fatti, preferenze (sempre nel contesto) ed episodi delle attività, che scadono dopo 30 giorni. Puoi
        correggere o cancellare ogni elemento; l'operazione viene registrata nell'audit.
      </p>
      <div className="memory-tools">
        <div className="mem-kinds" role="tablist" aria-label="Tipo di memoria">
          {KINDS.map(([k, label]) => (
            <button key={k} className={`chip${kind === k ? " active" : ""}`} data-kind={k} role="tab" aria-selected={kind === k} onClick={() => setKind(k)}>
              {label}
            </button>
          ))}
        </div>
        <input id="memory-search" type="search" placeholder="Cerca nella memoria…" aria-label="Cerca nella memoria" value={q}
          onChange={(e) => setQ(e.target.value)} />
        <button className="ghost" id="memory-reindex" title="Ricostruisce indice di ricerca e grafo dai dati" onClick={guard(async () => {
          const { reindexed } = await post<{ reindexed: number }>("/api/memory/reindex");
          reload();
          toast({ notification_id: `local-reindex-${Date.now()}`, severity: "info", category: "system", title: "Memoria",
            body: `Indici ricostruiti per ${reindexed} elementi`, created_at: new Date().toISOString() });
        })}>
          Ricostruisci indici
        </button>
      </div>
      <div id="memory" className="list">
        {data && !items.length ? (
          <Empty>{debounced ? `Nessun ricordo pertinente a «${debounced}».` : 'La memoria è vuota. Prova: "ricorda che …" oppure "preferisco …"'}</Empty>
        ) : null}
        {items.map((m) => {
          const meta = [m.id, fmtTime(m.created_at)];
          if (m.expires_at) meta.push(`scade ${fmtTime(m.expires_at)}`);
          if (m.score != null) meta.push(`pertinenza ${Math.round(m.score * 100)}%`);
          return (
            <div className="card" data-kind={m.kind || "semantic"} key={m.id}>
              <div className="body">
                <span className={`badge mem-${m.kind || "semantic"}`}>{LABELS[m.kind] || "fatto"}</span>{" "}
                <div className="title">{m.text}</div>
                <div className="sub">{meta.join(" · ")}</div>
              </div>
              {m.kind === "episodic" ? null : (
                <button className="ghost" onClick={guard(async () => {
                  const text = prompt("Correggi l'elemento di memoria:", m.text);
                  if (text == null || !text.trim() || text.trim() === m.text) return;
                  await api(`/api/memory/${encodeURIComponent(m.id)}`, { method: "PATCH", body: JSON.stringify({ text: text.trim() }) });
                  reload();
                })}>
                  Correggi
                </button>
              )}
              <button className="danger" onClick={guard(async () => {
                if (!confirm(`Cancellare definitivamente «${m.text}»?`)) return;
                await api(`/api/memory/${encodeURIComponent(m.id)}`, { method: "DELETE" });
                reload();
              })}>
                Cancella
              </button>
            </div>
          );
        })}
      </div>
      <details className="memory-graph-box" open={graphOpen} onToggle={(e) => setGraphOpen((e.target as HTMLDetailsElement).open)}>
        <summary>Grafo delle conoscenze</summary>
        <div id="memory-graph">{graphOpen && graph ? <MemoryGraph graph={graph} /> : null}</div>
      </details>
    </section>
  );
}
