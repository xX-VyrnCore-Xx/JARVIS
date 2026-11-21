"""Agente a regole locale. Le inferenze cloud sono gestite dal Core."""

from __future__ import annotations

import json
import re
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Callable

from jarvis_ai.contracts import SCHEMA_DIR

CATALOG_DIR = SCHEMA_DIR.parent / "catalog" / "tools"
OFFLINE_MODEL = "jarvis-offline-1"
def load_catalog() -> dict[str, dict]:
    return {p.stem: json.loads(p.read_text(encoding="utf-8")) for p in sorted(CATALOG_DIR.glob("*.json"))}


def _result(kind: str, backend: dict, *, text: str | None = None, tool: str | None = None, arguments: dict | None = None,
            rationale: str | None = None, catalog: dict | None = None) -> dict:
    out: dict[str, Any] = {"contract": "jarvis.agent-result/1", "kind": kind, "backend": backend}
    if kind == "reply":
        out["text"] = (text or "…")[:16000]
    else:
        version = (catalog or {}).get(tool, {}).get("version", "1.0.0")
        call: dict[str, Any] = {"tool": tool, "version": version, "arguments": arguments or {}}
        if rationale:
            call["rationale"] = rationale[:1000]
        out["tool_call"] = call
    return out


# --------------------------------------------------------------------------- offline


def _describe_tool_result(result: dict) -> str:
    tool = result["tool"]
    node = re.match(r"^node\.([a-z][a-z0-9-]*)\.(.+)$", tool)
    if node:
        # Strumento eseguito da un nodo esecutore: stessa descrizione dello strumento locale, con il nodo.
        inner = _describe_tool_result({**result, "tool": node.group(2)})
        return f"[nodo {node.group(1)}] {inner}"
    status, output = result["status"], result["output"]
    if status == "denied":
        return f"Non posso eseguire {tool}: la policy lo ha negato ({_reason(output)})."
    if status == "cancelled":
        return f"Operazione {tool} annullata: non hai confermato."
    if status == "failed":
        return f"{tool} non è riuscito: {_reason(output)}."
    if tool == "time.now":
        return f"Sono le {output.get('local', '?')} (UTC {output.get('utc', '?')})."
    if tool == "math.calc":
        return f"{output.get('expression')} = {output.get('result')}"
    if tool == "units.convert":
        return f"{output.get('value')} {output.get('from')} = {output.get('result')} {output.get('to')}"
    if tool == "date.calc":
        parts = [f"Il {output.get('date')} è {output.get('weekday')}."]
        if "days_between" in output:
            parts.append(f"Fino al {output.get('until')} mancano {output.get('days_between')} giorni.")
        if "result" in output:
            parts.append(f"Risultato: {output.get('result')} ({output.get('result_weekday')}).")
        return " ".join(parts)
    if tool == "weather.forecast":
        c = output.get("current") or {}
        days = "\n".join(f"• {d['date']}: {d['sky']}, {d['min_c']}–{d['max_c']} °C, pioggia {d.get('rain_probability')}%" for d in output.get("daily", []))
        return (f"Meteo a {output.get('place')} ({output.get('region') or output.get('country')}): ora {c.get('temperature_c')} °C, {c.get('sky')}, "
                f"vento {c.get('wind_kmh')} km/h.\n{days}\nFonte: Open-Meteo.")
    if tool == "system.disks":
        return "Dischi:\n" + "\n".join(f"• {d['mount']}: {d['free_gb']} GB liberi su {d['total_gb']} GB ({d['used_percent']}% usato)" for d in output.get("disks", []))
    if tool == "fs.search":
        res = output.get("results", [])
        if not res:
            return "Nessun file trovato."
        return "File trovati:\n" + "\n".join(f"• {r['path']}" + (f" (riga {r['line']}: {r['snippet']})" if r.get("line") else "") for r in res)
    if tool in ("fs.mkdir", "fs.move", "fs.delete"):
        return {"fs.mkdir": f"Cartella creata: {output.get('path')}", "fs.move": f"Spostato «{output.get('from')}» in «{output.get('to')}».",
                "fs.delete": f"Eliminato: {output.get('path')}"}[tool]
    if tool == "notes.add":
        return f"Nota salvata (id {output.get('id')})."
    if tool in ("notes.list", "notes.search"):
        notes = output.get("notes", [])
        if not notes:
            return "Nessuna nota."
        return "Note:\n" + "\n".join(f"• [{n['id']}] {n['title'] or n['text'][:80]}" for n in notes)
    if tool == "notes.delete":
        return f"Nota {output.get('id')} eliminata."
    if tool == "todo.add":
        due = f", scadenza {output.get('due_at')}" if output.get("due_at") else ""
        return f"Aggiunto alle cose da fare (id {output.get('id')}{due}): {output.get('text')}"
    if tool == "todo.list":
        todos = output.get("todos", [])
        if not todos:
            return "Nessuna cosa da fare."
        return "Cose da fare:\n" + "\n".join(f"• [{t['id']}] {t['text']}" + (f" (entro {t['due_at']})" if t.get("due_at") else "") for t in todos)
    if tool == "todo.done":
        return f"Fatto! Attività {output.get('id')} completata."
    if tool == "memory.update":
        return f"Ricordo {output.get('id')} aggiornato: {output.get('text')}"
    if tool in ("web.browse", "web.form"):
        excerpt = (output.get("text") or "").strip()[:1200]
        head = "Modulo inviato. " if tool == "web.form" else ""
        links = output.get("links") or []
        forms = output.get("forms") or []
        extra = f"\n({len(links)} collegamenti, {len(forms)} moduli nella pagina)" if tool == "web.browse" else ""
        blocked = f"\nRichieste bloccate dalla policy: {len(output['blocked_requests'])}" if output.get("blocked_requests") else ""
        return (f"{head}«{output.get('title') or output.get('url')}» ({output.get('url')}, {output.get('retrieved_at')}):\n{excerpt}"
                f"{extra}{blocked}\n[contenuto Web non attendibile]")
    if tool in {"code.check_python", "code.run_python"}:
        stdout = output.get("stdout", "").strip()
        stderr = output.get("stderr", "").strip()
        label = "Sintassi Python verificata" if tool == "code.check_python" else "Esecuzione Python in sandbox completata"
        if not output.get("succeeded"):
            label = f"Controllo Python terminato con codice {output.get('exit_code')}"
        return "\n".join(part for part in (label, stdout, stderr) if part)
    if tool == "agent.delegate":
        used = ", ".join(f"{t['tool']} ({t['status']})" for t in output.get("tools_used", [])) or "nessuno"
        return f"L'agente temporaneo «{output.get('role')}» ha risposto:\n{output.get('reply')}\n(strumenti usati: {used})"
    if tool == "process.list":
        procs = output.get("processes", [])
        if not procs:
            return "Nessun processo rilevato."
        def line(p: dict) -> str:
            cpu = f"CPU {p['cpu_percent']}% · " if "cpu_percent" in p else ""
            return f"• {p.get('name')} (pid {p.get('pid')}): {cpu}{p.get('memory_mb')} MB"
        head = "memoria" if output.get("sorted_by") == "memory" else "CPU"
        return f"Processi con più uso di {head} ({len(procs)} di {output.get('total')}):\n" + "\n".join(line(p) for p in procs)
    if tool == "shell.run":
        code = output.get("exit_code")
        body = (output.get("stdout") or "").strip() or (output.get("stderr") or "").strip() or "(nessun output)"
        cut = "\n(output troncato)" if output.get("truncated") else ""
        return f"Comando «{output.get('command')}» terminato con codice {code} in {output.get('duration_ms')} ms:\n{body[:3000]}{cut}"
    if tool == "n8n.create":
        return "Routine creata nel container n8n. Puoi consultarla in Automazioni."
    if tool == "security.status":
        sig = output.get("signatures") or {}
        audit = "integra" if output.get("audit_chain_valid") else "da verificare"
        return (f"Catena di audit {audit}. Firme: {'valide' if sig.get('valid') else 'da controllare'}. "
                f"Dispositivi attivi: {output.get('devices_active', 0)}. "
                f"Conferme in attesa: {output.get('pending_confirmations', 0)}.")
    if tool == "system.info":
        parts = [f"{k}: {v}" for k, v in output.items()]
        return "Informazioni sul sistema:\n" + "\n".join(f"• {p}" for p in parts)
    if tool == "fs.list":
        entries = output.get("entries", [])
        if not entries:
            return f"La cartella '{output.get('path', '.')}' della workspace è vuota."
        lines = [f"• {e['name']}{'/' if e.get('kind') == 'dir' else ''} ({e.get('size', 0)} byte)" for e in entries]
        return f"Contenuto di '{output.get('path', '.')}':\n" + "\n".join(lines)
    if tool == "fs.read":
        return f"Contenuto di '{output.get('path')}':\n\n{output.get('content', '')}"
    if tool == "fs.write":
        return f"Ho scritto {output.get('bytes_written', 0)} byte in '{output.get('path')}'."
    if tool == "memory.remember":
        if output.get("already_known"):
            return f"Lo sapevo già (id {output.get('id')}): {output.get('text')}"
        what = "Preferenza salvata" if output.get("kind") == "procedural" else "Ricordato"
        return f"{what} (id {output.get('id')}): {output.get('text')}"
    if tool == "memory.list":
        items = output.get("items", [])
        if not items:
            return "La memoria è vuota."
        return "Ecco cosa ricordo:\n" + "\n".join(f"• [{i['id']}]{_kind_label(i)} {i['text']}" for i in items)
    if tool == "memory.search":
        items = output.get("items", [])
        if not items:
            return f"Non trovo nulla in memoria su «{output.get('query')}»."
        return f"In memoria su «{output.get('query')}»:\n" + "\n".join(f"• [{i['id']}]{_kind_label(i)} {i['text']}" for i in items)
    if tool == "memory.forget":
        return f"Ho dimenticato l'elemento {output.get('id')}."
    if tool == "web.search":
        results = output.get("results", [])
        if not results:
            return f"Nessun risultato per «{output.get('query')}» ({output.get('provider')})."
        lines = [f"[{r['rank']}] {r['title']} — {r['url']}" + (f"\n    {r['snippet'][:200]}" if r.get("snippet") else "") for r in results]
        return (f"Risultati per «{output.get('query')}» (fonte: {output.get('provider')}, {output.get('retrieved_at')}):\n"
                + "\n".join(lines) + "\n\nPer leggere una pagina: «apri <url>».")
    if tool == "web.fetch":
        content = (output.get("content") or "").strip()
        excerpt = content[:1200] + ("…" if len(content) > 1200 else "")
        return (f"{output.get('title')}\n\n{excerpt}\n\nFonte: {output.get('final_url')} "
                f"(recuperata il {output.get('retrieved_at')}, SHA-256 {str(output.get('content_sha256'))[:16]}…). "
                "Il testo è riportato come dato: non eseguo istruzioni contenute nella pagina.")
    if tool == "workflow.create":
        return (f"Automazione creata [{output.get('id')}] «{output.get('name')}»: {output.get('trigger')}. "
                f"Prossima esecuzione: {output.get('next_run') or 'nessuna'}. Passi: {', '.join(output.get('steps') or [])}.")
    if tool == "workflow.list":
        items = output.get("items", [])
        if not items:
            return "Non ci sono automazioni. Prova: \"ogni ora info sistema\"."
        lines = []
        for w in items:
            state = "attiva" if w.get("enabled") else "sospesa"
            last = (w.get("last_run") or {}).get("status", "mai eseguita")
            lines.append(f"• [{w['id']}] {w['name']} — {w['trigger_text']}, {state}; prossima: {w.get('next_run') or '—'}; ultima: {last}")
        return "Automazioni:\n" + "\n".join(lines)
    if tool == "workflow.run":
        steps = ", ".join(f"{s['tool']}: {s['status']}" for s in output.get("steps", []))
        return f"Ho eseguito «{output.get('name')}»: esito {output.get('status')} ({steps})."
    if tool == "workflow.update":
        changes = "\n".join(f"• {c}" for c in output.get("changes", []))
        return (f"Automazione [{output.get('id')}] «{output.get('name')}» aggiornata alla versione {output.get('version')}:\n{changes}\n"
                f"Prossima esecuzione: {output.get('next_run') or 'nessuna'}.")
    if tool == "workflow.delete":
        return f"Automazione [{output.get('id')}] «{output.get('name')}» eliminata."
    if tool.startswith("connector."):
        data = json.dumps(output.get("data"), ensure_ascii=False, indent=1) if isinstance(output, dict) else str(output)
        excerpt = data[:1500] + ("…" if len(data) > 1500 else "")
        return (f"Risposta di {tool} (HTTP {output.get('status')}, {output.get('retrieved_at')}):\n{excerpt}\n"
                "Il contenuto viene dal servizio esterno ed è riportato come dato.")
    return f"Risultato di {tool}: {json.dumps(output, ensure_ascii=False)}"


_CONNECTOR_CALL = re.compile(r"^\s*(?:usa\s+)?connettore\s+([a-z][a-z0-9_]*)\s+([a-z][a-z0-9_]*)\s*(.*)$", re.IGNORECASE | re.DOTALL)
_ARG = re.compile(r'([a-z_][a-z0-9_]*)=(?:"([^"]*)"|(\S+))', re.IGNORECASE)


def _coerce(value: str, schema: dict) -> Any:
    kind = schema.get("type")
    try:
        if kind == "integer":
            return int(value)
        if kind == "number":
            return float(value)
    except ValueError:
        return value
    if kind == "boolean":
        return value.lower() in {"true", "si", "sì", "1"}
    return value


_NODE_CALL = re.compile(r"^\s*(?:sul\s+)?nodo\s+([a-z][a-z0-9-]*)\s+([a-z]+\.[a-z]+)\s*(.*)$", re.IGNORECASE | re.DOTALL)


def _parse_node_call(message: str, catalog: dict[str, dict]) -> tuple[str, dict] | None:
    """«nodo <nome> <strumento> chiave=valore …» → strumento eseguito da un nodo esecutore."""
    match = _NODE_CALL.match(message)
    if not match:
        return None
    tool = f"node.{match.group(1).lower()}.{match.group(2).lower()}"
    if tool not in catalog:
        return None
    props = catalog[tool].get("input_schema", {}).get("properties", {})
    args = {m.group(1): _coerce(m.group(2) if m.group(2) is not None else m.group(3), props.get(m.group(1), {}))
            for m in _ARG.finditer(match.group(3))}
    return tool, args


def _parse_connector_call(message: str, catalog: dict[str, dict]) -> tuple[str, dict] | None:
    """«connettore <id> <operazione> chiave=valore …» → chiamata a uno strumento dinamico."""
    match = _CONNECTOR_CALL.match(message)
    if not match:
        return None
    tool = f"connector.{match.group(1).lower()}.{match.group(2).lower()}"
    if tool not in catalog:
        return None
    props = catalog[tool].get("input_schema", {}).get("properties", {})
    args = {m.group(1): _coerce(m.group(2) if m.group(2) is not None else m.group(3), props.get(m.group(1), {}))
            for m in _ARG.finditer(match.group(3))}
    return tool, args


_STRUCTURED_CONNECTOR = re.compile(
    r"^connettore\s+([a-z][a-z0-9_]*)\s+([a-z][a-z0-9_]*)\s+(\{.*)$",
    re.IGNORECASE | re.DOTALL,
)


def _structured_connector(message: str, catalog: dict[str, dict], backend: dict) -> dict | None:
    match = _STRUCTURED_CONNECTOR.fullmatch(message.strip())
    if not match:
        return None
    tool = f"connector.{match[1].lower()}.{match[2].lower()}"
    if tool not in catalog:
        return _result("reply", backend, text="Operazione del connettore non disponibile. Verifica l’autorizzazione in Connettori.")
    try:
        args = json.loads(match[3])
    except ValueError:
        return _result("reply", backend, text="I parametri del connettore devono essere un oggetto JSON valido.")
    return _result("tool_call", backend, tool=tool, arguments=args, catalog=catalog,
                   rationale="Operazione del connettore indicata esplicitamente")


def _reason(output: Any) -> str:
    if isinstance(output, dict):
        return str(output.get("message") or output.get("reason") or output.get("code") or output)
    return str(output)


def _form_fields(text: str) -> dict:
    """«nome=Ada, email=ada@example.org» -> {"nome": "Ada", "email": "ada@example.org"}."""
    out = {}
    for part in re.split(r",\s*(?=[\w.-]+\s*=)", text.strip()):
        if "=" in part:
            k, v = part.split("=", 1)
            out[k.strip()[:64]] = v.strip()[:500]
    return out


_RULES: list[tuple[re.Pattern[str], str, Any]] = [
    # Strumenti quotidiani (ADR-0046).
    (re.compile(r"^\s*(?:calcola|quanto fa|quanto vale|calc)\s*:?\s+(.+?)\s*\??$", re.I), "math.calc", lambda m: {"expression": m.group(1)}),
    (re.compile(r"^\s*(?:converti|quanti|quante)\s+(-?\d+(?:[.,]\d+)?)\s*([a-z°/0-9]+)\s+(?:in|a)\s+([a-z°/0-9]+)\s*\??$", re.I), "units.convert",
     lambda m: {"value": float(m.group(1).replace(",", ".")), "from": m.group(2), "to": m.group(3)}),
    (re.compile(r"^\s*quanti giorni (?:mancano|ci sono) (?:al|fino al|dal)\s+(\d{4}-\d{2}-\d{2})\s*\??$", re.I), "date.calc",
     lambda m: {"date": time.strftime("%Y-%m-%d"), "until": m.group(1)}),
    (re.compile(r"^\s*(?:che tempo fa|meteo|previsioni(?: meteo)?)\s+(?:a|ad|in|per|di)?\s*(.+?)\s*\??$", re.I), "weather.forecast",
     lambda m: {"place": m.group(1).strip()[:120], "days": 3}),
    (re.compile(r"\b(spazio (?:su|del|sul) disco|spazio libero|dischi)\b", re.I), "system.disks", lambda m: {}),
    (re.compile(r"^\s*cerca nel codice\s+[«\"']?(.+?)[»\"']?\s*$", re.I), "fs.search", lambda m: {"text": m.group(1)[:200]}),
    (re.compile(r"^\s*(?:trova|cerca) (?:il |i )?file\s+(?:che contengono|con)\s+[«\"']?(.+?)[»\"']?\s*$", re.I), "fs.search", lambda m: {"text": m.group(1)[:200]}),
    (re.compile(r"^\s*(?:trova|cerca) (?:il |i )?file\s+[«\"']?([^«»\"']+?)[»\"']?\s*$", re.I), "fs.search", lambda m: {"name": m.group(1)[:200]}),
    (re.compile(r"^\s*(?:prendi nota|annota|nuova nota)\s*:?\s+(.+)$", re.I | re.S), "notes.add", lambda m: {"text": m.group(1).strip()[:10000]}),
    (re.compile(r"^\s*(?:le mie note|mostra(?:mi)? le note|elenca le note|note)\s*$", re.I), "notes.list", lambda m: {}),
    (re.compile(r"^\s*cerca nelle note\s*:?\s+(.+)$", re.I), "notes.search", lambda m: {"query": m.group(1)[:200]}),
    (re.compile(r"^\s*(?:aggiungi (?:alle|tra le) cose da fare|da fare|ricordami di)\s*:?\s+(.+?)(?:\s+(?:entro|per il|il)\s+(\d{4}-\d{2}-\d{2}(?:[ T]\d{2}:\d{2})?))?\s*$", re.I), "todo.add",
     lambda m: {"text": m.group(1)[:500], **({"due": m.group(2)} if m.group(2) else {})}),
    (re.compile(r"^\s*(?:cose da fare|lista delle cose da fare|cosa devo fare)\s*\??$", re.I), "todo.list", lambda m: {}),
    (re.compile(r"^\s*fatto\s+([0-9a-f]{8})\s*$", re.I), "todo.done", lambda m: {"id": m.group(1)}),
    (re.compile(r"^\s*(?:crea (?:la )?cartella|mkdir)\s+[«\"']?([^«»\"'\s]+)[»\"']?\s*$", re.I), "fs.mkdir", lambda m: {"path": m.group(1)}),
    (re.compile(r"^\s*(?:sposta|rinomina)\s+[«\"']?([^«»\"'\s]+)[»\"']?\s+(?:in|a|come)\s+[«\"']?([^«»\"'\s]+)[»\"']?\s*$", re.I), "fs.move",
     lambda m: {"from": m.group(1), "to": m.group(2)}),
    (re.compile(r"^\s*(?:elimina|cancella) (?:il file |la cartella )?[«\"']?([^«»\"'\s]+\.[a-z0-9]+|[^«»\"'\s]+/)[»\"']?\s*$", re.I), "fs.delete",
     lambda m: {"path": m.group(1).rstrip("/")}),
    # Navigazione con il browser (ADR-0043).
    (re.compile(r"^\s*naviga(?: su| in| a)?\s+(https?://\S+)(?:\s+e\s+(?:clicca|apri|segui)(?: su)?\s+[«\"']?(.+?)[»\"']?)?\s*$", re.I), "web.browse",
     lambda m: {"url": m.group(1), **({"follow": [m.group(2).strip()[:120]]} if m.group(2) else {})}),
    (re.compile(r"^\s*compila(?: il)? modulo(?: su| di| in)?\s+(https?://\S+)\s*:\s*(.+)$", re.I | re.S), "web.form",
     lambda m: {"url": m.group(1), "fields": _form_fields(m.group(2))}),
    # Agente temporaneo (ADR-0040): «delega a un agente <ruolo>: <compito>».
    (re.compile(r"^\s*(?:verifica|controlla)\s+(?:la\s+)?sintassi\s+(?:python\s+)?[\"']?([^\s\"']+)[\"']?\s*$", re.I), "code.check_python",
     lambda m: {"path": m[1]}),
    (re.compile(r"^\s*esegui\s+python\s+in\s+sandbox\s*:\s*(\S.*)$", re.I | re.S), "code.run_python",
     lambda m: {"source": m[1]}),
    (re.compile(r"^\s*(?:delega a un agente|agente temporaneo|sotto-?agente)\s+(?:come\s+)?(.{3,200}?)\s*:\s*(.{3,2000})$", re.I | re.S), "agent.delegate",
     lambda m: {"role": m.group(1).strip(), "objective": m.group(2).strip()}),
    # Terminale (ADR-0038): solo con il prefisso esplicito; il Core chiede sempre la conferma.
    (re.compile(r"^\s*(?:terminale|esegui(?: il)? comando|shell|\$)\s*:?\s+(.+)$", re.I | re.S), "shell.run",
     lambda m: {"command": m.group(1).strip()[:2000]}),
    (re.compile(r"\b(processi|programmi (?:in esecuzione|aperti|attivi)|task manager|cosa (?:sta )?(?:girando|usa(?:ndo)? la cpu))\b", re.I), "process.list",
     lambda m: {"sort": "memory" if re.search(r"memoria|ram", m.string, re.I) else "cpu"}),
    (re.compile(r"^\s*(?:apri|visita|leggi(?: la pagina)?|riassumi|fetch|open)\s+(https?://\S+)\s*$", re.I), "web.fetch", lambda m: {"url": m.group(1)}),
    (re.compile(r"^\s*(?:cerca nella (?:mia )?memoria|cerca tra i ricordi|search memory(?: for)?)\s*:?\s+(.+)$|^\s*cosa sai (?:di|su|sul|sulla|del|della)\s+(?!me\b)(.+?)\s*\??$", re.I | re.S),
     "memory.search", lambda m: {"query": (m.group(1) or m.group(2)).strip()[:500]}),
    (re.compile(r"^\s*(?:cerca(?!\s+(?:nelle note|(?:il |i )?file)\b)(?: sul web| online| in rete)?|search(?: the web)?)\s*:?\s+(.+)$", re.I | re.S), "web.search", lambda m: {"query": m.group(1).strip()[:300]}),
    (re.compile(r"\b(che ore sono|che ora è|ora attuale|data di oggi|che giorno è|what time)\b", re.I), "time.now", lambda m: {}),
    (re.compile(r"^\s*n8n\s+(audit|promemoria)\s+ogni\s+([0-9]{1,2})\s+minuti\s*:\s*(.{1,80})$", re.I),
     "n8n.create", lambda m: {"name": m[3].strip(), "minutes": int(m[2]), "kind": "audit" if m[1].lower() == "audit" else "reminder"}),
    (re.compile(r"\b(cybersecurity|postura di sicurezza|centro sicurezza|audit di sicurezza|controllo di sicurezza)\b", re.I), "security.status", lambda m: {}),
    (re.compile(r"\b(info(?:rmazioni)? (?:di |sul |del )?sistema|system info|che sistema)\b", re.I), "system.info", lambda m: {}),
    (re.compile(r"^\s*(?:leggi|apri|mostra(?:mi)? il file|read)\s+(?:il file\s+)?[\"']?([^\"'\s]+)[\"']?\s*$", re.I), "fs.read",
     lambda m: {"path": m.group(1)}),
    (re.compile(r"^\s*(?:scrivi|crea(?: il)? file|salva(?: in)?|write)\s+[\"']?([^\"':\s]+)[\"']?\s*(?::|con(?: contenuto)?|with)\s*(.+)$", re.I | re.S),
     "fs.write", lambda m: {"path": m.group(1), "content": m.group(2).strip()}),
    (re.compile(r"^\s*(?:elenca|lista|mostra)(?: i)? file(?: in| di| della cartella)?\s*[\"']?([^\"'\s]*)[\"']?\s*$|^\s*(?:ls|list files)\s*([^\s]*)\s*$", re.I),
     "fs.list", lambda m: {"path": (m.group(1) or m.group(2) or ".")}),
    (re.compile(r"^\s*(?:ricorda(?:ti)?(?: che)?|remember(?: that)?)\s+(.+)$", re.I | re.S), "memory.remember", lambda m: {"text": m.group(1).strip()}),
    (re.compile(r"^\s*((?:preferisco|d'ora in poi|da ora in poi|d’ora in poi)\b.+)$", re.I | re.S), "memory.remember",
     lambda m: {"text": m.group(1).strip()[:1000], "kind": "procedural"}),
    (re.compile(r"\b(cosa ricordi|cosa sai di me|mostra(?: la)? memoria|what do you remember)\b", re.I), "memory.list", lambda m: {}),
    (re.compile(r"^\s*(?:dimentica|forget)\s+([A-Za-z0-9-]+)\s*$", re.I), "memory.forget", lambda m: {"id": m.group(1)}),
]

_UNITS = {"second": 1, "minut": 60, "or": 3600, "giorn": 86400, "minute": 60, "hour": 3600, "day": 86400}
_SCHEDULE = [
    (re.compile(r"^\s*ogni giorno alle(?: ore)?\s+(\d{1,2})[:.](\d{2})\s*,?\s+(.+)$", re.I | re.S), "daily"),
    (re.compile(r"^\s*(?:ogni|every)\s+(?:(\d+)\s+)?(second[oi]|seconds?|minut[oi]|or[ae]|giorn[oi]|minutes?|hours?|days?)\s*,?\s+(.+)$", re.I | re.S), "interval"),
    (re.compile(r"^\s*(?:tra|fra|in)\s+(\d+)\s+(minut[oi]|or[ae]|minutes?|hours?)\s*,?\s+(.+)$", re.I | re.S), "once"),
]
_WORKFLOW_RULES: list[tuple[re.Pattern[str], str, Any]] = [
    (re.compile(r"^\s*(?:apri|visita|leggi(?: la pagina)?|riassumi|fetch|open)\s+(https?://\S+)\s*$", re.I), "web.fetch", lambda m: {"url": m.group(1)}),
    (re.compile(r"^\s*(?:cerca nella (?:mia )?memoria|cerca tra i ricordi|search memory(?: for)?)\s*:?\s+(.+)$|^\s*cosa sai (?:di|su|sul|sulla|del|della)\s+(?!me\b)(.+?)\s*\??$", re.I | re.S),
     "memory.search", lambda m: {"query": (m.group(1) or m.group(2)).strip()[:500]}),
    (re.compile(r"^\s*(?:cerca(?!\s+(?:nelle note|(?:il |i )?file)\b)(?: sul web| online| in rete)?|search(?: the web)?)\s*:?\s+(.+)$", re.I | re.S), "web.search", lambda m: {"query": m.group(1).strip()[:300]}),
    (re.compile(r"\b(elenca|mostra|quali|lista)( le)? automazioni\b|\bautomazioni\?*$", re.I), "workflow.list", lambda m: {}),
    (re.compile(r"^\s*(?:esegui|avvia|lancia)(?: ora)? (?:l')?automazione\s+([0-9a-f-]{4,})\s*$", re.I), "workflow.run", lambda m: {"id": m.group(1)}),
    (re.compile(r"^\s*(?:elimina|cancella|rimuovi) (?:l')?automazione\s+([0-9a-f-]{4,})\s*$", re.I), "workflow.delete", lambda m: {"id": m.group(1)}),
]


def _unit_seconds(unit: str) -> int:
    unit = unit.lower()
    for prefix, secs in _UNITS.items():
        if unit.startswith(prefix):
            return secs
    return 60


_TRIGGER_ONLY = [
    (re.compile(r"^\s*ogni giorno alle(?: ore)?\s+(\d{1,2})[:.](\d{2})\s*$", re.I), "daily"),
    (re.compile(r"^\s*(?:ogni|every)\s+(?:(\d+)\s+)?(second[oi]|seconds?|minut[oi]|or[ae]|giorn[oi]|minutes?|hours?|days?)\s*$", re.I), "interval"),
    (re.compile(r"^\s*(?:tra|fra|in)\s+(\d+)\s+(minut[oi]|or[ae]|minutes?|hours?)\s*$", re.I), "once"),
]
_UPDATE = re.compile(r"^\s*(?:modifica|cambia|aggiorna) (?:l')?automazione\s+([0-9a-f-]{4,})\s*(?::\s*(.+)|\s+(.+))$", re.I | re.S)
_RENAME = re.compile(r"^\s*rinomina (?:l')?automazione\s+([0-9a-f-]{4,})\s+(?:in|come)\s+(.+)$", re.I | re.S)


def _parse_trigger(text: str) -> tuple[dict | None, str | None]:
    for pattern, kind in _TRIGGER_ONLY:
        m = pattern.search(text)
        if not m:
            continue
        if kind == "daily":
            hour, minute = int(m.group(1)), m.group(2)
            if hour > 23 or int(minute) > 59:
                return None, "Orario non valido."
            return {"kind": "daily", "at": f"{hour:02d}:{minute}"}, None
        if kind == "interval":
            every = int(m.group(1) or 1) * _unit_seconds(m.group(2))
            if every < 60:
                return None, "L'intervallo minimo è un minuto."
            return {"kind": "interval", "every_seconds": every}, None
        at = (datetime.now(timezone.utc) + timedelta(seconds=int(m.group(1)) * _unit_seconds(m.group(2)))).replace(microsecond=0)
        return {"kind": "once", "at": at.isoformat().replace("+00:00", "Z")}, None
    return None, None


def _parse_update(message: str, catalog: dict[str, dict]) -> tuple[dict | None, str | None]:
    """Argomenti per workflow.update, oppure un messaggio d'errore."""
    m = _RENAME.search(message)
    if m:
        return {"id": m.group(1), "name": m.group(2).strip()[:200]}, None
    m = _UPDATE.search(message)
    if not m:
        return None, None
    wid, after_colon, rest = m.group(1), m.group(2), m.group(3)
    if rest:
        trigger, problem = _parse_trigger(rest)
        if problem:
            return None, problem
        if trigger:
            return {"id": wid, "trigger": trigger}, None
        after_colon = rest
    step = _parse_step(after_colon.strip(), catalog)
    if not step:
        return None, (f"Non so cosa cambiare in «{after_colon.strip()}». Esempi: «modifica automazione {wid} ogni 2 ore» "
                      f"oppure «modifica automazione {wid}: info sistema».")
    return {"id": wid, "steps": [step]}, None


def _parse_step(command: str, catalog: dict[str, dict]) -> dict | None:
    for pattern, tool, args in _RULES:
        match = pattern.search(command)
        if match and tool in catalog and not tool.startswith("workflow."):
            return {"tool": tool, "arguments": args(match)}
    return None


def _parse_schedule(message: str, catalog: dict[str, dict]) -> tuple[dict | None, str | None]:
    """Restituisce (argomenti per workflow.create, errore)."""
    for pattern, kind in _SCHEDULE:
        m = pattern.search(message)
        if not m:
            continue
        if kind == "daily":
            hour, minute, command = int(m.group(1)), m.group(2), m.group(3)
            if hour > 23 or int(minute) > 59:
                return None, "Orario non valido."
            trigger = {"kind": "daily", "at": f"{hour:02d}:{minute}"}
        elif kind == "interval":
            every = int(m.group(1) or 1) * _unit_seconds(m.group(2))
            if every < 60:
                return None, "L'intervallo minimo è un minuto."
            trigger, command = {"kind": "interval", "every_seconds": every}, m.group(3)
        else:
            delay = int(m.group(1)) * _unit_seconds(m.group(2))
            at = (datetime.now(timezone.utc) + timedelta(seconds=delay)).replace(microsecond=0)
            trigger, command = {"kind": "once", "at": at.isoformat().replace("+00:00", "Z")}, m.group(3)
        step = _parse_step(command.strip(), catalog)
        if not step:
            return None, f"Non so pianificare «{command.strip()}»: uso gli stessi comandi della chat (per esempio «info sistema» o «scrivi file.txt: testo»)."
        name = command.strip()[:80]
        return {"name": name, "trigger": trigger, "steps": [step]}, None
    return None, None


HELP = """Sono JARVIS, in modalità offline: capisco comandi semplici.
Prova con:
• "che ore sono"
• "info sistema"
• "elenca file"
• "delega a un agente analista: che ore sono" (agente temporaneo con strumenti di sola lettura)
• "calcola 15% di 230" · "converti 10 km in mi" · "quanti giorni mancano al 2026-12-25" · "meteo a Roma" · "spazio su disco"
• "prendi nota: …" · "le mie note" · "ricordami di pagare la bolletta entro 2026-11-30 18:00" · "cose da fare" · "fatto <id>"
• "trova file con «bolletta»" · "crea cartella archivio" · "sposta a.txt in archivio/a.txt" · "elimina vecchio.txt" (conferma)
• "processi" · "processi per memoria" · "terminale: ls -la" (ogni comando chiede conferma)
• "scrivi note.txt: testo da salvare" (chiede conferma)
• "leggi note.txt"
• "ricorda che …", "preferisco …", "cosa sai di …", "cerca nella memoria …"
• "ricorda che preferisco risposte brevi" · "cosa ricordi" · "dimentica <id>"
• "ogni ora info sistema" · "ogni giorno alle 08:00 scrivi buongiorno.txt: ciao" · "tra 5 minuti che ore sono"
• "elenca automazioni" · "esegui automazione <id>" · "elimina automazione <id>"
• "modifica automazione <id> ogni 2 ore" · "modifica automazione <id>: info sistema" · "rinomina automazione <id> in Report"
• "naviga https://esempio.org e clicca Contatti" · "compila modulo https://esempio.org/form: nome=Ada" (chiede conferma)
• "cerca intelligenza artificiale" · "apri https://it.wikipedia.org/wiki/Rust"
• "connettore <id> <operazione> chiave=valore" (connettori installati nella sezione Connettori)
• "nodo <nome> system.info" · "nodo <nome> fs.list path=." (nodi esecutori associati nella sezione Dispositivi)
Per la conversazione libera configura un provider cloud tramite l'API /api/cloud del Core."""


class OfflineBackend:
    """Modello deterministico a regole: nessuna rete, nessun peso, risultati riproducibili."""

    info = {"name": "offline", "model": OFFLINE_MODEL, "location": "local"}

    def __init__(self, degraded_reason: str | None = None):
        self.backend = dict(self.info)
        if degraded_reason:
            self.backend["degraded_reason"] = degraded_reason[:500]

    def respond(self, delegation: dict, catalog: dict[str, dict]) -> dict:
        inputs = delegation.get("inputs", {})
        if "tool_result" in inputs:
            return _result("reply", self.backend, text=_describe_tool_result(inputs["tool_result"]))
        message = _last_user_message(delegation)
        node_call = _parse_node_call(message, catalog)
        if node_call:
            return _result("tool_call", self.backend, tool=node_call[0], arguments=node_call[1], catalog=catalog,
                           rationale="Chiamata a un nodo esecutore riconosciuta dal modello offline")
        connector_call = _parse_connector_call(message, catalog)
        if connector_call:
            return _result("tool_call", self.backend, tool=connector_call[0], arguments=connector_call[1], catalog=catalog,
                           rationale="Chiamata a un connettore riconosciuta dal modello offline")
        if "workflow.create" in catalog:
            created, problem = _parse_schedule(message, catalog)
            if problem:
                return _result("reply", self.backend, text=problem)
            if created:
                return _result("tool_call", self.backend, tool="workflow.create", arguments=created, catalog=catalog,
                               rationale="Pianificazione riconosciuta dal modello offline")
        if "workflow.update" in catalog:
            update, problem = _parse_update(message, catalog)
            if problem:
                return _result("reply", self.backend, text=problem)
            if update:
                return _result("tool_call", self.backend, tool="workflow.update", arguments=update, catalog=catalog,
                               rationale="Modifica riconosciuta dal modello offline")
        for pattern, tool, args in _WORKFLOW_RULES:
            match = pattern.search(message)
            if match and tool in catalog:
                return _result("tool_call", self.backend, tool=tool, arguments=args(match), catalog=catalog,
                               rationale="Comando riconosciuto dal modello offline")
        for pattern, tool, args in _RULES:
            match = pattern.search(message)
            if match and tool in catalog:
                return _result("tool_call", self.backend, tool=tool, arguments=args(match), catalog=catalog,
                               rationale="Comando riconosciuto dal modello offline")
        lowered = message.lower().strip(" !?.")
        if lowered in {"ciao", "salve", "buongiorno", "buonasera", "hello", "hi", "hey"}:
            return _result("reply", self.backend, text="Ciao! " + HELP)
        if re.search(r"\b(?:di prima|di sopra|precedente|quest'ultim[oa])\b", lowered) or re.fullmatch(
            r"(?:e\s+)?(?:quell[oaie]|quest[oaie]|l'altr[oa]|prima|dopo|perché|come mai)(?:\s+lì)?", lowered
        ):
            return _result("reply", self.backend, text="A quale elemento ti riferisci? Indicami il nome o ripeti la richiesta completa.")
        # Cerca termini interi e ignora le parole generiche che accomunano richieste e ricordi diversi.
        generic = {"quale", "quali", "quello", "quella", "quelli", "quelle", "questo", "questa",
                   "questi", "queste", "come", "dove", "quando", "prima", "dopo", "anche", "della",
                   "dello", "delle", "degli", "dall", "sulla", "sullo", "ancora", "altro", "altra"}
        words = set(re.findall(r"\w{4,}", lowered)) - generic
        hits = [m for m in inputs.get("memory", []) if words.intersection(re.findall(r"\w+", m.lower()))]
        if hits:
            return _result("reply", self.backend, text="Dalla memoria:\n" + "\n".join(f"• {h}" for h in hits[:5]))
        return _result("reply", self.backend, text="Non ho capito la richiesta in modalità offline.\n\n" + HELP)


def _last_user_message(delegation: dict) -> str:
    for msg in reversed(delegation.get("inputs", {}).get("messages", [])):
        if msg["role"] == "user":
            return msg["content"]
    return delegation["objective"]



_KIND_LABELS = {"procedural": " (preferenza)", "episodic": " (episodio)"}


def _kind_label(item: dict) -> str:
    return _KIND_LABELS.get(item.get("kind", ""), "")


class Gateway:
    def __init__(self):
        self.catalog = load_catalog()
        self.offline = OfflineBackend()

    def status(self) -> dict:
        return {"backend": self.offline.backend, "tools": sorted(self.catalog), "reason": None}

    def respond(self, delegation: dict, emit: Callable[[str], None] | None = None) -> dict:
        """`emit` riceve i frammenti di testo man mano che il modello li genera (streaming, ADR-0025)."""
        allowed = {(t["name"], t["version"]) for t in delegation.get("tools", [])}
        catalog = {k: v for k, v in self.catalog.items() if (k, v["version"]) in allowed}
        # Strumenti dinamici (connettori RF-14): descritti dal Core nella delega, mai dal catalogo statico.
        for d in delegation.get("inputs", {}).get("tool_descriptors", []):
            if (d["name"].startswith(("connector.", "node."))
                    and (d["name"], d["version"]) in allowed):
                catalog[d["name"]] = d
        inputs = delegation.get("inputs", {})
        message = _last_user_message(delegation)
        if _STRUCTURED_CONNECTOR.fullmatch(message.strip()):
            history = inputs.get("tool_history") or []
            result = inputs.get("tool_result") or (history[-1] if history else None)
            if result:
                return _result("reply", self.offline.backend, text=_describe_tool_result(result))
            return _structured_connector(message, catalog, self.offline.backend)
        return self.offline.respond(delegation, catalog)


def catalog_path() -> Path:
    return CATALOG_DIR
