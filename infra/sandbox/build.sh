#!/bin/sh
# Costruisce jarvis-sandbox:latest per il terminale sandbox. Richiede Docker e accesso alla rete (solo qui:
# i container del terminale girano sempre senza rete).
set -eu
cd "$(dirname "$0")"
docker build --pull -t jarvis-sandbox:latest .
echo "Immagine jarvis-sandbox:latest pronta."
