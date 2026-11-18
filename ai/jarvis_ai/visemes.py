"""Labiale (ADR-0029): visemi temporizzati per la risposta vocale.

Il testo italiano viene convertito in una sequenza di visemi (forme della bocca). Le durate relative seguono la
fonetica: vocali lunghe, consonanti brevi, pause alla punteggiatura. I tempi vengono poi allineati all'audio
sintetizzato:
- la durata totale è quella dell'audio;
- le pause del testo cadono sui silenzi rilevati nell'audio.

L'allineamento è stimato: Piper non espone i tempi dei singoli fonemi per tutte le voci.
"""

from __future__ import annotations

import unicodedata

# Visemi: A, E, I, O, U (vocali), M (labbra chiuse: m b p), F (labbra sui denti: f v), L (consonanti con bocca
# semiaperta), S (sibilanti, denti vicini), X (pausa, bocca chiusa a riposo).
VISEMES = ("A", "E", "I", "O", "U", "M", "F", "L", "S", "X")
_WEIGHT = {"A": 1.6, "E": 1.4, "I": 1.3, "O": 1.5, "U": 1.3, "M": 0.8, "F": 0.8, "L": 0.7, "S": 0.9, "X": 1.0}
_PAUSE = {",": 2.0, ";": 2.5, ":": 2.5, ".": 3.5, "!": 3.5, "?": 3.5, "\n": 3.5}
MAX_ITEMS = 2000


def _fold(ch: str) -> str:
    return unicodedata.normalize("NFD", ch)[0].lower()


def _viseme(ch: str, nxt: str) -> str | None:
    c = _fold(ch)
    if c in "aeiou":
        return c.upper()
    if c in "mbp":
        return "M"
    if c in "fv":
        return "F"
    if c in "szx" or (c == "c" and _fold(nxt) in "ei") or (c == "g" and _fold(nxt) in "ei"):
        return "S"
    if c.isalpha():
        return None if c == "h" else "L"
    return None


def sequence(text: str) -> list[tuple[str, float]]:
    """Visemi con peso relativo; le lettere consecutive con lo stesso visema si fondono."""
    out: list[tuple[str, float]] = []
    chars = list(text)
    for i, ch in enumerate(chars):
        nxt = chars[i + 1] if i + 1 < len(chars) else ""
        if ch in _PAUSE:
            v, w = "X", _PAUSE[ch]
        elif ch.isspace():
            v, w = "X", 0.4
        else:
            vis = _viseme(ch, nxt)
            if vis is None:
                continue
            v, w = vis, _WEIGHT[vis]
        if out and out[-1][0] == v:
            out[-1] = (v, out[-1][1] + (w if v == "X" else w * 0.5))
        else:
            out.append((v, w))
    while out and out[0][0] == "X":
        out.pop(0)
    while out and out[-1][0] == "X":
        out.pop()
    return out


def _silences(samples: list[int], rate: int, frame_ms: int = 20, threshold: int = 500, min_ms: int = 120) -> list[tuple[int, int]]:
    """Intervalli di silenzio (inizio, fine in ms) più lunghi di `min_ms`."""
    step = max(1, rate * frame_ms // 1000)
    spans: list[tuple[int, int]] = []
    start = None
    for i in range(0, len(samples), step):
        frame = samples[i:i + step]
        loud = max((abs(x) for x in frame), default=0) > threshold
        t = i * 1000 // rate
        if not loud and start is None:
            start = t
        elif loud and start is not None:
            if t - start >= min_ms:
                spans.append((start, t))
            start = None
    return spans


def timeline(text: str, samples: list[int], rate: int) -> list[dict]:
    """Visemi con inizio e durata in millisecondi, allineati alla durata e ai silenzi dell'audio."""
    seq = sequence(text)
    if not seq or not samples or rate <= 0:
        return []
    total_ms = len(samples) * 1000 // rate
    all_silences = _silences(samples, rate)
    # Inizio e fine del parlato: i silenzi agli estremi restano bocca chiusa.
    lead = next((e for s, e in all_silences if s == 0), 0)
    tail_start = next((s for s, e in all_silences if e >= total_ms - 25), total_ms)
    inner = [(s, e) for s, e in all_silences if s > 0 and e < total_ms - 25]
    span = max(1, tail_start - lead)
    weights = sum(w for _, w in seq)
    # Ogni pausa lunga del testo si aggancia al silenzio più vicino nell'audio, se c'è.
    anchors: dict[int, tuple[int, int]] = {}
    used: set[int] = set()
    for idx, (v, w) in enumerate(seq):
        if v != "X" or w < 2.0:
            continue
        expected = lead + span * sum(x for _, x in seq[:idx]) / weights
        best = min(((abs(s - expected), j) for j, (s, _) in enumerate(inner) if j not in used), default=None)
        if best and best[0] < span * 0.25:
            used.add(best[1])
            anchors[idx] = inner[best[1]]
    # Segmenti fra le pause agganciate: ognuno occupa esattamente il proprio tratto di audio.
    items: list[dict] = []
    start_i, start_t = 0, float(lead)
    for k in sorted(anchors) + [len(seq)]:
        end_t = float(anchors[k][0]) if k in anchors else float(tail_start)
        segment = seq[start_i:k]
        seg_w = sum(w for _, w in segment) or 1.0
        t = start_t
        for v, w in segment:
            d = (end_t - start_t) * w / seg_w
            items.append({"v": v, "t": int(t), "d": max(1, int(d))})
            t += d
        if k in anchors:
            s, e = anchors[k]
            items.append({"v": "X", "t": int(s), "d": max(1, e - s)})
            start_i, start_t = k + 1, float(e)
    return items[:MAX_ITEMS]
