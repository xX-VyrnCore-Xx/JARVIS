#!/usr/bin/env bash
# Avvia il prototipo JARVIS: prepara l'ambiente Python, compila il Core e apre il server locale.
# Uso: ./jarvis.sh            interfaccia nel browser su http://localhost:8765
#      ./jarvis.sh desktop    app desktop (Tauri) che avvia il Core da sola
#      ./jarvis.sh voice-setup  installa i motori vocali locali e scarica i modelli (una volta)
#                             (variabili utili: JARVIS_PORT, JARVIS_DATA_DIR, JARVIS_AI_BACKEND,
#                              JARVIS_OLLAMA_URL, JARVIS_OLLAMA_MODEL — vedi README.md)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

fail() { echo "✗ $*" >&2; exit 1; }
command -v python3 >/dev/null || fail "Python 3.10+ non trovato: installalo da https://www.python.org/"
python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)' || fail "Serve Python 3.10 o successivo"
command -v cargo >/dev/null || fail "Rust non trovato: installalo da https://rustup.rs/ (poi riapri il terminale)"

VENV="$ROOT/.venv"
if [ ! -x "$VENV/bin/python" ]; then
  echo "→ Creo l'ambiente Python in .venv"
  python3 -m venv "$VENV" || fail "Impossibile creare .venv (su Debian/Ubuntu: sudo apt install python3-venv)"
fi
echo "→ Dipendenze Python"
"$VENV/bin/python" -m pip install -q --disable-pip-version-check -r "$ROOT/tests/contracts/requirements.txt"

echo "→ Compilo il Core (la prima volta richiede qualche minuto)"
(cd "$ROOT/core" && cargo build --release --locked -q -p jarvis-core)

if command -v ollama >/dev/null 2>&1 || curl -s -m 1 "${JARVIS_OLLAMA_URL:-http://127.0.0.1:11434}/api/tags" >/dev/null 2>&1; then
  echo "→ Ollama rilevato: JARVIS userà un modello locale se disponibile"
else
  echo "→ Ollama non rilevato: JARVIS parte in modalità offline (comandi semplici)."
  echo "  Per la conversazione libera: installa Ollama (https://ollama.com) ed esegui 'ollama pull qwen2.5:3b'"
fi

export JARVIS_PYTHON="$VENV/bin/python"
export JARVIS_HOME="$ROOT"

MODELS="${JARVIS_MODELS_DIR:-$ROOT/data/models}"
VOICE_NAME="${JARVIS_TTS_VOICE_NAME:-it_IT-paola-medium}"
if [ "${1:-}" = "voice-setup" ]; then
  echo "→ Motori vocali locali (faster-whisper, Piper, openWakeWord)"
  "$VENV/bin/python" -m pip install -q --disable-pip-version-check -r "$ROOT/ai/requirements-voice.txt"
  mkdir -p "$MODELS/piper"
  echo "→ Modello di riconoscimento ${JARVIS_STT_MODEL:-small} (download una tantum)"
  "$VENV/bin/python" -c "import sys; from faster_whisper import WhisperModel; WhisperModel(sys.argv[1], device='cpu', compute_type='int8', download_root=sys.argv[2])" "${JARVIS_STT_MODEL:-small}" "$MODELS"
  echo "→ Voce di sintesi $VOICE_NAME"
  "$VENV/bin/python" -m piper.download_voices "$VOICE_NAME" --download-dir "$MODELS/piper"
  echo "✓ Voce pronta. Avvia ./jarvis.sh e attiva il microfono nell'interfaccia."
  exit 0
fi
if [ -d "$MODELS" ]; then
  export JARVIS_MODELS_DIR="$MODELS"
  if [ -z "${JARVIS_TTS_VOICE:-}" ] && [ -f "$MODELS/piper/$VOICE_NAME.onnx" ]; then
    export JARVIS_TTS_VOICE="$MODELS/piper/$VOICE_NAME.onnx"
  fi
fi

if [ "${1:-}" = "desktop" ]; then
  shift
  if [ "$(uname -s)" = "Linux" ] && ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    fail "Per l'app desktop servono le librerie WebKitGTK: sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev"
  fi
  echo "→ Compilo l'app desktop (la prima volta richiede alcuni minuti)"
  (cd "$ROOT/apps/desktop/src-tauri" && cargo build --release --locked -q)
  exec "$ROOT/apps/desktop/src-tauri/target/release/jarvis-desktop" "$@"
fi

exec "$ROOT/core/target/release/jarvis" "$@"
