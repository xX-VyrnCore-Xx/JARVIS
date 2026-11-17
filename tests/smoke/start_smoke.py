"""Prova di avvio reale: dopo jarvis.sh / jarvis.ps1, il Core risponde e la chat funziona.

Uso: python tests/smoke/start_smoke.py http://127.0.0.1:8765 [timeout_s]
Solo libreria standard: gira identico su Linux, macOS e Windows.
"""

from __future__ import annotations

import json
import re
import sys
import time
import urllib.request


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

    page = get(f"{base}/", {"Host": f"localhost:{port}"})
    token = re.search(r'name="jarvis-token" content="([0-9a-f]{32})"', page).group(1)

    def chat(message: str) -> str:
        req = urllib.request.Request(
            f"{base}/api/chat",
            data=json.dumps({"message": message}).encode("utf-8"),
            headers={"Content-Type": "application/json", "X-Jarvis-Token": token, "Host": f"localhost:{port}"},
        )
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.loads(r.read().decode("utf-8"))["reply"]

    hello = chat("ciao")
    assert "JARVIS" in hello and "modalità" in hello, f"risposta inattesa o caratteri alterati: {hello!r}"
    print("✓ chat e caratteri accentati integri")
    assert chat("che ore sono").startswith("Sono le"), "strumento time.now non eseguito"
    print("✓ strumento eseguito tramite policy")
    return 0


if __name__ == "__main__":
    sys.exit(main())
