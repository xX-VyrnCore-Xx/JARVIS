#!/usr/bin/env python3
"""Import the Android Firebase client identifiers into GitHub Actions variables."""

import json
import subprocess
import sys
from pathlib import Path

REPOSITORY = "xX-VyrnCore-Xx/JARVIS"
PACKAGE_NAME = "local.jarvis.app"


def main() -> int:
    config_path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("google-services.json")
    try:
        config = json.loads(config_path.read_text(encoding="utf-8"))
        project = config["project_info"]
        client = next(
            item
            for item in config["client"]
            if item["client_info"]["android_client_info"]["package_name"] == PACKAGE_NAME
        )
        values = {
            "JARVIS_FIREBASE_PROJECT_ID": project["project_id"],
            "JARVIS_FIREBASE_SENDER_ID": project["project_number"],
            "JARVIS_FIREBASE_APP_ID": client["client_info"]["mobilesdk_app_id"],
            "JARVIS_FIREBASE_API_KEY": client["api_key"][0]["current_key"],
        }
    except (OSError, KeyError, IndexError, StopIteration, json.JSONDecodeError) as error:
        print(f"Configurazione Firebase non valida o app {PACKAGE_NAME} assente: {error}", file=sys.stderr)
        return 2

    if any(not isinstance(value, str) or not value for value in values.values()):
        print("La configurazione Firebase contiene identificativi vuoti o non testuali.", file=sys.stderr)
        return 2

    check = subprocess.run(
        ["gh", "variable", "list", "--repo", REPOSITORY],
        capture_output=True,
        text=True,
        check=False,
    )
    if check.returncode:
        print(
            "Impossibile accedere alle Actions variables. Autenticati con gh e abilita "
            "Actions: write nel token; nessuna variabile è stata modificata.",
            file=sys.stderr,
        )
        return check.returncode

    for name, value in values.items():
        result = subprocess.run(
            ["gh", "variable", "set", name, "--repo", REPOSITORY],
            input=value,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode:
            print(f"Impostazione di {name} non riuscita: {result.stderr.strip()}", file=sys.stderr)
            return result.returncode
        print(f"Configurata la variable Actions {name}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
