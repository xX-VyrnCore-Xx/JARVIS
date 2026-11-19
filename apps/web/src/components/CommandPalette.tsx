// Palette comandi (⌘K su macOS, Ctrl+K su Windows e Linux): vai a una sezione o invia un comando a JARVIS.

import { useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../state";
import type { View } from "../state";
import { Icon } from "./Icon";

export const IS_MAC = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
export const SHORTCUT = IS_MAC ? "⌘K" : "Ctrl K";

interface Entry {
  id: string;
  label: string;
  hint: string;
  icon: string;
  run: () => void;
}

export function CommandPalette({ views }: { views: { view: View; label: string; icon: string }[] }) {
  const { show, sendRef, thinking } = useApp();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      } else if (e.key === "Escape") setOpen(false);
    };
    const toggle = () => setOpen(true);
    document.addEventListener("keydown", key);
    window.addEventListener("jarvis:palette", toggle);
    return () => {
      document.removeEventListener("keydown", key);
      window.removeEventListener("jarvis:palette", toggle);
    };
  }, []);

  useEffect(() => {
    if (open) {
      setQ("");
      setSel(0);
      setTimeout(() => input.current?.focus(), 0);
    }
  }, [open]);

  const entries = useMemo<Entry[]>(() => {
    const term = q.trim().toLowerCase();
    const nav: Entry[] = views
      .filter((v) => !term || v.label.toLowerCase().includes(term))
      .map((v) => ({ id: `view-${v.view}`, label: v.label, hint: "Vai alla sezione", icon: v.icon, run: () => show(v.view) }));
    const quick: Entry[] = thinking ? [] : ["che ore sono", "info sistema", "cosa ricordi", "elenca automazioni"]
      .filter((c) => !term || c.includes(term))
      .map((c) => ({ id: `cmd-${c}`, label: c, hint: "Comando rapido", icon: "bolt", run: () => { show("chat"); sendRef.current(c); } }));
    const free: Entry[] = term && !thinking
      ? [{ id: "ask", label: q.trim(), hint: "Chiedi a JARVIS", icon: "chat", run: () => { show("chat"); sendRef.current(q.trim()); } }]
      : [];
    return [...nav, ...quick, ...free];
  }, [q, views, show, sendRef, thinking]);

  if (!open) return null;
  const choose = (e: Entry) => {
    setOpen(false);
    e.run();
  };
  return (
    <div className="palette-backdrop" onClick={() => setOpen(false)}>
      <div className="palette" role="dialog" aria-label="Palette comandi" onClick={(e) => e.stopPropagation()}>
        <div className="palette-input">
          <Icon name="search" />
          <input
            ref={input} autoFocus value={q} placeholder="Cerca una sezione o scrivi una richiesta…" aria-label="Comando"
            onChange={(e) => { setQ(e.target.value); setSel(0); }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") { e.preventDefault(); setSel((s) => Math.min(s + 1, entries.length - 1)); }
              if (e.key === "ArrowUp") { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
              if (e.key === "Enter" && entries[sel]) choose(entries[sel]);
            }}
          />
          <kbd>esc</kbd>
        </div>
        <ul className="palette-list" role="listbox">
          {entries.map((e, i) => (
            <li key={e.id} role="option" aria-selected={i === sel} className={i === sel ? "on" : ""} onMouseEnter={() => setSel(i)} onClick={() => choose(e)}>
              <Icon name={e.icon} />
              <span className="pl">{e.label}</span>
              <span className="ph">{e.hint}</span>
            </li>
          ))}
          {!entries.length ? <li className="empty">Nessun risultato</li> : null}
        </ul>
      </div>
    </div>
  );
}
