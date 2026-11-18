#!/usr/bin/env bash
# Avvia JARVIS: prepara l'ambiente Python, compila il Core e apre l'interfaccia nel browser.
#
# Uso: ./jarvis.sh               interfaccia nel browser su http://localhost:8765
#      ./jarvis.sh desktop       app desktop (Tauri) che avvia il Core da sola
#      ./jarvis.sh voice-setup   installa i motori vocali locali e scarica i modelli (una volta)
#      ./jarvis.sh doctor        controlla i prerequisiti e spiega cosa manca
#      ./jarvis.sh backup        backup cifrato dei dati (passphrase richiesta)
#      ./jarvis.sh restore FILE  ripristina un backup .jbak (con JARVIS chiuso)
#      ./jarvis.sh rollback      riporta i dati alla copia fatta prima dell'ultimo aggiornamento
#      ./jarvis.sh node URL CODICE  questo computer diventa un nodo esecutore del Core all'indirizzo URL
# In una release scaricata il Core è già compilato (bin/jarvis) e Rust non serve.
# Su Windows usa jarvis.cmd (doppio clic) oppure jarvis.ps1.
# Se il file non è eseguibile (es. scaricato come ZIP): bash jarvis.sh
#
# Variabili utili: JARVIS_PORT, JARVIS_DATA_DIR, JARVIS_AI_BACKEND, JARVIS_OLLAMA_MODEL,
# JARVIS_NO_BROWSER=1 (non aprire il browser) — vedi README.md.

# Avviato con `sh jarvis.sh` (dash su Ubuntu non conosce pipefail): si riavvia con bash.
if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT="${JARVIS_PORT:-8765}"
VENV="$ROOT/.venv"
CMD="${1:-}"
# Release precompilata: bin/jarvis presente e sorgenti del Core assenti.
if [ -x "$ROOT/bin/jarvis" ] && [ ! -f "$ROOT/core/Cargo.toml" ]; then PREBUILT=1; CORE_BIN="$ROOT/bin/jarvis"; else PREBUILT=0; CORE_BIN="$ROOT/core/target/release/jarvis"; fi

fail() { echo "✗ $*" >&2; exit 1; }
ok() { echo "✓ $*"; }
warn() { echo "! $*"; }

python_ok() { command -v python3 >/dev/null && python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)' 2>/dev/null; }

doctor() {
  local problems=0
  echo "JARVIS — controllo dei prerequisiti ($(uname -s) $(uname -m))"
  if python_ok; then ok "Python $(python3 -c 'import platform; print(platform.python_version())')"; else warn "Python 3.10+ mancante: https://www.python.org/downloads/"; problems=$((problems + 1)); fi
  if python3 -c 'import venv, ensurepip' 2>/dev/null; then ok "Modulo venv con pip"; else warn "venv/ensurepip mancante (Debian/Ubuntu: sudo apt install python3-venv)"; problems=$((problems + 1)); fi
  if [ "$PREBUILT" = 1 ]; then ok "Core precompilato (release): Rust non serve"; else
    if command -v cargo >/dev/null; then ok "Rust $(cargo --version | cut -d' ' -f2)"; else warn "Rust mancante: https://rustup.rs/ (poi riapri il terminale)"; problems=$((problems + 1)); fi
    if command -v cc >/dev/null || command -v gcc >/dev/null || command -v clang >/dev/null; then ok "Compilatore C (serve per SQLite)"; else
      warn "Compilatore C mancante (Ubuntu: sudo apt install build-essential · macOS: xcode-select --install)"; problems=$((problems + 1)); fi
    if [ -x "$CORE_BIN" ]; then ok "Core già compilato"; else echo "· Core non ancora compilato (succede al primo avvio)"; fi
  fi
  if (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null; then warn "La porta $PORT è occupata: imposta JARVIS_PORT=8766 o chiudi l'altro programma"; problems=$((problems + 1)); else ok "Porta $PORT libera"; fi
  if curl -s -m 1 "${JARVIS_OLLAMA_URL:-http://127.0.0.1:11434}/api/tags" >/dev/null 2>&1; then ok "Ollama attivo"; else echo "· Ollama non attivo (facoltativo: https://ollama.com, poi 'ollama pull qwen2.5:3b')"; fi
  if [ "$problems" -eq 0 ]; then ok "Tutto pronto: ./jarvis.sh"; else echo "Problemi trovati: $problems"; return 1; fi
}

if [ "$CMD" = "doctor" ]; then
  doctor
  exit $?
fi

python_ok || { doctor || true; fail "Serve Python 3.10 o successivo"; }
[ "$PREBUILT" = 1 ] || command -v cargo >/dev/null || { doctor || true; fail "Rust non trovato: installalo da https://rustup.rs/ e riapri il terminale"; }

# Ambiente Python: ricreato se un tentativo precedente l'ha lasciato senza pip.
if [ -d "$VENV" ] && ! "$VENV/bin/python" -m pip --version >/dev/null 2>&1; then
  warn "Ambiente .venv incompleto: lo ricreo"
  rm -rf "$VENV"
fi
if [ ! -x "$VENV/bin/python" ]; then
  echo "→ Creo l'ambiente Python in .venv"
  python3 -m venv "$VENV" || { rm -rf "$VENV"; fail "Impossibile creare .venv (Debian/Ubuntu: sudo apt install python3-venv)"; }
fi
echo "→ Dipendenze Python"
"$VENV/bin/python" -m pip install -q --disable-pip-version-check -r "$ROOT/tests/contracts/requirements.txt" \
  || fail "Installazione delle dipendenze Python non riuscita (serve la rete la prima volta)"

if [ "$PREBUILT" = 1 ]; then
  echo "→ Core precompilato: $("$CORE_BIN" --version 2>/dev/null || echo release)"
elif [ ! -x "$CORE_BIN" ]; then
  echo "→ Compilo il Core: la prima volta richiede alcuni minuti, l'avanzamento è qui sotto"
  (cd "$ROOT/core" && cargo build --release --locked -p jarvis-core) || { doctor || true; fail "Compilazione non riuscita: vedi i messaggi sopra"; }
else
  echo "→ Aggiorno il Core"
  (cd "$ROOT/core" && cargo build --release --locked -q -p jarvis-core) || fail "Compilazione non riuscita: riprova con ./jarvis.sh doctor"
fi

export JARVIS_PYTHON="$VENV/bin/python"
export JARVIS_HOME="$ROOT"

MODELS="${JARVIS_MODELS_DIR:-$ROOT/data/models}"
VOICE_NAME="${JARVIS_TTS_VOICE_NAME:-it_IT-paola-medium}"
if [ "$CMD" = "voice-setup" ]; then
  echo "→ Motori vocali locali (faster-whisper, Piper, openWakeWord)"
  "$VENV/bin/python" -m pip install -q --disable-pip-version-check -r "$ROOT/ai/requirements-voice.txt"
  mkdir -p "$MODELS/piper"
  echo "→ Modello di riconoscimento ${JARVIS_STT_MODEL:-small} (download una tantum)"
  "$VENV/bin/python" -c "import sys; from faster_whisper import WhisperModel; WhisperModel(sys.argv[1], device='cpu', compute_type='int8', download_root=sys.argv[2])" "${JARVIS_STT_MODEL:-small}" "$MODELS"
  echo "→ Voce di sintesi $VOICE_NAME"
  "$VENV/bin/python" -m piper.download_voices "$VOICE_NAME" --download-dir "$MODELS/piper"
  ok "Voce pronta. Avvia ./jarvis.sh e attiva il microfono nell'interfaccia."
  exit 0
fi
if [ -d "$MODELS" ]; then
  export JARVIS_MODELS_DIR="$MODELS"
  if [ -z "${JARVIS_TTS_VOICE:-}" ] && [ -f "$MODELS/piper/$VOICE_NAME.onnx" ]; then
    export JARVIS_TTS_VOICE="$MODELS/piper/$VOICE_NAME.onnx"
  fi
fi

if [ "$CMD" = "desktop" ]; then
  shift
  [ "$PREBUILT" = 1 ] && fail "La release non include l'app desktop: usa ./jarvis.sh (interfaccia nel browser) oppure i sorgenti"
  if [ "$(uname -s)" = "Linux" ] && ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    fail "Per l'app desktop servono le librerie WebKitGTK: sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev"
  fi
  echo "→ Compilo l'app desktop (la prima volta richiede alcuni minuti)"
  (cd "$ROOT/apps/desktop/src-tauri" && cargo build --release --locked)
  exec "$ROOT/apps/desktop/src-tauri/target/release/jarvis-desktop" "$@"
fi

if curl -s -m 1 "${JARVIS_OLLAMA_URL:-http://127.0.0.1:11434}/api/tags" >/dev/null 2>&1; then
  echo "→ Ollama rilevato: JARVIS userà un modello locale se disponibile"
else
  echo "→ Ollama non rilevato: JARVIS parte in modalità offline (comandi semplici)."
  echo "  Per la conversazione libera: installa Ollama (https://ollama.com) ed esegui 'ollama pull qwen2.5:3b'"
fi

# Apre il browser quando il Core risponde (disattivabile con JARVIS_NO_BROWSER=1).
open_browser() {
  local url="http://localhost:$PORT/"
  for _ in $(seq 1 60); do
    if curl -sf -m 1 "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1; then
      if command -v open >/dev/null && [ "$(uname -s)" = "Darwin" ]; then open "$url"
      elif command -v wslview >/dev/null; then wslview "$url"
      elif command -v xdg-open >/dev/null && [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then xdg-open "$url" >/dev/null 2>&1
      else echo "→ Apri nel browser: $url"; fi
      return 0
    fi
    sleep 1
  done
}
if [ "$CMD" = "backup" ] || [ "$CMD" = "restore" ] || [ "$CMD" = "rollback" ] || [ "$CMD" = "node" ]; then
  # Manutenzione da terminale: il Core esegue l'operazione ed esce (restore e rollback richiedono JARVIS chiuso).
  exec "$CORE_BIN" "$@"
fi
if [ "${JARVIS_NO_BROWSER:-0}" != "1" ]; then
  open_browser &
fi

export JARVIS_PORT="$PORT"
exec "$CORE_BIN" "$@"
