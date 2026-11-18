"""Scarica un Python autonomo (python-build-standalone) da includere nei programmi di installazione (ADR-0016).

Uso: python scripts/fetch_python.py --triple x86_64-unknown-linux-gnu --dest apps/desktop/src-tauri/python

Sceglie dall'ultima release di astral-sh/python-build-standalone l'archivio `install_only` di CPython
3.12 per la piattaforma indicata, ne verifica l'impronta SHA-256 pubblicata e lo estrae in --dest.
Nella CI usa GITHUB_TOKEN per l'API di GitHub. Solo libreria standard.

I collegamenti simbolici (es. bin/python3 -> python3.12) vengono sostituiti da copie: il bundler degli
installer non li conserva. Con --trim, da eseguire dopo l'installazione delle dipendenze, rimuove le
parti non usate da JARVIS (test della libreria standard, IDLE, Tk, intestazioni C) per ridurre gli installer.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

API = "https://api.github.com/repos/astral-sh/python-build-standalone/releases/latest"


def get(url: str, accept: str = "application/vnd.github+json") -> bytes:
    headers = {"Accept": accept, "User-Agent": "jarvis-release"}
    token = os.environ.get("GITHUB_TOKEN")
    if token and url.startswith("https://api.github.com/"):
        headers["Authorization"] = f"Bearer {token}"
    with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=300) as r:  # noqa: S310
        return r.read()


# Percorsi da rimuovere, separati per sistema: su Windows i nomi ignorano maiuscole e minuscole e un
# modello Unix come lib/thread* colpirebbe Lib/threading.py della libreria standard.
TRIM_UNIX = [
    "lib/python3.*/test", "lib/python3.*/idlelib", "lib/python3.*/tkinter", "lib/python3.*/turtledemo",
    "lib/python3.*/lib2to3", "lib/python3.*/ensurepip", "lib/tcl8*", "lib/tk8*", "lib/itcl4*", "lib/thread2*",
    "share", "include",
    # Librerie statiche e file per compilare estensioni C: JARVIS non compila nulla sul computer dell'utente.
    "lib/python3.*/config-*", "lib/*.a",
]
TRIM_WINDOWS = ["Lib/test", "Lib/idlelib", "Lib/tkinter", "Lib/turtledemo", "Lib/lib2to3", "Lib/ensurepip", "tcl", "include"]


def dereference(root: Path) -> int:
    """Sostituisce i collegamenti simbolici ai file con copie reali; restituisce quanti ne ha sostituiti."""
    count = 0
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            target = path.resolve()
            path.unlink()
            if target.is_file():
                shutil.copy2(target, path)
                count += 1
            elif target.is_dir():
                shutil.copytree(target, path, symlinks=False)
                count += 1
    return count


def trim(root: Path) -> int:
    before = sum(f.stat().st_size for f in root.rglob("*") if f.is_file())
    patterns = TRIM_WINDOWS if (root / "python.exe").is_file() else TRIM_UNIX
    for pattern in patterns:
        for path in root.glob(pattern):
            shutil.rmtree(path) if path.is_dir() else path.unlink()
    for cache in list(root.rglob("__pycache__")):
        shutil.rmtree(cache, ignore_errors=True)
    after = sum(f.stat().st_size for f in root.rglob("*") if f.is_file())
    return (before - after) // 1048576


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--triple", required=True)
    parser.add_argument("--dest", required=True, type=Path)
    parser.add_argument("--minor", default="3.12")
    parser.add_argument("--trim", action="store_true", help="riduce un Python già scaricato in --dest")
    args = parser.parse_args()
    if args.trim:
        print(f"Python ridotto di {trim(args.dest)} MB")
        return 0
    release = json.loads(get(API))
    pattern = re.compile(rf"^cpython-{re.escape(args.minor)}\.\d+\+\d+-{re.escape(args.triple)}-install_only\.tar\.gz$")
    assets = {a["name"]: a for a in release["assets"]}
    names = sorted((n for n in assets if pattern.match(n)), key=lambda n: int(n.split("+")[0].rsplit(".", 1)[1]))
    if not names:
        sys.exit(f"nessun Python {args.minor} per {args.triple} nella release {release.get('tag_name')}")
    name = names[-1]
    data = get(assets[name]["browser_download_url"], "application/octet-stream")
    digest = hashlib.sha256(data).hexdigest()
    expected = None
    if name + ".sha256" in assets:
        expected = get(assets[name + ".sha256"]["browser_download_url"], "application/octet-stream").decode().split()[0]
    elif "SHA256SUMS" in assets:
        for line in get(assets["SHA256SUMS"]["browser_download_url"], "application/octet-stream").decode().splitlines():
            parts = line.split()
            if len(parts) == 2 and parts[1].lstrip("*") == name:
                expected = parts[0]
    if expected is None:
        sys.exit(f"impronta SHA-256 non pubblicata per {name}")
    if expected.lower() != digest:
        sys.exit(f"impronta errata per {name}: {digest} invece di {expected}")
    with tempfile.TemporaryDirectory() as tmp:
        archive = Path(tmp) / name
        archive.write_bytes(data)
        with tarfile.open(archive) as t:
            t.extractall(tmp, filter="data")
        if args.dest.exists():
            shutil.rmtree(args.dest)
        args.dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(Path(tmp) / "python"), args.dest)
    print(f"collegamenti simbolici sostituiti da copie: {dereference(args.dest)}")
    print(f"{name} ({len(data) // 1048576} MB, sha256 {digest[:16]}…) -> {args.dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
