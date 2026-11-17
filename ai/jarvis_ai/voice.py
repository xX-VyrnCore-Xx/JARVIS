"""Voce locale (RF-09): riconoscimento (STT), sintesi (TTS) e parola di attivazione.

Motori (ADR-0010, scelta del proprietario: tutto locale):

* STT: ``faster-whisper`` (modello ``JARVIS_STT_MODEL``, predefinito ``small``, CPU int8).
* TTS: ``piper-tts`` con la voce ONNX indicata da ``JARVIS_TTS_VOICE`` (percorso del file ``.onnx``).
* Parola di attivazione: ``openwakeword`` con il modello ``hey_jarvis``; se assente, si riconosce
  «Jarvis» all'inizio della trascrizione locale.
* ``fake``: motori deterministici per i test (nessun modello, nessuna rete).

``JARVIS_VOICE_ENGINE``: ``auto`` (predefinito: motori reali se installati e con i modelli
disponibili), ``local`` (solo motori reali), ``fake``, ``off``. L'audio arriva come WAV PCM 16 bit;
non viene salvato né inviato fuori dal computer.
"""

from __future__ import annotations

import base64
import io
import math
import os
import re
import struct
import wave
from pathlib import Path
from typing import Any

MAX_SECONDS = 60
MAX_BYTES = 4 * 1024 * 1024
MAX_TTS_CHARS = 4000
WAKE_RE = re.compile(r"^\s*(?:(?:ehi|ehy|hey|ok|ciao)\s*,?\s*)?jarvis\b[\s,.:!?-]*", re.I)


class VoiceError(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


# ---------------------------------------------------------------------- audio WAV


def read_wav(data: bytes) -> tuple[list[int], int, dict[str, bytes]]:
    """Restituisce campioni mono int16, frequenza e chunk RIFF aggiuntivi."""
    if len(data) > MAX_BYTES:
        raise VoiceError("voice.too_large", f"audio oltre {MAX_BYTES // (1024 * 1024)} MiB")
    try:
        with wave.open(io.BytesIO(data), "rb") as wav:
            channels, width, rate, frames = wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()
            raw = wav.readframes(frames)
    except (wave.Error, EOFError) as exc:
        raise VoiceError("voice.invalid_audio", f"atteso WAV PCM: {exc}") from exc
    if width != 2:
        raise VoiceError("voice.invalid_audio", "atteso PCM a 16 bit")
    if frames / max(rate, 1) > MAX_SECONDS:
        raise VoiceError("voice.too_long", f"registrazione oltre {MAX_SECONDS} secondi")
    samples = list(struct.unpack(f"<{len(raw) // 2}h", raw))
    if channels > 1:
        samples = [sum(samples[i:i + channels]) // channels for i in range(0, len(samples), channels)]
    return samples, rate, _extra_chunks(data)


def _extra_chunks(data: bytes) -> dict[str, bytes]:
    chunks: dict[str, bytes] = {}
    pos = 12
    while pos + 8 <= len(data):
        cid, size = data[pos:pos + 4].decode("latin-1"), struct.unpack("<I", data[pos + 4:pos + 8])[0]
        if cid not in ("fmt ", "data"):
            chunks[cid] = data[pos + 8:pos + 8 + size]
        pos += 8 + size + (size % 2)
    return chunks


def write_wav(samples: list[int], rate: int, extra: dict[str, bytes] | None = None) -> bytes:
    buf = io.BytesIO()
    with wave.open(buf, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(rate)
        wav.writeframes(struct.pack(f"<{len(samples)}h", *samples))
    data = buf.getvalue()
    for cid, payload in (extra or {}).items():
        pad = b"\0" if len(payload) % 2 else b""
        data += cid.encode("latin-1") + struct.pack("<I", len(payload)) + payload + pad
    if extra:
        data = data[:4] + struct.pack("<I", len(data) - 8) + data[8:]
    return data


def rms(samples: list[int]) -> float:
    return math.sqrt(sum(s * s for s in samples) / len(samples)) if samples else 0.0


def resample(samples: list[int], rate: int, target: int) -> list[int]:
    if rate == target or not samples:
        return samples
    ratio = rate / target
    return [samples[min(int(i * ratio), len(samples) - 1)] for i in range(int(len(samples) / ratio))]


# ---------------------------------------------------------------------- motori finti (test)


class FakeSTT:
    """Trascrizione deterministica: chunk RIFF ``jtxt`` se presente, poi ``JARVIS_FAKE_STT_TEXT``; silenzio → testo vuoto."""

    info = {"name": "fake", "model": "fake-stt-1", "location": "local"}

    def __init__(self, env: dict[str, str]):
        self.default_text = env.get("JARVIS_FAKE_STT_TEXT", "")

    def transcribe(self, data: bytes, language: str) -> str:
        samples, _, chunks = read_wav(data)
        if "jtxt" in chunks:
            return chunks["jtxt"].decode("utf-8")
        return self.default_text if rms(samples) > 200 else ""


class FakeTTS:
    """Tono sinusoidale lungo in proporzione al testo: audio valido, senza modelli."""

    info = {"name": "fake", "model": "fake-tts-1", "location": "local"}
    voice = "fake-voice"
    rate = 16000

    def synthesize(self, text: str) -> bytes:
        n = int(self.rate * min(0.06 * len(text), 20))
        return write_wav([int(6000 * math.sin(2 * math.pi * 440 * i / self.rate)) for i in range(n)], self.rate)


# ---------------------------------------------------------------------- motori reali


class WhisperSTT:
    def __init__(self, env: dict[str, str]):
        from faster_whisper import WhisperModel  # import pigro: dipendenza opzionale

        self.model_name = env.get("JARVIS_STT_MODEL", "small")
        root = env.get("JARVIS_MODELS_DIR") or None
        self.model = WhisperModel(self.model_name, device="cpu", compute_type="int8", download_root=root,
                                  local_files_only=env.get("JARVIS_VOICE_ALLOW_DOWNLOAD") != "1")
        self.info = {"name": "faster-whisper", "model": self.model_name, "location": "local"}

    def transcribe(self, data: bytes, language: str) -> str:
        read_wav(data)  # stessi limiti e controlli del formato
        segments, _ = self.model.transcribe(io.BytesIO(data), language=language, vad_filter=True, beam_size=1)
        return " ".join(s.text.strip() for s in segments).strip()


class PiperTTS:
    def __init__(self, env: dict[str, str]):
        from piper import PiperVoice  # import pigro: dipendenza opzionale

        path = env.get("JARVIS_TTS_VOICE", "")
        if not path or not Path(path).is_file():
            raise VoiceError("voice.model_missing", "voce Piper non trovata: imposta JARVIS_TTS_VOICE con il percorso del file .onnx")
        self.piper = PiperVoice.load(path)
        self.voice = Path(path).stem
        self.rate = int(getattr(getattr(self.piper, "config", None), "sample_rate", 22050))
        self.info = {"name": "piper", "model": self.voice, "location": "local"}

    def synthesize(self, text: str) -> bytes:
        buf = io.BytesIO()
        with wave.open(buf, "wb") as wav:
            self.piper.synthesize_wav(text, wav)
        return buf.getvalue()


class OpenWakeWord:
    """Rilevamento di «hey Jarvis» sull'audio a 16 kHz, a blocchi di 80 ms."""

    def __init__(self, env: dict[str, str]):
        import openwakeword  # import pigro: dipendenza opzionale
        from openwakeword.model import Model

        import inspect

        wanted = env.get("JARVIS_WAKE_MODEL", "hey_jarvis")
        # Nome di un modello incluso nel pacchetto (0.4 include «hey_jarvis») oppure percorso di un file .onnx.
        registry = getattr(openwakeword, "MODELS", None) or getattr(openwakeword, "models", {})
        path = wanted if wanted.endswith(".onnx") else registry.get(wanted, {}).get("model_path", "")
        if not path or not Path(path).is_file():
            raise VoiceError("voice.model_missing", f"modello di attivazione «{wanted}» non trovato")
        # 0.4 usa `wakeword_model_paths`; 0.6 usa `wakeword_models`. Un argomento sbagliato verrebbe
        # ignorato (**kwargs) e caricherebbe tutti i modelli: si sceglie quello dichiarato dalla firma.
        params = inspect.signature(Model.__init__).parameters
        key = "wakeword_models" if "wakeword_models" in params else "wakeword_model_paths"
        self.model = Model(**{key: [path]})
        if len(getattr(self.model, "models", {}) or {}) not in (0, 1):
            raise VoiceError("voice.model_missing", "openwakeword ha caricato più modelli del previsto")
        self.threshold = float(env.get("JARVIS_WAKE_THRESHOLD", "0.5"))
        self.name = f"openwakeword:{Path(path).stem}"

    def score(self, samples: list[int], rate: int) -> float:
        import numpy as np

        audio = np.array(resample(samples, rate, 16000), dtype=np.int16)
        best = 0.0
        for start in range(0, max(len(audio) - 1280, 1), 1280):
            prediction = self.model.predict(audio[start:start + 1280])
            best = max(best, max((float(v) for v in prediction.values()), default=0.0))
        self.model.reset()
        return float(best)


# ---------------------------------------------------------------------- servizio


class VoiceService:
    def __init__(self, env: dict[str, str] | None = None):
        self.env = dict(os.environ if env is None else env)
        mode = self.env.get("JARVIS_VOICE_ENGINE", "auto")
        self.language = self.env.get("JARVIS_VOICE_LANG", "it")
        self.stt: Any = None
        self.tts: Any = None
        self.wake: Any = None
        self.reasons: list[str] = []
        if mode == "off":
            self.reasons.append("voce disattivata (JARVIS_VOICE_ENGINE=off)")
        elif mode == "fake":
            self.stt, self.tts = FakeSTT(self.env), FakeTTS()
        else:
            for attr, factory, label in (("stt", WhisperSTT, "STT faster-whisper"), ("tts", PiperTTS, "TTS Piper")):
                try:
                    setattr(self, attr, factory(self.env))
                except ImportError:
                    self.reasons.append(f"{label} non installato (./jarvis.sh voice-setup)")
                except VoiceError as exc:
                    self.reasons.append(str(exc))
                except Exception as exc:  # noqa: BLE001 — modello assente o corrotto: la voce resta disattivata, il resto funziona
                    self.reasons.append(f"{label} non disponibile: {exc.__class__.__name__}: {str(exc)[:200]}")
            try:
                self.wake = OpenWakeWord(self.env)
            except Exception:  # noqa: BLE001 — opzionale: si usa il riconoscimento della parola nella trascrizione
                self.wake = None

    def status(self) -> dict:
        return {
            "stt": self.stt.info if self.stt else None,
            "tts": {**self.tts.info, "voice": self.tts.voice} if self.tts else None,
            "wake": self.wake.name if self.wake else ("parola «Jarvis» nella trascrizione" if self.stt else None),
            "language": self.language,
            "available": bool(self.stt),
            "reasons": self.reasons,
        }

    def transcribe(self, audio_b64: str, mode: str) -> dict:
        if not self.stt:
            raise VoiceError("voice.unavailable", "; ".join(self.reasons) or "riconoscimento vocale non disponibile")
        try:
            data = base64.b64decode(audio_b64, validate=True)
        except ValueError as exc:
            raise VoiceError("voice.invalid_audio", "audio non in base64") from exc
        samples, rate, _ = read_wav(data)
        text = self.stt.transcribe(data, self.language).strip()[:8000]
        wake: dict[str, Any] = {"mode": mode, "detected": False}
        if mode == "wake_word":
            match = WAKE_RE.match(text)
            if self.wake:
                score = self.wake.score(samples, rate)
                wake.update(engine=self.wake.name, score=round(min(max(score, 0.0), 1.0), 3), detected=score >= self.wake.threshold)
            else:
                wake.update(engine="trascrizione", detected=bool(match))
            if wake["detected"]:
                wake["command"] = (text[match.end():] if match else text).strip()
        return {
            "contract": "jarvis.voice-transcription/1",
            "text": text,
            "language": self.language,
            "duration_ms": int(1000 * len(samples) / max(rate, 1)),
            "engine": self.stt.info,
            "wake": wake,
        }

    def synthesize(self, text: str) -> dict:
        if not self.tts:
            raise VoiceError("voice.unavailable", "; ".join(self.reasons) or "sintesi vocale non disponibile")
        text = text.strip()
        if not text:
            raise VoiceError("voice.invalid_text", "testo vuoto")
        if len(text) > MAX_TTS_CHARS:
            raise VoiceError("voice.too_long", f"testo oltre {MAX_TTS_CHARS} caratteri")
        audio = self.tts.synthesize(text)
        samples, rate, _ = read_wav(audio) if len(audio) <= MAX_BYTES else ([], self.tts.rate, {})
        return {
            "contract": "jarvis.voice-synthesis/1",
            "audio_format": "wav",
            "sample_rate": rate,
            "duration_ms": int(1000 * len(samples) / max(rate, 1)),
            "engine": self.tts.info,
            "voice": self.tts.voice,
            "text_chars": len(text),
            "audio_b64": base64.b64encode(audio).decode("ascii"),
        }
