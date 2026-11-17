"use strict";
// Avvio dell'interfaccia: un dispositivo remoto non ancora associato va alla pagina di associazione (RF-11).
(() => {
  const local = document.querySelector('meta[name="jarvis-token"]').content !== "";
  let paired = false;
  try { paired = !!localStorage.getItem("jarvis-device"); } catch { /* archiviazione non disponibile */ }
  if (!local && !paired) { location.replace("/pair"); return; }
  const s = document.createElement("script");
  s.src = "/app.js";
  document.body.append(s);
})();
