// Pagina locale di attesa: il processo Tauri chiama showError se l'avvio del Core non riesce.
window.showError = (message) => {
  document.querySelector(".orb").style.animation = "none";
  document.getElementById("status").textContent = "JARVIS non è riuscito ad avviarsi.";
  const box = document.getElementById("error");
  box.textContent = message;
  box.style.display = "block";
};
