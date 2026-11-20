"""Prova di avvio reale: dopo jarvis.sh / jarvis.ps1, il Core risponde e la chat funziona.

Uso: python tests/smoke/start_smoke.py http://127.0.0.1:8765 [timeout_s]
Solo libreria standard: gira identico su Linux, macOS e Windows.
"""

from __future__ import annotations

import json
import os
import re
import sys
import time
import urllib.request

# Console Windows in cp1252: i simboli ✓/✗ non devono far fallire la prova.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")


def get(url: str, headers: dict | None = None) -> str:
    req = urllib.request.Request(url, headers=headers or {})
    with urllib.request.urlopen(req, timeout=5) as r:
        return r.read().decode("utf-8")


def main() -> int:
    base = sys.argv[1].rstrip("/")
    port = base.rsplit(":", 1)[1]
    deadline = time.time() + (float(sys.argv[2]) if len(sys.argv) > 2 else 900)
    health = None
    while time.time() < deadline:
        try:
            health = json.loads(get(f"{base}/api/health"))
            if health.get("ai", {}).get("status") == "up":
                break
        except Exception:  # noqa: BLE001 — il Core può essere ancora in compilazione
            pass
        time.sleep(3)
    else:
        print("✗ il Core non ha risposto entro il tempo previsto", health)
        return 1
    print("✓ Core attivo; AI:", health["ai"]["gateway"]["backend"]["name"])
    store = health.get("core", {}).get("secrets_store")
    expected = os.environ.get("JARVIS_EXPECT_SECRETS_STORE")
    if expected and store != expected:
        print(f"✗ chiave dei segreti in «{store}», attesa in «{expected}»")
        return 1
    print("✓ chiave dei segreti custodita in:", store)

    page = get(f"{base}/", {"Host": f"localhost:{port}"})
    token = re.search(r'name="jarvis-token" content="([0-9a-f]{32})"', page).group(1)

    def api(path: str, body: dict | None = None) -> dict:
        req = urllib.request.Request(
            f"{base}/api/{path}",
            data=json.dumps(body).encode("utf-8") if body is not None else None,
            headers={"Content-Type": "application/json", "X-Jarvis-Token": token, "Host": f"localhost:{port}"},
        )
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.loads(r.read().decode("utf-8"))

    def chat(message: str) -> dict:
        return api("chat", {"message": message})

    hello = chat("ciao")["reply"]
    assert "JARVIS" in hello and "modalità" in hello, f"risposta inattesa o caratteri alterati: {hello!r}"
    print("✓ chat e caratteri accentati integri")
    assert chat("che ore sono")["reply"].startswith("Sono le"), "strumento time.now non eseguito"
    print("✓ strumento eseguito tramite policy")
    pending = chat("scrivi prova-installazione.txt: verifica JARVIS")
    assert pending["task"]["state"] == "waiting_confirmation"
    confirmation = pending["confirmation"]["confirmation_id"]
    assert api(f"confirmations/{confirmation}", {"approve": True})["task"]["state"] == "succeeded"
    assert "verifica JARVIS" in chat("leggi prova-installazione.txt")["reply"]
    assert api("security")["audit_chain_valid"] is True
    print("✓ scrittura confermata, lettura e catena di audit")

    settings = {"provider": "nvidia_nim", "model": "moonshotai/kimi-k3.1-pro"}
    key = "synthetic-smoke-key"
    configured = api("cloud", dict(settings, key=key))
    nim = next(item for item in configured["providers"] if item["id"] == "nvidia_nim")
    assert nim["configured"] and nim["effective_rpm"] == 35
    assert key not in json.dumps(configured)
    removed = api("cloud", dict(settings, remove_key=True))
    assert not next(item for item in removed["providers"] if item["id"] == "nvidia_nim")["configured"]
    print("✓ configurazione NVIDIA NIM, limite e rimozione della credenziale di prova")
    return 0


if __name__ == "__main__":
    sys.exit(main())
