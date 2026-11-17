"""Verifica dei contratti v1 con fixture sintetiche.

Esecuzione dalla radice del repository:
    python3 -m unittest discover -s tests/contracts -v

Convenzione delle fixture: ``<schema>.<caso>.json``; il prefisso indica lo schema
in ``contracts/v1/<schema>.schema.json``. Le fixture ``*.semantic.json`` in
``invalid/`` devono superare lo schema e fallire gli invarianti semantici, così
il test dimostra che la regola semantica è quella che le respinge.
"""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "ai"))

from jarvis_ai.contracts import (  # noqa: E402
    SCHEMA_DIR,
    TRANSITIONS,
    _load,
    audit_chain_errors,
    semantic_errors,
    validator_for,
)

FIXTURE_DIR = Path(__file__).resolve().parent / "fixtures"


def _fixtures(kind: str) -> list[Path]:
    return sorted((FIXTURE_DIR / kind).glob("*.json"))


class SchemaWellFormed(unittest.TestCase):
    def test_schemas_are_valid_draft_2020_12(self) -> None:
        paths = sorted(SCHEMA_DIR.glob("*.schema.json"))
        self.assertGreaterEqual(len(paths), 5)
        for path in paths:
            with self.subTest(schema=path.name):
                schema = _load(path)
                Draft202012Validator.check_schema(schema)
                self.assertTrue(schema["$id"].endswith(f"/v1/{path.name}"))


class Fixtures(unittest.TestCase):
    def test_every_schema_has_valid_and_invalid_fixtures(self) -> None:
        names = {p.name.removesuffix(".schema.json") for p in SCHEMA_DIR.glob("*.schema.json")} - {"common"}
        for kind in ("valid", "invalid"):
            covered = {p.name.split(".")[0] for p in _fixtures(kind)}
            self.assertEqual(names - covered, set(), f"schemi senza fixture {kind}")

    def test_valid_fixtures_pass(self) -> None:
        for path in _fixtures("valid"):
            name = path.name.split(".")[0]
            with self.subTest(fixture=path.name):
                doc = _load(path)
                errs = [e.message for e in validator_for(name).iter_errors(doc)]
                self.assertEqual(errs, [])
                self.assertEqual(semantic_errors(name, doc), [])

    def test_invalid_fixtures_are_rejected(self) -> None:
        for path in _fixtures("invalid"):
            name = path.name.split(".")[0]
            with self.subTest(fixture=path.name):
                doc = _load(path)
                schema_ok = validator_for(name).is_valid(doc)
                if path.name.endswith(".semantic.json"):
                    self.assertTrue(schema_ok, "la fixture semantica deve superare lo schema")
                    self.assertNotEqual(semantic_errors(name, doc), [])
                else:
                    self.assertFalse(schema_ok, "la fixture doveva essere respinta dallo schema")


class AuditChain(unittest.TestCase):
    def _stream(self, name: str) -> list[dict]:
        path = FIXTURE_DIR / "streams" / name
        events = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]
        for event in events:
            validator_for("audit-event").validate(event)
        return events

    def test_valid_chain(self) -> None:
        self.assertEqual(audit_chain_errors(self._stream("audit-event.valid.jsonl")), [])

    def test_tampered_event_is_detected(self) -> None:
        self.assertIn("hash non corrispondente all'evento 1", audit_chain_errors(self._stream("audit-event.tampered.jsonl")))

    def test_deleted_event_is_detected(self) -> None:
        errors = audit_chain_errors(self._stream("audit-event.deleted-event.jsonl"))
        self.assertIn("prev_hash errato all'evento 1", errors)


class TaskTransitions(unittest.TestCase):
    def test_table_is_consistent(self) -> None:
        table = TRANSITIONS["transitions"]
        states = set(_load(SCHEMA_DIR / "task.schema.json")["$defs"]["state"]["enum"])
        self.assertEqual(set(table), states)
        for state in TRANSITIONS["terminal"]:
            self.assertEqual(table[state], [])
        for targets in table.values():
            self.assertLessEqual(set(targets), states)


if __name__ == "__main__":
    unittest.main()
