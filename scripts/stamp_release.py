"""Stamp a verified source checkout with the version used by both installers."""
import argparse
import json
from pathlib import Path
import re


def replace_once(path, pattern, replacement):
    source = path.read_text(encoding="utf-8")
    updated, count = re.subn(pattern, replacement, source, flags=re.MULTILINE)
    if count != 1:
        raise ValueError(f"versione attesa una volta in {path}, trovata {count} volte")
    path.write_text(updated, encoding="utf-8")


def stamp(root, tag):
    if not re.fullmatch(r"v(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*)){2}", tag):
        raise ValueError(f"tag automatico non valido: {tag}")
    version = tag[1:]
    if max(map(int, version.split("."))) > 65535:
        raise ValueError("versione fuori dai limiti dell’installer Windows")
    for folder, package in [
        (root / "core", "jarvis-core"),
        (root / "apps/desktop/src-tauri", "jarvis-desktop"),
    ]:
        manifest = folder / "server/Cargo.toml" if package == "jarvis-core" else folder / "Cargo.toml"
        prefix = rf'(^name = "{package}"\nversion = )'
        replacement = lambda match: f'{match[1]}"{version}"'
        replace_once(manifest, prefix + r'"[^"]+"', replacement)
        replace_once(folder / "Cargo.lock", prefix + r'"[^"]+"', replacement)

    config = root / "apps/desktop/src-tauri/tauri.conf.json"
    document = json.loads(config.read_text(encoding="utf-8"))
    document["version"] = version
    config.write_text(json.dumps(document, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("tag")
    stamp(Path.cwd(), parser.parse_args().tag)
