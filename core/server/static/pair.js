"use strict";
// Associazione di un dispositivo remoto (RF-11): scambia il codice monouso con il token del dispositivo.
const ua = navigator.userAgent;
const platform = /Android/i.test(ua) ? "android" : /iPhone|iPad|iPod/i.test(ua) ? "ios" : "web";
const form = document.getElementById("pair-form");
form.name.value = platform === "android" ? "Telefono Android" : platform === "ios" ? "iPhone" : "Browser";
form.addEventListener("submit", async (e) => {
  e.preventDefault();
  const err = document.getElementById("pair-error");
  err.textContent = "";
  const res = await fetch("/api/devices/pair", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code: form.code.value, name: form.name.value, platform }),
  }).catch(() => null);
  const data = res ? await res.json().catch(() => ({})) : {};
  if (!res || !res.ok) {
    err.textContent = (data.error && data.error.message) || "Associazione non riuscita: controlla la connessione.";
    return;
  }
  try { localStorage.setItem("jarvis-device", data.token); } catch { err.textContent = "Il browser non consente di salvare l'associazione."; return; }
  location.replace("/");
});
