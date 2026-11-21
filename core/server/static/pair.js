"use strict";
// Associazione di un dispositivo remoto (RF-11): scambia il codice monouso con il token del dispositivo.
const ua = navigator.userAgent;
const platform = /Android/i.test(ua) ? "android" : /iPhone|iPad|iPod/i.test(ua) ? "ios" : "web";
const form = document.getElementById("pair-form");
const codeField = form.elements.namedItem("code");
const nameField = form.elements.namedItem("name");
const submit = form.querySelector("button[type=submit]");
function readInvite() {
  if (!location.hash) return;
  const invite = new URLSearchParams(location.hash.slice(1)).get("code");
  history.replaceState(null, "", location.pathname + location.search);
  if (invite && /^[A-Z0-9]{4}-?[A-Z0-9]{4}$/i.test(invite)) {
    codeField.value = invite.toUpperCase();
    nameField.focus();
  }
}
nameField.value = platform === "android" ? "Telefono Android" : platform === "ios" ? "iPhone" : "Browser";
readInvite();
window.addEventListener("hashchange", readInvite);
form.addEventListener("submit", async (e) => {
  e.preventDefault();
  const err = document.getElementById("pair-error");
  if (submit.disabled) return;
  err.textContent = "";
  submit.disabled = true;
  const res = await fetch("/api/devices/pair", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code: codeField.value, name: nameField.value, platform }),
  }).catch(() => null);
  const data = res ? await res.json().catch(() => ({})) : {};
  if (!res || !res.ok) {
    submit.disabled = false;
    err.textContent = (data.error && data.error.message) || "Associazione non riuscita: controlla la connessione.";
    return;
  }
  try { localStorage.setItem("jarvis-device", data.token); } catch { err.textContent = "Il browser non consente di salvare l’associazione. Genera un nuovo codice e abilita i dati del sito."; submit.disabled = false; return; }
  location.replace("/");
});
