"""Scarica un Python autonomo (python-build-standalone) da includere nei programmi di installazione (ADR-0016).

Uso: python scripts/fetch_python.py --triple x86_64-unknown-linux-gnu --dest apps/desktop/src-tauri/python

Sceglie dall'ultima release di astral-sh/python-build-standalone l'archivio `install_only` di CPython
3.12 per la piattaforma indicata, ne verifica l'impronta SHA-256 pubblicata e lo estrae in --dest.
Nella CI usa GITHUB_TOKEN per l'API di GitHub. Solo libreria standard.
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--triple", required=True)
    parser.add_argument("--dest", required=True, type=Path)
    parser.add_argument("--minor", default="3.12")
    args = parser.parse_args()
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
    print(f"{name} ({len(data) // 1048576} MB, sha256 {digest[:16]}…) -> {args.dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
