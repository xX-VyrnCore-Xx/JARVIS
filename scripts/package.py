"""Crea l'archivio di release di JARVIS (Fase 7, ADR-0015).

Uso: python scripts/package.py --binary core/target/release/jarvis --target linux-x86_64 --out dist

L'archivio contiene il Core precompilato in bin/, il runtime AI Python, i contratti, i lanciatori e la
documentazione essenziale: Rust non serve per avviarlo, Python 3.10+ sì. Accanto all'archivio viene
scritto il file .sha256. Solo libreria standard, identico su Linux, macOS e Windows.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import re
import shutil
import sys
import tarfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INCLUDE = [
    "ai/jarvis_ai",
    "ai/requirements-voice.txt",
    "contracts",
    "tests/contracts/requirements.txt",
    "jarvis.sh",
    "jarvis.ps1",
    "jarvis.cmd",
    "README.md",
    "docs/ACCEPTANCE.md",
]
SKIP = re.compile(r"(__pycache__|\.pyc$|\.pyo$)")
ARCHIVE_EPOCH = 315532800  # 1980-01-01 UTC, prima data rappresentabile in ZIP.


def version() -> str:
    text = (ROOT / "core/server/Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    if not match:
        sys.exit("versione non trovata in core/server/Cargo.toml")
    return match.group(1)


def files() -> list[Path]:
    out: list[Path] = []
    for item in INCLUDE:
        path = ROOT / item
        if not path.exists():
            sys.exit(f"manca {item}")
        candidates = [path] if path.is_file() else sorted(p for p in path.rglob("*") if p.is_file())
        out += [p for p in candidates if not SKIP.search(p.as_posix())]
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--target", required=True, help="es. linux-x86_64, macos-aarch64, windows-x86_64")
    parser.add_argument("--out", default="dist", type=Path)
    args = parser.parse_args()
    if not args.binary.is_file():
        sys.exit(f"binario non trovato: {args.binary}")
    name = f"jarvis-{version()}-{args.target}"
    args.out.mkdir(parents=True, exist_ok=True)
    windows = args.target.startswith("windows")
    archive = args.out / (name + (".zip" if windows else ".tar.gz"))
    exe = "bin/jarvis.exe" if windows else "bin/jarvis"
    entries = [(p, f"{name}/{p.relative_to(ROOT).as_posix()}") for p in files()]
    entries.append((args.binary, f"{name}/{exe}"))
    entries.sort(key=lambda entry: entry[1])
    if windows:
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as z:
            for src, arc in entries:
                info = zipfile.ZipInfo(arc)
                info.create_system = 3  # Metadati POSIX anche quando il pacchetto è generato su Windows.
                info.external_attr = 0o100644 << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                with src.open("rb") as source, z.open(info, "w") as output:
                    shutil.copyfileobj(source, output)
    else:
        executables = {f"{name}/{exe}", f"{name}/jarvis.sh"}
        with (
            archive.open("wb") as output,
            gzip.GzipFile(filename="", fileobj=output, mtime=ARCHIVE_EPOCH) as compressed,
            tarfile.open(fileobj=compressed, mode="w|") as t,
        ):
            for src, arc in entries:
                info = tarfile.TarInfo(arc)
                info.mtime = ARCHIVE_EPOCH
                info.mode = 0o755 if arc in executables else 0o644
                info.size = src.stat().st_size
                with src.open("rb") as source:
                    t.addfile(info, source)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (archive.parent / (archive.name + ".sha256")).write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(f"{archive} ({archive.stat().st_size // 1024} KiB) sha256 {digest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
