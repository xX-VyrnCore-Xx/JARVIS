"""Verifica dei contratti v1 con fixture sintetiche.

Esecuzione dalla radice del repository:
    python3 -m unittest discover -s tests/contracts -v

Convenzione delle fixture: ``<schema>.<caso>.json``; il prefisso indica lo schema
in ``contracts/v1/<schema>.schema.json``. Le fixture ``*.semantic.json`` in
``invalid/`` devono superare lo schema e fallire gli invarianti semantici, così
il test dimostra che la regola semantica è quella che le respinge.
"""

from __future__ import annotations

import hashlib
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
TRANSITIONS = _load(SCHEMA_DIR / "task-transitions.json")
GENESIS_HASH = "0" * 64


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
    if schema_name == "task":
        history = doc["history"]
        first = history[0]
        if "from" in first or first["to"] != TRANSITIONS["initial"]:
            errors.append("la history deve iniziare con lo stato iniziale senza 'from'")
        previous_at = _ts(doc["created_at"])
        for prev, step in zip(history, history[1:]):
            if step.get("from") != prev["to"]:
                errors.append(f"'from' {step.get('from')} non segue {prev['to']}")
            elif step["to"] not in TRANSITIONS["transitions"][prev["to"]]:
                errors.append(f"transizione non ammessa {prev['to']} -> {step['to']}")
        for step in history:
            if _ts(step["at"]) < previous_at:
                errors.append("istanti della history non monotoni")
            previous_at = _ts(step["at"])
        if history[-1]["to"] != doc["state"]:
            errors.append("state diverso dall'ultima transizione")
    if schema_name == "policy-decision":
        decided, expires = _ts(doc["decided_at"]), _ts(doc["expires_at"])
        if expires <= decided:
            errors.append("expires_at deve seguire decided_at")
        conf = doc.get("confirmation")
        if conf:
            if conf["parameters_digest"] != doc["request"]["parameters_digest"]:
                errors.append("la conferma è legata a parametri diversi")
            if not decided < _ts(conf["expires_at"]) <= expires:
                errors.append("la conferma deve scadere entro la decisione")
    if schema_name == "envelope":
        if doc["sender"] == doc["recipient"]:
            errors.append("sender e recipient coincidono")
        if doc.get("causation_id") == doc["message_id"]:
            errors.append("causation_id non può riferirsi al messaggio stesso")
    return errors


def audit_hash(event: dict) -> str:
    """SHA-256 del JSON canonico (chiavi ordinate, nessuno spazio, UTF-8) senza il campo 'hash'."""
    body = {k: v for k, v in event.items() if k != "hash"}
    canonical = json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def audit_chain_errors(events: list[dict]) -> list[str]:
    errors: list[str] = []
    prev_hash = GENESIS_HASH
    for index, event in enumerate(events):
        if event["sequence"] != index:
            errors.append(f"sequenza interrotta all'evento {index}")
        if event["prev_hash"] != prev_hash:
            errors.append(f"prev_hash errato all'evento {index}")
        if audit_hash(event) != event["hash"]:
            errors.append(f"hash non corrispondente all'evento {index}")
        prev_hash = event["hash"]
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
