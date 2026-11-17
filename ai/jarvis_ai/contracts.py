"""Validazione dei contratti v1 per il lato Python (AI e agenti).

Implementazione di riferimento degli invarianti semantici descritti in
``contracts/README.md``; il Core Rust li replica in ``core/contracts``.
La directory degli schemi è ``contracts/v1`` del monorepo, sovrascrivibile
con la variabile d'ambiente ``JARVIS_CONTRACTS_DIR``.
"""

from __future__ import annotations

import functools
import hashlib
import json
import os
from datetime import datetime, timezone
from pathlib import Path

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

SCHEMA_DIR = Path(os.environ.get("JARVIS_CONTRACTS_DIR", Path(__file__).resolve().parents[2] / "contracts" / "v1"))


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


@functools.cache
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
    if schema_name == "workflow-run":
        if _ts(doc["finished_at"]) < _ts(doc["started_at"]):
            errors.append("finished_at precede started_at")
        if [step["index"] for step in doc["steps"]] != list(range(len(doc["steps"]))):
            errors.append("indici dei passi non consecutivi da 0")
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


def validation_errors(schema_name: str, doc: dict) -> tuple[list[str], list[str]]:
    """Restituisce (errori di schema, violazioni semantiche); la semantica si controlla solo su documenti validi."""
    schema = [f"{'/'.join(map(str, e.absolute_path))}: {e.message}" for e in validator_for(schema_name).iter_errors(doc)]
    return schema, ([] if schema else semantic_errors(schema_name, doc))
