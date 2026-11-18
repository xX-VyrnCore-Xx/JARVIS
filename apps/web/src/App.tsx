import { useEffect, useRef, useState } from "react";
import { api, DEVICE_MODE } from "./lib/api";
import { CommandPalette } from "./components/CommandPalette";
import { Icon } from "./components/Icon";
import { SetupDialog } from "./components/SetupDialog";
import type { SetupHandle } from "./components/SetupDialog";
import { Toasts, TopBar } from "./components/TopBar";
import { AppProvider, useApp } from "./state";
import type { View } from "./state";
import { Automations } from "./views/Automations";
import { Chat } from "./views/Chat";
import { Dashboard } from "./views/Dashboard";
import { Memory } from "./views/Memory";
import { Audit, Connectors, Devices, Tasks, Workspace } from "./views/Simple";
import { System } from "./views/System";

interface NavEntry {
  view: View;
  label: string;
  owner?: boolean;
  icon: string;
  group: string;
}

const NAV: NavEntry[] = [
  { view: "dashboard", label: "Plancia", owner: true, icon: "dashboard", group: "Operazioni" },
  { view: "chat", label: "Chat", icon: "chat", group: "Operazioni" },
  { view: "tasks", label: "Attività", icon: "tasks", group: "Operazioni" },
  { view: "automations", label: "Automazioni", owner: true, icon: "automations", group: "Operazioni" },
  { view: "memory", label: "Memoria", owner: true, icon: "memory", group: "Dati" },
  { view: "workspace", label: "Workspace", owner: true, icon: "workspace", group: "Dati" },
  { view: "connectors", label: "Connettori", owner: true, icon: "connectors", group: "Integrazioni" },
  { view: "devices", label: "Dispositivi", owner: true, icon: "devices", group: "Integrazioni" },
  { view: "audit", label: "Audit", owner: true, icon: "audit", group: "Sistema" },
  { view: "system", label: "Sistema", owner: true, icon: "system", group: "Sistema" },
];

/** Conferme in attesa, per il contatore sulla voce Chat. */
function usePending() {
  const { activity, notifTick } = useApp();
  const [n, setN] = useState(0);
  useEffect(() => {
    api<{ items: unknown[] }>("/api/confirmations").then((d) => setN(d.items.length)).catch(() => {});
  }, [activity, notifTick]);
  return n;
}

function Shell() {
  const { view, show } = useApp();
  const setup = useRef<SetupHandle>(null);
  const owner = !DEVICE_MODE;
  const pending = usePending();
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem("jarvis.nav.collapsed") === "1";
    } catch {
      return false;
    }
  });
  const entries = NAV.filter((n) => owner || !n.owner);
  const groups = [...new Set(entries.map((n) => n.group))];
  return (
    <>
      <div className="hud-backdrop" aria-hidden="true">
        <i className="orb orb-a"></i>
        <i className="orb orb-b"></i>
        <i className="orb orb-c"></i>
      </div>
      <TopBar />
      <div className={`layout${collapsed ? " nav-collapsed" : ""}`}>
        <nav className="nav" aria-label="Sezioni">
          {groups.map((g) => (
            <div className="nav-group" key={g}>
              <div className="nav-group-title" aria-hidden="true">{g}</div>
              {entries.filter((n) => n.group === g).map((n) => (
                <button key={n.view} className={`nav-item${view === n.view ? " active" : ""}`} data-view={n.view} title={n.label} onClick={() => show(n.view)}>
                  <Icon name={n.icon} className="glyph" />
                  <span className="nav-label">{n.label}</span>
                  {n.view === "chat" && pending ? <span className="nav-badge" aria-label={`${pending} conferme in attesa`}>{pending}</span> : null}
                </button>
              ))}
            </div>
          ))}
          <button
            className="nav-collapse" title={collapsed ? "Espandi il menu" : "Riduci il menu"} aria-label={collapsed ? "Espandi il menu" : "Riduci il menu"}
            onClick={() => {
              const next = !collapsed;
              setCollapsed(next);
              try {
                localStorage.setItem("jarvis.nav.collapsed", next ? "1" : "0");
              } catch {
                /* niente */
              }
            }}
          >
            <Icon name={collapsed ? "expand" : "collapse"} />
          </button>
        </nav>
        <main>
          {owner ? <Dashboard active={view === "dashboard"} /> : null}
          <Chat active={view === "chat"} />
          <Tasks active={view === "tasks"} />
          {owner ? (
            <>
              <Automations active={view === "automations"} />
              <Memory active={view === "memory"} />
              <Workspace active={view === "workspace"} />
              <Connectors active={view === "connectors"} />
              <Devices active={view === "devices"} />
              <Audit active={view === "audit"} />
              <System active={view === "system"} onSetup={() => setup.current?.open()} />
            </>
          ) : null}
        </main>
      </div>
      <Toasts />
      <CommandPalette views={entries.map((n) => ({ view: n.view, label: n.label, icon: n.icon }))} />
      {owner ? <SetupDialog ref={setup} /> : null}
    </>
  );
}

export function App() {
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  );
}
