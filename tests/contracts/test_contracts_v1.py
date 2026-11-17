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
import unittest
from datetime import datetime, timezone
from pathlib import Path

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[2]
SCHEMA_DIR = ROOT / "contracts" / "v1"
FIXTURE_DIR = Path(__file__).resolve().parent / "fixtures"


def _load(path: Path) -> dict:
    with path.open(encoding="utf-8") as fh:
        return json.load(fh)


def _registry() -> Registry:
    resources = []
    for path in SCHEMA_DIR.glob("*.schema.json"):
        schema = _load(path)
        resource = Resource.from_contents(schema)
        resources.append((schema["$id"], resource))
    return Registry().with_resources(resources)


REGISTRY = _registry()


def validator_for(schema_name: str) -> Draft202012Validator:
    schema = _load(SCHEMA_DIR / f"{schema_name}.schema.json")
    return Draft202012Validator(schema, registry=REGISTRY)


def _ts(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(timezone.utc)


def semantic_errors(schema_name: str, doc: dict) -> list[str]:
    """Invarianti non esprimibili in JSON Schema. Documentati in contracts/README.md."""
    errors: list[str] = []
    if schema_name == "delegation":
        issued, expires = _ts(doc["issued_at"]), _ts(doc["expires_at"])
        lifetime_ms = (expires - issued).total_seconds() * 1000
        if lifetime_ms <= 0:
            errors.append("expires_at deve seguire issued_at")
        elif doc["timeout_ms"] > lifetime_ms:
            errors.append("timeout_ms supera la durata della delega")
    if schema_name == "envelope":
        if doc["sender"] == doc["recipient"]:
            errors.append("sender e recipient coincidono")
        if doc.get("causation_id") == doc["message_id"]:
            errors.append("causation_id non può riferirsi al messaggio stesso")
    return errors


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


if __name__ == "__main__":
    unittest.main()
