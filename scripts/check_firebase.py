#!/usr/bin/env python3
"""Verifica la configurazione Firebase (FCM) di JARVIS senza mai stampare chiavi o token.

Controlli, tutti facoltativi e combinabili:
  --google-services FILE   google-services.json dell'app Android (package local.jarvis.app)
  --service-account FILE   chiave JSON dell'account di servizio usato dal Core
  --apk FILE               APK di una release: controlla che l'SDK abbia gli identificativi Firebase
  --core URL --token TOK   stato di consegna push del Core in esecuzione (GET /api/push/status)

Esempio:
  python3 scripts/check_firebase.py --google-services google-services.json \\
      --service-account ~/jarvis/fcm-service-account.json --apk JARVIS_android-release.apk
"""

import argparse
import json
import re
import stat
import sys
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

PACKAGE = "local.jarvis.app"
OK, WARN, FAIL = "✓", "!", "✗"
problems = 0


def report(kind: str, text: str) -> None:
    global problems
    if kind == FAIL:
        problems += 1
    print(f"{kind} {text}")


def load_json(path: str, what: str):
    try:
        return json.loads(Path(path).expanduser().read_text(encoding="utf-8"))
    except OSError as error:
        report(FAIL, f"{what}: file non leggibile ({error.strerror})")
    except json.JSONDecodeError:
        report(FAIL, f"{what}: il file non è un JSON valido")
    return None


def check_google_services(path: str) -> str | None:
    config = load_json(path, "google-services.json")
    if config is None:
        return None
    try:
        project = config["project_info"]
        client = next(
            c for c in config["client"] if c["client_info"]["android_client_info"]["package_name"] == PACKAGE
        )
    except (KeyError, StopIteration, TypeError):
        report(FAIL, f"google-services.json: manca l'app Android con package {PACKAGE}. "
                     "Nella console Firebase registra l'app con questo nome esatto e scarica di nuovo il file.")
        return None
    project_id = project.get("project_id", "")
    app_id = client["client_info"].get("mobilesdk_app_id", "")
    keys = client.get("api_key") or [{}]
    api_key = keys[0].get("current_key", "")
    sender = str(project.get("project_number", ""))
    for name, value, pattern in (
        ("ID progetto", project_id, r"^[a-z][a-z0-9-]{4,28}[a-z0-9]$"),
        ("ID app", app_id, r"^1:\d+:android:[0-9a-f]+$"),
        ("chiave API", api_key, r"^AIza[0-9A-Za-z_\-]{35}$"),
        ("sender ID", sender, r"^\d{6,}$"),
    ):
        if re.match(pattern, value or ""):
            report(OK, f"google-services.json: {name} presente e ben formato")
        else:
            report(FAIL, f"google-services.json: {name} mancante o in un formato inatteso")
    print("  Le quattro Actions variables si importano con: python3 scripts/configure_firebase_actions.py "
          "<google-services.json>")
    return project_id or None


def check_service_account(path: str, expected_project: str | None) -> None:
    config = load_json(path, "account di servizio")
    if config is None:
        return
    p = Path(path).expanduser()
    mode = stat.S_IMODE(p.stat().st_mode)
    if mode & 0o077 and sys.platform != "win32":
        report(WARN, f"account di servizio: il file è leggibile da altri utenti (permessi {mode:o}); "
                     f"usa: chmod 600 {p}")
    else:
        report(OK, "account di servizio: permessi del file adeguati")
    if config.get("type") == "service_account":
        report(OK, "account di servizio: tipo corretto")
    else:
        report(FAIL, "account di servizio: non è una chiave di tipo service_account "
                     "(non usare google-services.json né il file OAuth client_secret_*.json)")
    for field in ("project_id", "client_email", "private_key", "private_key_id"):
        if config.get(field):
            report(OK, f"account di servizio: campo {field} presente")
        else:
            report(FAIL, f"account di servizio: campo {field} mancante")
    if "BEGIN PRIVATE KEY" not in str(config.get("private_key", "")):
        report(FAIL, "account di servizio: la chiave privata non è in formato PEM PKCS#8")
    if config.get("token_uri", "https://oauth2.googleapis.com/token") != "https://oauth2.googleapis.com/token":
        report(FAIL, "account di servizio: token_uri diverso da quello di Google (il Core lo rifiuterebbe)")
    if expected_project and config.get("project_id") and config["project_id"] != expected_project:
        report(FAIL, f"il progetto dell'account di servizio ({config['project_id']}) non coincide con quello "
                     f"dell'app Android ({expected_project}): FCM rifiuterebbe i messaggi")
    elif expected_project:
        report(OK, "l'account di servizio e l'app Android sono dello stesso progetto")
    print("  Avvia il Core con JARVIS_FCM_SERVICE_ACCOUNT_FILE=<percorso del file>. Il ruolo dell'account deve "
          "essere «Firebase Cloud Messaging API Admin».")


def check_apk(path: str) -> None:
    try:
        archive = zipfile.ZipFile(Path(path).expanduser())
    except (OSError, zipfile.BadZipFile):
        report(FAIL, "APK: file non leggibile")
        return
    dex = "".join(archive.read(n).decode("latin-1") for n in archive.namelist() if n.endswith(".dex"))
    has_id = re.search(r"1:\d{6,}:android:[0-9a-f]{12,}", dex) is not None
    has_key = re.search(r"AIza[0-9A-Za-z_\-]{35}", dex) is not None
    if has_id and has_key:
        report(OK, "APK: contiene gli identificativi Firebase, le push sono attivabili")
    else:
        report(FAIL, "APK: NON contiene gli identificativi Firebase. È stato costruito senza le quattro Actions "
                     "variables JARVIS_FIREBASE_*: l'app non può registrarsi alle push. Imposta le variabili e "
                     "pubblica una nuova release.")


def check_core(url: str, token: str) -> None:
    request = urllib.request.Request(url.rstrip("/") + "/api/push/status", headers={"X-Jarvis-Token": token})
    try:
        with urllib.request.urlopen(request, timeout=8) as response:
            status = json.load(response)
    except (urllib.error.URLError, OSError, json.JSONDecodeError) as error:
        report(FAIL, f"Core: stato push non raggiungibile ({type(error).__name__})")
        return
    delivery = status.get("delivery")
    message = {
        "ready": (OK, "Core: credenziale dell'account di servizio valida"),
        "configuration_missing": (FAIL, "Core: manca JARVIS_FCM_SERVICE_ACCOUNT_FILE (o il file non esiste): "
                                        "nessuna push può partire"),
        "configuration_invalid": (FAIL, "Core: il file dell'account di servizio non è valido o ha un project_id "
                                        "non ammesso"),
    }.get(delivery, (WARN, f"Core: stato di consegna sconosciuto ({delivery})"))
    report(*message)
    registered = status.get("registered", 0)
    report(OK if registered else WARN,
           f"Core: {registered} registrazioni push valide"
           + ("" if registered else " (nessun telefono registrato: in Android apri Notifiche → Attiva notifiche push)"))
    if status.get("invalid_pending"):
        report(WARN, f"Core: {status['invalid_pending']} registrazioni da eliminare (verranno rimosse al prossimo invio)")
    print(f"  Ultimo invio riuscito: {status.get('last_ok_at') or 'mai'}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--google-services")
    parser.add_argument("--service-account")
    parser.add_argument("--apk")
    parser.add_argument("--core")
    parser.add_argument("--token")
    args = parser.parse_args()
    if not any((args.google_services, args.service_account, args.apk, args.core)):
        parser.print_help()
        return 2
    project = check_google_services(args.google_services) if args.google_services else None
    if args.service_account:
        check_service_account(args.service_account, project)
    if args.apk:
        check_apk(args.apk)
    if args.core:
        if not args.token:
            report(FAIL, "Core: serve --token (il token di sessione del proprietario)")
        else:
            check_core(args.core, args.token)
    print()
    print("Tutto a posto." if not problems else f"{problems} problema/i da risolvere.")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
