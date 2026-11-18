import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles/hud.css";

// Tema: scuro (HUD) per impostazione, chiaro se scelto dall'utente.
try {
  if (localStorage.getItem("jarvis.theme") === "light") document.documentElement.dataset.theme = "light";
} catch {
  /* archiviazione non disponibile */
}

if ("serviceWorker" in navigator && location.protocol === "https:") navigator.serviceWorker.register("/sw.js").catch(() => {});

createRoot(document.getElementById("root")!).render(<App />);
