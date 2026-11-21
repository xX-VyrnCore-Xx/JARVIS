"""Misura le latenze di un Core avviato e le confronta con gli obiettivi di tests/perf/slo.json.

Uso: JARVIS_TOKEN=<token> python tests/perf/latency.py http://127.0.0.1:8765 [richieste_per_scenario]
Esce con codice 1 se un p95 supera l'obiettivo. Solo libreria standard.
"""

from __future__ import annotations

import json
import os
import sys
import time
import urllib.request
from pathlib import Path


def main() -> int:
    base = sys.argv[1].rstrip("/")
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 30
    port = base.rsplit(":", 1)[1]
    host = {"Host": f"localhost:{port}"}
    token = os.environ.get("JARVIS_TOKEN", "")
    if len(token) < 32 or not token.isalnum():
        print("JARVIS_TOKEN deve contenere almeno 32 caratteri alfanumerici.", file=sys.stderr)
        return 2
    slo = json.loads((Path(__file__).parent / "slo.json").read_text(encoding="utf-8"))
    failed = []
    print(f"{'scenario':<12} {'n':>4} {'p50':>9} {'p95':>9} {'max':>9} {'obiettivo p95':>14}")
    for name, s in slo["scenarios"].items():
        samples = []
        for _ in range(n):
            data = json.dumps(s["body"]).encode() if "body" in s else None
            req = urllib.request.Request(f"{base}{s['path']}", data=data, method=s["method"],
                                         headers={**host, "X-Jarvis-Token": token, "Content-Type": "application/json"})
            t = time.perf_counter()
            with urllib.request.urlopen(req, timeout=60) as r:
                r.read()
                assert r.status == 200, f"{name}: HTTP {r.status}"
            samples.append((time.perf_counter() - t) * 1000)
        samples.sort()
        p = lambda q: samples[min(len(samples) - 1, max(0, int(round(q * len(samples) + 0.5)) - 1))]  # noqa: E731
        ok = p(0.95) <= s["p95_ms"]
        failed += [] if ok else [name]
        print(f"{name:<12} {n:>4} {p(0.5):>7.1f}ms {p(0.95):>7.1f}ms {samples[-1]:>7.1f}ms {s['p95_ms']:>11} ms {'✓' if ok else '✗'}")
    if failed:
        print("Obiettivi superati:", ", ".join(failed))
        return 1
    print("✓ tutti gli obiettivi rispettati")
    return 0


if __name__ == "__main__":
    sys.exit(main())
