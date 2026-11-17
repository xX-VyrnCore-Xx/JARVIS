"""Worker Python del prototipo di bridge Core–AI (Fase 2).

Protocollo: una riga JSON per messaggio su stdin/stdout, ogni messaggio è un
``jarvis.envelope/1``. Il Core invia ``delegation.request`` con una
``jarvis.delegation/1`` nel payload; il worker risponde con
``delegation.result`` (``succeeded`` o ``failed`` con ``error``). Il canale
stderr è riservato alla diagnostica minimizzata. Questo trasporto è una prova
tecnica: la scelta RPC definitiva resta aperta (DECISIONS.md).

Avvio dalla directory ``ai/``: ``python3 -m jarvis_ai.worker [--handler NOME]``.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import uuid
from datetime import datetime, timezone
from typing import Callable

from jarvis_ai.contracts import validation_errors

WORKER = {"kind": "module", "id": "ai-worker"}


class HandlerError(Exception):
    def __init__(self, code: str, category: str, message: str, retryable: bool, effect: str = "not_applied"):
        super().__init__(message)
        self.error = {"code": code, "category": category, "message": message, "retryable": retryable, "effect": effect}


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def echo_handler(delegation: dict) -> dict:
    """Agente sintetico deterministico: restituisce obiettivo e strumenti ricevuti."""
    return {
        "delegation_id": delegation["delegation_id"],
        "agent": delegation["delegate"]["agent"],
        "objective": delegation["objective"],
        "tools": [t["name"] for t in delegation["tools"]],
    }


def synthetic_handler(delegation: dict) -> dict:
    """Comportamenti di guasto per i test del bridge, selezionati dall'obiettivo. Non usare fuori dai test."""
    objective = delegation["objective"]
    if objective.startswith("sleep:"):
        time.sleep(int(objective.split(":", 1)[1]) / 1000)
    elif objective == "crash":
        sys.exit(3)
    elif objective == "fail":
        raise HandlerError("agent.synthetic_failure", "internal", "Errore sintetico dell'agente", retryable=False)
    elif objective == "malformed":
        sys.stdout.write('{"contract":"jarvis.envelope/1"}\n')
        sys.stdout.flush()
        return {}
    return echo_handler(delegation)


_GATEWAY = None
_VOICE = None


def voice_service():
    global _VOICE
    if _VOICE is None:
        from jarvis_ai.voice import VoiceService

        _VOICE = VoiceService()
    return _VOICE


def assistant_handler(delegation: dict) -> dict:
    """Agente General Assistant: delega all'AI Gateway e valida l'esito (jarvis.agent-result/1)."""
    global _GATEWAY
    if _GATEWAY is None:
        from jarvis_ai.gateway import Gateway

        _GATEWAY = Gateway()
    result = _GATEWAY.respond(delegation)
    schema, semantic = validation_errors("agent-result", result)
    if schema or semantic:
        raise HandlerError("agent.invalid_result", "internal", "Esito dell'agente non conforme", retryable=False)
    return result


def assistant_status() -> dict:
    global _GATEWAY
    if _GATEWAY is None:
        from jarvis_ai.gateway import Gateway

        _GATEWAY = Gateway()
    return _GATEWAY.status()


HANDLERS: dict[str, Callable[[dict], dict]] = {"echo": echo_handler, "synthetic": synthetic_handler, "assistant": assistant_handler}


def _reply(request: dict | None, status: str, payload: dict, error: dict | None = None) -> dict:
    env = {
        "contract": "jarvis.envelope/1",
        "message_id": str(uuid.uuid4()),
        "correlation_id": request["correlation_id"] if request else str(uuid.uuid4()),
        "task_id": request["task_id"] if request else str(uuid.uuid4()),
        "sender": WORKER,
        "recipient": request["sender"] if request else {"kind": "module", "id": "core"},
        "type": "delegation.result",
        "status": status,
        "issued_at": _now(),
        "payload": payload,
    }
    if request:
        env["causation_id"] = request["message_id"]
    if error:
        env["error"] = error
    return env


def handle_line(line: str, handler: Callable[[dict], dict]) -> dict:
    try:
        request = json.loads(line)
    except json.JSONDecodeError:
        return _reply(None, "failed", {}, HandlerError("bridge.invalid_json", "validation", "Riga non JSON", False).error)
    schema, semantic = validation_errors("envelope", request) if isinstance(request, dict) else (["non è un oggetto"], [])
    if schema or semantic:
        err = HandlerError("bridge.invalid_envelope", "validation", "Envelope non conforme", False).error
        err["details"] = {"violations": (schema + semantic)[:10]}
        return _reply(None, "failed", {}, err)
    if request["type"] in ("voice.transcribe", "voice.synthesize"):
        from jarvis_ai.contracts import validation_errors as _check
        from jarvis_ai.voice import VoiceError

        payload = request["payload"]
        try:
            if request["type"] == "voice.transcribe":
                result = voice_service().transcribe(str(payload.get("audio_b64", "")), "wake_word" if payload.get("mode") == "wake_word" else "push_to_talk")
                contract = "voice-transcription"
            else:
                result = voice_service().synthesize(str(payload.get("text", "")))
                contract = "voice-synthesis"
        except VoiceError as exc:
            err = HandlerError(exc.code, "unavailable" if exc.code == "voice.unavailable" else "validation", str(exc), retryable=False).error
            reply = _reply(request, "failed", {}, err)
            reply["type"] = "voice.result"
            return reply
        schema, semantic = _check(contract, result)
        if schema or semantic:
            reply = _reply(request, "failed", {}, HandlerError("voice.invalid_result", "internal", "Esito vocale non conforme", False).error)
        else:
            reply = _reply(request, "succeeded", result)
        reply["type"] = "voice.result"
        return reply
    if request["type"] == "health.request":
        status = assistant_status() if handler is assistant_handler else {"handler": handler.__name__}
        if handler is assistant_handler:
            status["voice"] = voice_service().status()
        reply = _reply(request, "succeeded", status)
        reply["type"] = "health.result"
        return reply
    if request["type"] != "delegation.request":
        return _reply(request, "failed", {}, HandlerError("bridge.unsupported_type", "validation", "Tipo non supportato", False).error)
    delegation = request["payload"]
    schema, semantic = validation_errors("delegation", delegation)
    if schema or semantic:
        err = HandlerError("bridge.invalid_delegation", "validation", "Delega non conforme", False).error
        err["details"] = {"violations": (schema + semantic)[:10]}
        return _reply(request, "failed", {}, err)
    try:
        return _reply(request, "succeeded", handler(delegation))
    except HandlerError as exc:
        return _reply(request, "failed", {}, exc.error)
    except Exception:  # noqa: BLE001 — confine di processo: nessuna eccezione deve chiudere il canale
        err = HandlerError("agent.unhandled", "internal", "Errore interno dell'agente", False, effect="unknown").error
        return _reply(request, "failed", {}, err)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handler", choices=sorted(HANDLERS), default="echo")
    args = parser.parse_args(argv)
    handler = HANDLERS[args.handler]
    # Protocollo in UTF-8 con righe '\n' su ogni sistema: su Windows le pipe userebbero cp1252 e '\r\n'.
    for stream in (sys.stdin, sys.stdout):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", newline="\n" if stream is sys.stdout else None)
    for line in sys.stdin:
        if not line.strip():
            continue
        sys.stdout.write(json.dumps(handle_line(line, handler), ensure_ascii=False) + "\n")
        sys.stdout.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main())
