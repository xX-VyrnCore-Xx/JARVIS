"""Il diagramma dell'architettura (docs/ARCHITECTURE.md) deve restare fedele al codice.

Controlla che: ogni modulo del Core, del runtime AI e ogni client compaiano nella «Mappa del repository»; i file
citati con estensione esistano davvero; il diagramma non nomini provider rimossi; ogni blocco Mermaid abbia
sottografi bilanciati e frecce fra nodi dichiarati.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOC = (ROOT / "docs" / "ARCHITECTURE.md").read_text(encoding="utf-8")


def mermaid_blocks() -> list[str]:
    return re.findall(r"```mermaid\n(.*?)```", DOC, flags=re.S)


def repo_map() -> str:
    """Il solo blocco Mermaid della «Mappa del repository» (non la prosa che lo precede)."""
    start = DOC.index("### Mappa del repository")
    end = DOC.index("#### Da una frase a un risultato")
    return re.search(r"```mermaid\n(.*?)```", DOC[start:end], flags=re.S).group(1)


class ArchitectureDiagram(unittest.TestCase):
    def test_every_core_module_appears_in_the_map(self):
        src = ROOT / "core" / "server" / "src"
        stems = sorted(p.stem for p in src.iterdir() if (p.suffix == ".rs" or p.is_dir()) and p.stem not in {"lib", "main"})
        text = repo_map()
        missing = [s for s in stems if not re.search(rf"(?<![A-Za-z0-9_]){re.escape(s)}(?![A-Za-z0-9_])", text)]
        self.assertEqual(missing, [], f"moduli del Core assenti dal diagramma: {missing}")

    def test_ai_runtime_clients_and_pipelines_are_present(self):
        text = repo_map()
        for py in sorted(p.name for p in (ROOT / "ai" / "jarvis_ai").glob("*.py") if p.name != "__init__.py"):
            self.assertIn(py, text, f"{py} non compare nel diagramma")
        for client in ("macOS", "egui", "Android", "Web"):
            self.assertIn(client, text)
        for workflow in ("contracts.yml", "release.yml", "release-after-ci.yml"):
            self.assertTrue((ROOT / ".github" / "workflows" / workflow).is_file(), workflow)
            self.assertIn(workflow, text)

    def test_removed_providers_do_not_come_back(self):
        self.assertNotIn("Ollama", repo_map(), "Ollama Cloud è stato rimosso dal prodotto")
        self.assertIn("NVIDIA NIM", repo_map())

    def test_cited_files_exist(self):
        text = repo_map()
        for name in sorted(set(re.findall(r"\b([a-z_]+\.py)\b", text))):
            self.assertTrue((ROOT / "ai" / "jarvis_ai" / name).is_file(), f"{name} non esiste")
        for name in sorted(set(re.findall(r"\b([a-z_\-]+\.ya?ml)\b", text))):
            self.assertTrue((ROOT / ".github" / "workflows" / name).is_file(), f"{name} non esiste")

    def test_mermaid_blocks_are_well_formed(self):
        blocks = mermaid_blocks()
        self.assertGreaterEqual(len(blocks), 5)
        for block in blocks:
            if "flowchart" not in block:
                continue
            opens = len(re.findall(r"^\s*subgraph\b", block, flags=re.M))
            closes = len(re.findall(r"^\s*end\s*$", block, flags=re.M))
            self.assertEqual(opens, closes, "sottografi non bilanciati")
            declared = set(re.findall(r"(?<![A-Za-z0-9_])([A-Z][A-Za-z0-9_]*)\s*[\[\(\{]", block))
            subgraphs = set(re.findall(r"^\s*subgraph\s+([A-Za-z0-9_]+)", block, flags=re.M))
            used = set()
            for left, right in re.findall(r"([A-Z][A-Za-z0-9_]*)\s*(?:-->|-\.\s*\w+\s*\.->|-\s*\"[^\"]*\"\s*-->|--\s[^>]*-->)\s*([A-Z][A-Za-z0-9_]*)", block):
                used.update((left, right))
            unknown = used - declared - subgraphs
            self.assertEqual(unknown, set(), f"nodi usati ma non dichiarati: {sorted(unknown)}")


if __name__ == "__main__":
    unittest.main()
