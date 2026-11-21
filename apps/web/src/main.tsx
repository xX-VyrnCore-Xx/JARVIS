import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles/hud.css";
import "./styles/desktop.css";
import "./styles/jarvis.css";

// Piattaforma: piccoli adattamenti per macOS e Windows (caratteri, raggi, barre di scorrimento) e per l'app
// desktop, che apre la pagina con ?app=desktop.
{
  const ua = navigator.userAgent;
  const root = document.documentElement;
  root.classList.add("jarvis-shell");
  root.classList.add(/Mac|iPhone|iPad/.test(ua) ? "platform-mac" : /Windows/.test(ua) ? "platform-win" : "platform-other");
  let desktop = new URLSearchParams(location.search).get("app") === "desktop";
  try {
    if (desktop) sessionStorage.setItem("jarvis.app", "desktop");
    desktop = desktop || sessionStorage.getItem("jarvis.app") === "desktop";
  } catch {
    /* archiviazione non disponibile */
  }
  if (desktop) root.classList.add("app-desktop");
}

// Tema: scuro (HUD) per impostazione, chiaro se scelto dall'utente.
try {
  if (localStorage.getItem("jarvis.theme") === "light") document.documentElement.dataset.theme = "light";
} catch {
  /* archiviazione non disponibile */
}

if ("serviceWorker" in navigator && location.protocol === "https:") navigator.serviceWorker.register("/sw.js").catch(() => {});

createRoot(document.getElementById("root")!).render(<App />);
