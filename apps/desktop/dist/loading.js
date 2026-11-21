// Tauri comunica un eventuale errore mentre il Core si sta avviando.
window.showError = (message) => {
  document.body.classList.add("failed");
  document.getElementById("status").textContent = "Avvio di JARVIS interrotto.";
  const box = document.getElementById("error");
  box.textContent = message;
  box.hidden = false;
};
if (window.jarvisStartupError) window.showError(window.jarvisStartupError);
