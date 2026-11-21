"""Worker di test per esercitare il ciclo delle deleghe senza un runtime di inferenza."""
import json
import os
import sys
import uuid
from datetime import datetime, timezone

calls = json.loads(os.environ.get("JARVIS_TEST_CALLS", "[]"))
last = None
backend = {"name": "offline", "model": "fixture", "location": "local"}

for line in sys.stdin:
    request = json.loads(line)
    delegation = request["payload"]
    result = {"contract": "jarvis.agent-result/1", "kind": "reply", "backend": backend, "text": "Fatto: ecco il riepilogo."}
    if request["type"] == "delegation.request":
        if log_path := os.environ.get("JARVIS_TEST_DELEGATIONS"):
            with open(log_path, "a", encoding="utf-8") as log:
                print(json.dumps(delegation), file=log)
        if delegation["inputs"].get("steps_left", 4) and delegation["tools"]:
            if calls:
                last = calls.pop(0)
            elif os.environ.get("JARVIS_TEST_REPEAT") != "1":
                last = None
            if last:
                tool, arguments = last
                result.pop("text")
                version = next((entry["version"] for entry in delegation["tools"] if entry["name"] == tool), "1.0.0")
                result.update(kind="tool_call", tool_call={"tool": tool, "version": version, "arguments": arguments})
    envelope = {
        "contract": "jarvis.envelope/1", "message_id": str(uuid.uuid4()),
        "causation_id": request["message_id"], "correlation_id": request["correlation_id"],
        "task_id": request["task_id"], "sender": {"kind": "module", "id": "ai-worker"},
        "recipient": request["sender"], "type": "delegation.result", "status": "succeeded",
        "issued_at": datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z"), "payload": result,
    }
    if os.environ.get("JARVIS_TEST_STREAM") and delegation.get("stream"):
        for delta in ("Ciao, ", "sono ", "JARVIS."):
            print(json.dumps(dict(envelope, message_id=str(uuid.uuid4()), status="running",
                                  type="delegation.progress", payload={"delta": delta})), flush=True)
        result.update(text="Ciao, sono JARVIS.", usage={"prompt_tokens": 40, "output_tokens": 3})
    print(json.dumps(envelope), flush=True)
