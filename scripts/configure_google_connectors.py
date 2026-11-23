#!/usr/bin/env python3
"""Install and authorize Gmail and Google Calendar on a local JARVIS Core."""

from __future__ import annotations

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import webbrowser
from pathlib import Path

CONNECTORS = ("gmail", "google_calendar")


def load_credentials(path: Path) -> dict:
    try:
        with path.open("rb") as file:
            raw = file.read(16 * 1024 + 1)
        if len(raw) > 16 * 1024:
            raise ValueError("Il file di credenziali supera 16 KB")
        config = json.loads(raw)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("Impossibile leggere il JSON delle credenziali") from error
    if not isinstance(config, dict) or "installed" not in config:
        raise ValueError("Seleziona il JSON OAuth Google di tipo App desktop")
    return config


def local_core_url() -> str:
    value = os.environ.get("JARVIS_URL", "http://127.0.0.1:8765").rstrip("/")
    parsed = urllib.parse.urlsplit(value)
    if parsed.scheme != "http" or parsed.hostname not in {"localhost", "127.0.0.1", "::1"}:
        raise ValueError("JARVIS_URL deve puntare al Core locale su HTTP loopback")
    if parsed.username or parsed.password or parsed.path or parsed.query or parsed.fragment:
        raise ValueError("JARVIS_URL deve contenere solo origine e porta del Core")
    return value


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


CORE_OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())


def request(base: str, token: str, path: str, body: dict | None = None) -> dict:
    data = json.dumps(body).encode("utf-8") if body is not None else None
    headers = {"X-Jarvis-Token": token, "Host": urllib.parse.urlsplit(base).netloc}
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(f"{base}{path}", data=data, headers=headers)
    try:
        with CORE_OPENER.open(req, timeout=15) as response:
            result = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"Core ha rifiutato {path} (HTTP {error.code})") from None
    except (urllib.error.URLError, TimeoutError) as error:
        raise RuntimeError(f"Core non raggiungibile per {path}: {error}") from None
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError(f"Risposta Core non valida per {path}") from error
    if not isinstance(result, dict):
        raise RuntimeError(f"Risposta Core non valida per {path}")
    return result


def wait_for_authorization(base: str, token: str, connector_id: str) -> None:
    deadline = time.monotonic() + 300
    while time.monotonic() < deadline:
        connectors = request(base, token, "/api/connectors").get("items", [])
        current = next((item for item in connectors if item.get("id") == connector_id), None)
        if current and current.get("authorized") is True:
            print(f"Connettore {connector_id} autorizzato; token conservati cifrati dal Core.")
            return
        time.sleep(2)
    raise TimeoutError(f"Consenso OAuth per {connector_id} non completato entro 5 minuti")


def configure(base: str, token: str, credentials: dict) -> None:
    current = request(base, token, "/api/connectors").get("items", [])
    for connector_id in CONNECTORS:
        existing = next((item for item in current if item.get("id") == connector_id), None)
        if existing and existing.get("authorized") is True:
            print(f"Connettore {connector_id} già autorizzato.")
            continue
        started = request(
            base, token, f"/api/connectors/{connector_id}/oauth/import", credentials
        )
        authorize_url = started.get("authorize_url")
        parsed = urllib.parse.urlsplit(authorize_url or "")
        if parsed.scheme != "https" or parsed.netloc != "accounts.google.com":
            raise RuntimeError(f"URL OAuth inatteso per {connector_id}")
        print(
            f"{connector_id}: si aprirà Google per il consenso agli scope richiesti; "
            "completa il passaggio nel browser."
        )
        if not webbrowser.open(authorize_url):
            print(f"Apri questo URL nel browser: {authorize_url}")
        wait_for_authorization(base, token, connector_id)


def main() -> int:
    if len(sys.argv) != 2:
        print("Uso: JARVIS_TOKEN=... python3 scripts/configure_google_connectors.py client_secret.json", file=sys.stderr)
        return 2
    token = os.environ.get("JARVIS_TOKEN", "")
    if len(token) < 32 or not token.isalnum():
        print("JARVIS_TOKEN deve contenere almeno 32 caratteri alfanumerici.", file=sys.stderr)
        return 2
    try:
        base = local_core_url()
        configure(base, token, load_credentials(Path(sys.argv[1])))
    except (ValueError, RuntimeError, TimeoutError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
