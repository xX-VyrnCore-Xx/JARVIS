"""Test del worker Python del bridge: python3 -m unittest discover -s tests/ai -v"""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "ai"))

from jarvis_ai.contracts import validation_errors  # noqa: E402
from jarvis_ai.worker import echo_handler, handle_line  # noqa: E402

FIXTURES = ROOT / "tests" / "contracts" / "fixtures" / "valid"


def _request(**payload_overrides) -> dict:
    env = json.loads((FIXTURES / "envelope.delegation-request.json").read_text(encoding="utf-8"))
    env["payload"] = json.loads((FIXTURES / "delegation.research.json").read_text(encoding="utf-8")) | payload_overrides
    return env


class WorkerReplies(unittest.TestCase):
    def assert_valid_reply(self, reply: dict) -> None:
        self.assertEqual(validation_errors("envelope", reply), ([], []))

    def test_success_is_correlated(self) -> None:
        req = _request()
        reply = handle_line(json.dumps(req), echo_handler)
        self.assert_valid_reply(reply)
        self.assertEqual(reply["status"], "succeeded")
        self.assertEqual((reply["causation_id"], reply["correlation_id"]), (req["message_id"], req["correlation_id"]))

    def test_non_json_line(self) -> None:
        reply = handle_line("non json", echo_handler)
        self.assert_valid_reply(reply)
        self.assertEqual(reply["error"]["code"], "bridge.invalid_json")

    def test_invalid_envelope(self) -> None:
        reply = handle_line(json.dumps({"contract": "jarvis.envelope/1"}), echo_handler)
        self.assert_valid_reply(reply)
        self.assertEqual(reply["error"]["code"], "bridge.invalid_envelope")

    def test_unsupported_type(self) -> None:
        req = _request()
        req["type"] = "memory.query"
        reply = handle_line(json.dumps(req), echo_handler)
        self.assert_valid_reply(reply)
        self.assertEqual(reply["error"]["code"], "bridge.unsupported_type")

    def test_invalid_delegation_semantics(self) -> None:
        reply = handle_line(json.dumps(_request(expires_at="2026-11-17T11:00:00Z")), echo_handler)
        self.assert_valid_reply(reply)
        self.assertEqual(reply["error"]["code"], "bridge.invalid_delegation")
        self.assertIn("expires_at deve seguire issued_at", reply["error"]["details"]["violations"])

    def test_unhandled_exception_reports_unknown_effect(self) -> None:
        def boom(_: dict) -> dict:
            raise RuntimeError("sintetico")

        reply = handle_line(json.dumps(_request()), boom)
        self.assert_valid_reply(reply)
        self.assertEqual((reply["error"]["code"], reply["error"]["effect"]), ("agent.unhandled", "unknown"))


if __name__ == "__main__":
    unittest.main()
