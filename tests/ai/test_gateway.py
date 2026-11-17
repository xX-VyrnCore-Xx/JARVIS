"""Test dell'AI Gateway: modello offline, adapter Ollama contro un server HTTP sintetico, degradazione.

python3 -m unittest discover -s tests/ai -v
"""

from __future__ import annotations

import json
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "ai"))

from jarvis_ai.contracts import validation_errors  # noqa: E402
from jarvis_ai.gateway import Gateway, load_catalog  # noqa: E402

FIXTURES = ROOT / "tests" / "contracts" / "fixtures" / "valid"


def delegation(message: str, **inputs) -> dict:
    d = json.loads((FIXTURES / "delegation.research.json").read_text(encoding="utf-8"))
    d["tools"] = [{"name": n, "version": "1.0.0"} for n in load_catalog()]
    d["inputs"] = {"messages": [{"role": "user", "content": message}], "memory": [], **inputs}
    assert validation_errors("delegation", d) == ([], [])
    return d


class FakeOllama(BaseHTTPRequestHandler):
    """Imita /api/tags e /api/chat di Ollama; registra le richieste ricevute."""

    models = ["qwen2.5:3b"]
    requests: list[dict] = []
    mode = "tool"

    def log_message(self, *args):  # silenzia il server
        pass

    def _json(self, body: dict, status: int = 200):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/api/tags":
            self._json({"models": [{"name": m} for m in FakeOllama.models]})
        else:
            self._json({}, 404)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        FakeOllama.requests.append(body)
        if FakeOllama.mode == "error":
            self._json({"error": "model crashed"}, 500)
        elif "tools" in body and FakeOllama.mode == "tool":
            self._json({"message": {"role": "assistant", "content": "", "tool_calls": [
                {"function": {"name": "fs__read", "arguments": {"path": "note.txt"}}}]}})
        else:
            self._json({"message": {"role": "assistant", "content": "Risposta sintetica del modello."}})


class OllamaAdapter(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = HTTPServer(("127.0.0.1", 0), FakeOllama)
        cls.url = f"http://127.0.0.1:{cls.server.server_port}"
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()

    def setUp(self):
        FakeOllama.requests.clear()
        FakeOllama.mode = "tool"
        FakeOllama.models = ["qwen2.5:3b"]

    def gateway(self, **env) -> Gateway:
        return Gateway({"JARVIS_AI_BACKEND": "auto", "JARVIS_OLLAMA_URL": self.url, **env})

    def test_auto_selects_ollama_and_maps_tool_calls(self):
        g = self.gateway()
        self.assertEqual(g.status()["backend"]["name"], "ollama")
        result = g.respond(delegation("leggi le note"))
        self.assertEqual(validation_errors("agent-result", result), ([], []))
        self.assertEqual(result["tool_call"]["tool"], "fs.read")
        sent = FakeOllama.requests[0]
        self.assertEqual(sent["model"], "qwen2.5:3b")
        self.assertIn("fs__write", {t["function"]["name"] for t in sent["tools"]})
        self.assertEqual(sent["messages"][0]["role"], "system")

    def test_tool_result_round_returns_reply_without_tools(self):
        result = self.gateway().respond(delegation("leggi le note", tool_result={"tool": "fs.read", "status": "succeeded", "output": {"content": "x"}}))
        self.assertEqual(result["kind"], "reply")
        self.assertNotIn("tools", FakeOllama.requests[-1])
        self.assertEqual(FakeOllama.requests[-1]["messages"][-2]["role"], "tool")

    def test_only_delegated_tools_are_offered(self):
        d = delegation("ciao")
        d["tools"] = [{"name": "time.now", "version": "1.0.0"}]
        self.gateway().respond(d)
        self.assertEqual([t["function"]["name"] for t in FakeOllama.requests[0]["tools"]], ["time__now"])

    def test_tool_not_delegated_is_ignored(self):
        d = delegation("leggi")
        d["tools"] = [{"name": "time.now", "version": "1.0.0"}]
        result = self.gateway().respond(d)
        self.assertEqual(result["kind"], "reply", "fs.read non era nella delega")

    def test_runtime_failure_degrades_to_offline(self):
        g = self.gateway()
        FakeOllama.mode = "error"
        result = g.respond(delegation("che ore sono"))
        self.assertEqual(result["backend"]["name"], "offline")
        self.assertIn("Ollama non disponibile", result["backend"]["degraded_reason"])
        self.assertEqual(result["tool_call"]["tool"], "time.now")

    def test_missing_preferred_model_falls_back_with_reason(self):
        g = self.gateway(JARVIS_OLLAMA_MODEL="llama3.1")
        self.assertEqual(g.status()["backend"]["name"], "offline")
        self.assertIn("llama3.1", g.status()["reason"])

    def test_no_models_installed(self):
        FakeOllama.models = []
        self.assertIn("ollama pull", self.gateway().status()["reason"])


class OfflineBackend(unittest.TestCase):
    def setUp(self):
        self.g = Gateway({"JARVIS_AI_BACKEND": "offline"})

    def call(self, message: str, **inputs) -> dict:
        result = self.g.respond(delegation(message, **inputs))
        self.assertEqual(validation_errors("agent-result", result), ([], []))
        return result

    def test_intents(self):
        cases = {
            "che ore sono": ("time.now", {}),
            "info sistema": ("system.info", {}),
            "elenca file": ("fs.list", {"path": "."}),
            "leggi docs/a.md": ("fs.read", {"path": "docs/a.md"}),
            "scrivi a.txt: riga uno": ("fs.write", {"path": "a.txt", "content": "riga uno"}),
            "ricorda che uso Linux": ("memory.remember", {"text": "uso Linux"}),
            "cosa ricordi": ("memory.list", {}),
            "dimentica 1a2b3c4d": ("memory.forget", {"id": "1a2b3c4d"}),
        }
        for message, (tool, args) in cases.items():
            with self.subTest(message=message):
                r = self.call(message)
                self.assertEqual((r["tool_call"]["tool"], r["tool_call"]["arguments"]), (tool, args))

    def test_offline_mode_has_no_degraded_reason(self):
        self.assertNotIn("degraded_reason", self.g.status()["backend"])

    def test_tool_results_are_described(self):
        r = self.call("x", tool_result={"tool": "fs.write", "status": "denied", "output": {"message": "il percorso è fuori dalla workspace"}})
        self.assertIn("negato", r["text"])
        r = self.call("x", tool_result={"tool": "memory.list", "status": "succeeded", "output": {"items": [{"id": "a", "text": "t"}]}})
        self.assertIn("[a] t", r["text"])

    def test_memory_recall_and_help(self):
        self.assertIn("caffè", self.call("caffè?", memory=["amo il caffè"])["text"])
        self.assertIn("Prova con", self.call("qualcosa di incomprensibile")["text"])


if __name__ == "__main__":
    unittest.main()
