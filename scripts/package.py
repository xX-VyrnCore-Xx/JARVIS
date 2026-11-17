"""Crea l'archivio di release di JARVIS (Fase 7, ADR-0015).

Uso: python scripts/package.py --binary core/target/release/jarvis --target linux-x86_64 --out dist

L'archivio contiene il Core precompilato in bin/, il runtime AI Python, i contratti, i lanciatori e la
documentazione essenziale: Rust non serve per avviarlo, Python 3.10+ sì. Accanto all'archivio viene
scritto il file .sha256. Solo libreria standard, identico su Linux, macOS e Windows.
"""

from __future__ import annotations

import argparse
import hashlib
import re
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
    if windows:
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as z:
            for src, arc in entries:
                z.write(src, arc)
    else:
        def executable(info: tarfile.TarInfo) -> tarfile.TarInfo:
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            if info.name.endswith(("/bin/jarvis", "/jarvis.sh")):
                info.mode = 0o755
            return info
        with tarfile.open(archive, "w:gz") as t:
            for src, arc in entries:
                t.add(src, arc, filter=executable)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (archive.parent / (archive.name + ".sha256")).write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(f"{archive} ({archive.stat().st_size // 1024} KiB) sha256 {digest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
