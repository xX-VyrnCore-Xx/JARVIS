import { useRef } from "react";
import { DEVICE_MODE } from "./lib/api";
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

const NAV: { view: View; label: string; owner?: boolean; glyph: string }[] = [
  { view: "dashboard", label: "Plancia", owner: true, glyph: "◎" },
  { view: "chat", label: "Chat", glyph: "◇" },
  { view: "tasks", label: "Attività", glyph: "▤" },
  { view: "automations", label: "Automazioni", owner: true, glyph: "↻" },
  { view: "memory", label: "Memoria", owner: true, glyph: "◈" },
  { view: "workspace", label: "Workspace", owner: true, glyph: "▣" },
  { view: "connectors", label: "Connettori", owner: true, glyph: "⇄" },
  { view: "devices", label: "Dispositivi", owner: true, glyph: "▢" },
  { view: "audit", label: "Audit", owner: true, glyph: "⛓" },
  { view: "system", label: "Sistema", owner: true, glyph: "⚙" },
];

function Shell() {
  const { view, show } = useApp();
  const setup = useRef<SetupHandle>(null);
  const owner = !DEVICE_MODE;
  return (
    <>
      <div className="hud-backdrop" aria-hidden="true"></div>
      <TopBar />
      <div className="layout">
        <nav className="nav" aria-label="Sezioni">
          {NAV.filter((n) => owner || !n.owner).map((n) => (
            <button key={n.view} className={`nav-item${view === n.view ? " active" : ""}`} data-view={n.view} onClick={() => show(n.view)}>
              <span className="glyph" aria-hidden="true">{n.glyph}</span>
              {n.label}
            </button>
          ))}
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
