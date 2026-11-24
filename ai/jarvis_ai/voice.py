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
import logging
import re
import struct
import wave
from pathlib import Path
from typing import Any

from jarvis_ai import visemes

MAX_SECONDS = 60
MAX_BYTES = 4 * 1024 * 1024
MAX_TTS_CHARS = 4000
# «Jarvis» e le sue trascrizioni più frequenti: i riconoscitori italiani spesso scrivono «Giarvis» o «Garvis».
WAKE_RE = re.compile(
    r"^\s*(?:(?:ehi|ehy|hey|ok|ciao)\s*,?\s*)?(?:jarvis|giarvis|garvis|jervis|yarvis|charvis|giarvi|jarvi)\b[\s,.:!?-]*",
    re.I,
)

# Solo voci maschili (ADR-0123). Il nome del parlante è nel nome del file Piper: «it_IT-riccardo-x_low».
MALE_SPEAKERS = {
    "riccardo", "ryan", "alan", "joe", "john", "norman", "danny", "kusal", "bryce", "carlfm", "hfc_male",
    "northern_english_male", "thorsten", "pavoque", "karlsson", "gilles", "mls",
}
FEMALE_SPEAKERS = {
    "paola", "lessac", "amy", "kathleen", "ljspeech", "jenny", "alba", "cori", "hfc_female", "southern_english_female",
    "aru", "lisa", "eva", "siwis", "mirjam", "kerstin", "ramona", "faber",
}


def voice_is_male(stem: str, env: dict[str, str]) -> bool:
    """Vero se la voce è maschile: nome noto, oppure dichiarata tale con JARVIS_TTS_VOICE_GENDER=male."""
    parts = set(re.split(r"[-_.\s]+", stem.lower())) | {stem.lower()}
    if parts & FEMALE_SPEAKERS:
        return False
    if parts & MALE_SPEAKERS:
        return True
    return env.get("JARVIS_TTS_VOICE_GENDER", "").strip().lower() == "male"


def not_male(name: str) -> "VoiceError":
    return VoiceError(
        "voice.not_male",
        f"La voce «{name}» non è maschile: JARVIS usa solo voci maschili. Esegui ./jarvis.sh voice-setup per scaricare «it_IT-riccardo-x_low».",
    )


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


# Guida il modello verso l'italiano dei comandi (ortografia, punteggiatura e parole tecniche).
INITIAL_PROMPT = (
    "Comandi a JARVIS, l'assistente personale. Esempi: «Jarvis, ricordami tra venti minuti di chiamare Marco.» "
    "«Apri l'app Spotify.» «Che ore sono?» «Scrivi una nota: comprare il latte.» «Avvia una missione.»"
)


class WhisperSTT:
    def __init__(self, env: dict[str, str]):
        os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
        from faster_whisper import WhisperModel  # import pigro: dipendenza opzionale

        self.model_name = env.get("JARVIS_STT_MODEL", "small")
        root = env.get("JARVIS_MODELS_DIR") or None
        self.model = WhisperModel(self.model_name, device="cpu", compute_type="int8", download_root=root,
                                  local_files_only=env.get("JARVIS_VOICE_ALLOW_DOWNLOAD") != "1")
        self.info = {"name": "faster-whisper", "model": self.model_name, "location": "local"}

    def transcribe(self, data: bytes, language: str) -> str:
        import numpy as np

        samples, rate, _ = read_wav(data)
        if not samples or rms(samples) < 45 or speech_ratio(samples, rate) < 0.04:
            return ""
        samples = preprocess(samples, rate)
        audio = np.asarray(samples, dtype=np.float32) / 32768.0
        if rate != 16000:
            positions = np.arange(len(samples) * 16000 // rate) * (rate / 16000.0)
            audio = np.interp(positions, np.arange(len(samples)), audio).astype(np.float32)
        segments, _ = self.model.transcribe(
            audio, language=language, vad_filter=True, beam_size=5,
            condition_on_previous_text=False, temperature=0,
            no_speech_threshold=0.6, log_prob_threshold=-1.0, compression_ratio_threshold=2.4,
            initial_prompt=INITIAL_PROMPT if language == "it" else None,
            vad_parameters={"min_silence_duration_ms": 350, "speech_pad_ms": 180},
            hotwords="JARVIS, GitHub, Python, JavaScript, macOS, Ollama, NVIDIA NIM, sandbox, missione, conferma, annulla, ricordami, promemoria",
        )
        text = " ".join(
            s.text.strip() for s in segments if s.no_speech_prob < 0.7 and getattr(s, "avg_logprob", 0.0) > -1.3
        ).strip()
        return strip_hallucinations(text)


_UNITS = {
    "zero": 0, "uno": 1, "una": 1, "un": 1, "due": 2, "tre": 3, "quattro": 4, "cinque": 5, "sei": 6, "sette": 7,
    "otto": 8, "nove": 9, "dieci": 10, "undici": 11, "dodici": 12, "tredici": 13, "quattordici": 14,
    "quindici": 15, "sedici": 16, "diciassette": 17, "diciotto": 18, "diciannove": 19,
}
_TENS = {"venti": 20, "trenta": 30, "quaranta": 40, "cinquanta": 50, "sessanta": 60, "settanta": 70, "ottanta": 80, "novanta": 90}
_COUNTED = r"(?:minut[oi]|second[oi]|or[ae]|giorn[oi]|settiman[ae]|mes[ei]|ann[oi]|euro|gradi|per cento|percento|chilometri|metri|chili|litri|gramm[oi]|pagine|righe|volte)"


def parse_it_number(word: str) -> int | None:
    """Numero italiano scritto in una sola parola, fino a 999 («trentacinque», «centocinquanta», «duecentoventi»).

    «un», «uno» e «una» restano articoli: non sono mai convertiti.
    """
    rest = word.lower()
    if rest in ("un", "uno", "una", ""):
        return None
    total = 0
    m = re.match(r"^(?:(due|tre|quattro|cinque|sei|sette|otto|nove))?cento(.*)$", rest)
    if m:
        total = (_UNITS[m.group(1)] if m.group(1) else 1) * 100
        rest = m.group(2)
        if not rest:
            return total
    if rest in _UNITS and rest not in ("un", "una"):
        return total + _UNITS[rest]
    for tens, value in _TENS.items():
        if rest == tens:
            return total + value
        stem = tens[:-1]
        if rest.startswith(stem):
            tail = rest[len(stem):]
            for unit_word in ("uno", "due", "tre", "quattro", "cinque", "sei", "sette", "otto", "nove"):
                if tail in (unit_word, "a" + unit_word):
                    return total + value + _UNITS[unit_word]
    return None


def normalize_command(text: str, language: str) -> str:
    """Riduce a cifre i numeri scritti a parole davanti a un'unità («tra venti minuti» → «tra 20 minuti»)."""
    if language != "it":
        return text
    pattern = re.compile(rf"\b([A-Za-zàèéìòù]+)\s+({_COUNTED})\b", re.I)

    def repl(m: re.Match) -> str:
        n = parse_it_number(m.group(1))
        return f"{n} {m.group(2)}" if n is not None and n > 1 else m.group(0)

    return pattern.sub(repl, text)


_STOP = {"stop", "basta", "zitto", "silenzio", "fermati", "ferma", "taci", "smettila"}
_REPEAT = {"ripeti", "ripetilo", "ripeti per favore", "puoi ripetere", "ripeti l'ultima risposta", "come hai detto"}
_CANCEL = {"annulla", "lascia perdere", "non importa", "dimentica"}


def voice_intent(text: str) -> str | None:
    """Comandi di controllo della sola voce, riconosciuti solo se sono l'intera frase."""
    t = re.sub(r"[^\w' ]+", "", text.lower()).strip()
    t = re.sub(r"\b(jarvis|giarvis|garvis)\b", "", t).strip()
    t = re.sub(r"^(?:per favore|grazie|ok)\s+|\s+(?:per favore|grazie)$", "", t).strip()
    if t in _STOP:
        return "stop"
    if t in _REPEAT:
        return "repeat"
    if t in _CANCEL:
        return "cancel"
    return None


_ABBREV = (
    (r"https?://\S+", " link "),
    (r"\b\d{1,2}\s?%", lambda m: m.group(0).replace("%", "").strip() + " per cento"),
    (r"%", " per cento"),
    (r"€\s?(\d[\d.,]*)", r"\1 euro"),
    (r"(\d[\d.,]*)\s?€", r"\1 euro"),
    (r"(\d+)\s?°\s?C\b", r"\1 gradi"),
    (r"(\d+)\s?°", r"\1 gradi"),
    (r"\bkm/h\b", "chilometri orari"),
    (r"\bkm\b", "chilometri"),
    (r"\bGB\b", "gigabyte"),
    (r"\bMB\b", "megabyte"),
    (r"\bGHz\b", "gigahertz"),
    (r"&", " e "),
)


def speech_text(text: str) -> str:
    """Prepara un testo per la voce: toglie markdown, codice, indirizzi ed emoji e legge simboli e unità."""
    text = re.sub(r"```.*?```", " Ecco il codice nella finestra. ", text, flags=re.S)
    text = re.sub(r"`([^`]*)`", r"\1", text)
    text = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"[*_~]{1,3}([^*_~\n]+)[*_~]{1,3}", r"\1", text)
    text = re.sub(r"^\s*(?:#{1,6}|>|[-*+])\s+", "", text, flags=re.M)
    text = re.sub(r"[\U0001F300-\U0001FAFF\u2600-\u27BF\uFE0F]", "", text)
    for pattern, replacement in _ABBREV:
        text = re.sub(pattern, replacement, text)
    text = re.sub(r"\s*\n+\s*", ". ", text)
    text = re.sub(r"([.!?])\s*\.", r"\1", text)
    return re.sub(r"[ \t]+", " ", text).strip()


def polish(samples: list[int], rate: int) -> list[int]:
    """Livella il volume (picco a −1 dBFS), dissolvenze di 25 ms e niente silenzio in coda."""
    if not samples:
        return samples
    end = len(samples)
    floor = 200
    while end > 1 and abs(samples[end - 1]) < floor and len(samples) - end < rate * 2:
        end -= 1
    samples = samples[: min(len(samples), end + int(rate * 0.12))]
    peak = max(abs(s) for s in samples) or 1
    gain = min(8.0, 29000 / peak)
    out = [int(max(-32768, min(32767, s * gain))) for s in samples]
    fade = min(int(rate * 0.025), len(out) // 2)
    for i in range(fade):
        k = i / fade
        out[i] = int(out[i] * k)
        out[-1 - i] = int(out[-1 - i] * k)
    return out


# ---------------------------------------------------------------------- qualità dell'audio e del testo (ADR-0127)


def preprocess(samples: list[int], rate: int) -> list[int]:
    """Prepara l'audio per il riconoscimento: toglie la componente continua e i rumori sotto ~80 Hz e livella il volume."""
    if not samples:
        return samples
    mean = sum(samples) / len(samples)
    alpha = 1.0 - math.exp(-2.0 * math.pi * 80.0 / max(rate, 1))
    low = 0.0
    out: list[float] = []
    for s in samples:
        x = s - mean
        low += alpha * (x - low)
        out.append(x - low)
    peak = max(abs(v) for v in out) or 1.0
    gain = min(40.0, 26000.0 / peak)
    return [int(max(-32768, min(32767, v * gain))) for v in out]


def speech_ratio(samples: list[int], rate: int) -> float:
    """Frazione di blocchi da 20 ms con voce, rispetto al rumore di fondo stimato dai blocchi più silenziosi."""
    size = max(int(rate * 0.02), 1)
    frames = [rms(samples[i:i + size]) for i in range(0, len(samples) - size + 1, size)]
    if not frames:
        return 0.0
    floor = sorted(frames)[len(frames) // 5]
    # Il limite superiore evita che un suono costante e forte (voce continua) sembri rumore di fondo.
    threshold = max(250.0, min(floor * 2.8, 1500.0))
    return sum(1 for f in frames if f > threshold) / len(frames)


# Frasi che Whisper inventa sul silenzio o sul rumore (note dai sottotitoli con cui è stato addestrato).
_HALLUCINATIONS = (
    "sottotitoli creati dalla comunita amara.org", "sottotitoli e revisione a cura di qtss",
    "sottotitoli a cura di qtss", "grazie per la visione", "grazie per l'attenzione", "iscriviti al canale",
    "alla prossima", "ci vediamo al prossimo video", "grazie a tutti", "buona visione", "musica", "applausi",
)


def strip_hallucinations(text: str) -> str:
    """Elimina le frasi inventate dal modello sul silenzio e i cicli di parole ripetute."""
    plain = re.sub(r"[^\w' .]+", "", text.lower().replace("à", "a")).strip(" .")
    if any(plain == h.strip(" .") for h in _HALLUCINATIONS):
        return ""
    for h in _HALLUCINATIONS[:4]:
        text = re.sub(re.escape(h), "", text, flags=re.I).strip()
    # «ciao ciao ciao ciao» → «ciao»; «grazie. grazie. grazie.» → «grazie.»
    text = re.sub(r"\b(\w+[.,!?]?)(\s+\1){3,}", r"\1", text, flags=re.I)
    return text.strip()


def wake_decision(score: float | None, threshold: float, transcript_match: bool, speech: float) -> tuple[bool, str]:
    """Decide se è la parola di attivazione. Il solo punteggio basta se alto; uno medio richiede la conferma della
    trascrizione; rumore senza voce non attiva mai."""
    if speech < 0.08:
        return False, ""
    if score is None:
        return transcript_match, "transcript" if transcript_match else ""
    if score >= threshold:
        return True, "score+transcript" if transcript_match else "score"
    if score >= threshold * 0.65 and transcript_match:
        return True, "score+transcript"
    return False, ""


_PAUSES = {".": 0.30, "!": 0.30, "?": 0.34, ":": 0.22, ";": 0.22}


def split_sentences(text: str) -> list[tuple[str, float]]:
    """Frasi con la pausa da inserire dopo ciascuna: una sintesi frase per frase dà un ritmo più naturale."""
    parts = re.findall(r"[^.!?;:]+[.!?;:]?", text)
    out: list[tuple[str, float]] = []
    for part in parts:
        part = part.strip()
        if not part or not re.search(r"\w", part):
            continue
        out.append((part, _PAUSES.get(part[-1], 0.18)))
    # frasi molto brevi si uniscono alla successiva per non spezzare il discorso
    merged: list[tuple[str, float]] = []
    for sentence, pause in out:
        if merged and len(merged[-1][0]) < 14:
            merged[-1] = (merged[-1][0] + " " + sentence, pause)
        else:
            merged.append((sentence, pause))
    return merged[:14]


def correct_transcript(text: str, language: str) -> str:
    """Corregge solo locuzioni italiane note; lascia comandi, codice e punteggiatura tecnica intatti."""
    if language != "it" or re.search(r"[`/:\\_@=]|\.\w{1,8}\b", text):
        return text
    for pattern, replacement in (
        (r"\bqual['’]è\b", "qual è"),
        (r"\bqual é\b", "qual è"),
        (r"\bun pò\b", "un po’"),
        (r"\bperchè\b", "perché"),
    ):
        text = re.sub(pattern, lambda m: replacement.capitalize() if m[0][0].isupper() else replacement, text, flags=re.I)
    return text


class PiperTTS:
    def __init__(self, env: dict[str, str]):
        import onnxruntime
        from piper import PiperVoice  # import pigro: dipendenza opzionale

        onnxruntime.disable_telemetry_events()

        path = env.get("JARVIS_TTS_VOICE", "")
        if not path or not Path(path).is_file():
            raise VoiceError("voice.model_missing", "voce Piper non trovata: imposta JARVIS_TTS_VOICE con il percorso del file .onnx")
        if not voice_is_male(Path(path).stem, env):
            raise not_male(Path(path).stem)
        self.env = env
        self.piper = PiperVoice.load(path)
        self.voice = Path(path).stem
        self.rate = int(getattr(getattr(self.piper, "config", None), "sample_rate", 22050))
        self.info = {"name": "piper", "model": self.voice, "location": "local"}

    def synthesize(self, text: str) -> bytes:
        buf = io.BytesIO()
        with wave.open(buf, "wb") as wav:
            config = None
            try:
                from piper import SynthesisConfig  # versioni recenti di Piper

                scale = min(1.3, max(0.85, float(self.env.get("JARVIS_TTS_LENGTH_SCALE", "1.06"))))
                config = SynthesisConfig(length_scale=scale, noise_scale=0.62, noise_w_scale=0.75)
            except (ImportError, TypeError, ValueError):
                config = None
            if config is not None:
                try:
                    self.piper.synthesize_wav(text, wav, syn_config=config)
                    return buf.getvalue()
                except TypeError:
                    pass
            self.piper.synthesize_wav(text, wav)
        return buf.getvalue()


class XttsTTS:
    """XTTS v2 locale, pesi e riferimento scelti dal proprietario, senza download impliciti."""

    info = {"name": "xtts", "model": "xtts-v2", "location": "local"}
    voice = "xtts-reference"
    rate = 24000

    def __init__(self, env: dict[str, str]):
        self.directory = Path(env.get("JARVIS_XTTS_DIR", ""))
        self.reference = Path(env.get("JARVIS_XTTS_REFERENCE", ""))
        if env.get("JARVIS_TTS_VOICE_GENDER", "").strip().lower() != "male":
            raise not_male("XTTS")
        self.language = env.get("JARVIS_VOICE_LANG", "it")
        speed = float(env.get("JARVIS_XTTS_SPEED", "1.08"))
        self.speed = min(1.3, max(0.8, speed))
        if not self.reference.is_file() or not all(
            (self.directory / name).is_file()
            for name in ("config.json", "model.pth", "vocab.json")
        ):
            raise VoiceError("voice.model_missing", "XTTS v2 richiede pesi locali e un campione WAV")
        with self.reference.open("rb") as audio:
            samples, rate, _ = read_wav(audio.read(MAX_BYTES + 1))
        if not 3 <= len(samples) / rate <= 30 or rms(samples) < 100:
            raise VoiceError("voice.invalid_audio", "Il campione XTTS deve contenere da 3 a 30 secondi di voce")
        self.model = None
        self.conditioning = None

    def synthesize(self, text: str) -> bytes:
        import torch
        from TTS.tts.configs.xtts_config import XttsConfig
        from TTS.tts.models.xtts import Xtts

        with torch.inference_mode():
            if self.model is None:
                config = XttsConfig()
                config.load_json(str(self.directory / "config.json"))
                model = Xtts(config)
                model.load_checkpoint(config, checkpoint_dir=str(self.directory), eval=True)
                if torch.cuda.is_available():
                    model.cuda()
                conditioning = model.get_conditioning_latents(audio_path=[str(self.reference)])
                self.model, self.conditioning = model, conditioning
            result = self.model.inference(
                text, self.language, *self.conditioning, speed=self.speed, enable_text_splitting=True,
            )
        samples = [int(max(-1.0, min(1.0, float(sample))) * 32767) for sample in result["wav"]]
        return write_wav(samples, self.rate)


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
        self.fallback_tts: Any = None
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
            if self.env.get("JARVIS_XTTS_DIR") and self.env.get("JARVIS_XTTS_REFERENCE"):
                try:
                    self.fallback_tts = self.tts
                    self.tts = XttsTTS(self.env)
                except (OSError, ValueError, VoiceError):
                    self.reasons.append("Configurazione XTTS v2 non valida; usa una cartella modello e un riferimento WAV validi")
            try:
                self.wake = OpenWakeWord(self.env)
            except Exception:  # noqa: BLE001 — opzionale: si usa il riconoscimento della parola nella trascrizione
                self.wake = None

    def status(self) -> dict:
        return {
            "stt": self.stt.info if self.stt else None,
            "tts": {**self.tts.info, "voice": self.tts.voice, "gender": "male"} if self.tts else None,
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
        text = correct_transcript(text, self.language)
        wake: dict[str, Any] = {"mode": mode, "detected": False}
        # Il comando senza la parola di attivazione è quello che conta per intento e numeri.
        if mode == "wake_word":
            match = WAKE_RE.match(text)
            speech = speech_ratio(samples, rate)
            wake["speech_ratio"] = round(speech, 3)
            if self.wake:
                score = self.wake.score(samples, rate)
                detected, confirmed = wake_decision(score, self.wake.threshold, bool(match), speech)
                wake.update(engine=self.wake.name, score=round(min(max(score, 0.0), 1.0), 3), detected=detected)
            else:
                detected, confirmed = wake_decision(None, 0.5, bool(match), speech)
                wake.update(engine="trascrizione", detected=detected)
            if detected and confirmed:
                wake["confirmed_by"] = confirmed
            if wake["detected"]:
                wake["command"] = (text[match.end():] if match else text).strip()
        spoken = wake.get("command", text) if mode == "wake_word" else text
        result = {
            "contract": "jarvis.voice-transcription/1",
            "text": text,
            "language": self.language,
            "duration_ms": int(1000 * len(samples) / max(rate, 1)),
            "engine": self.stt.info,
            "wake": wake,
        }
        normalized = normalize_command(spoken, self.language)
        if normalized != spoken:
            result["normalized"] = normalized
        intent = voice_intent(spoken)
        if intent:
            result["intent"] = intent
        return result

    def _render(self, spoken: str, sentences: list[tuple[str, float]]) -> tuple[bytes, list[int], int]:
        """Audio del testo: frase per frase con pause naturali se è lungo, in un solo passaggio altrimenti."""
        if len(sentences) < 2:
            audio = self.tts.synthesize(spoken)
            samples, rate, _ = read_wav(audio) if len(audio) <= MAX_BYTES else ([], self.tts.rate, {})
            return audio, samples, rate
        joined: list[int] = []
        rate = self.tts.rate
        for sentence, pause in sentences:
            chunk, chunk_rate, _ = read_wav(self.tts.synthesize(sentence))
            if joined and chunk_rate != rate:
                chunk = resample(chunk, chunk_rate, rate)
            else:
                rate = chunk_rate
            joined.extend(chunk)
            joined.extend([0] * int(rate * pause))
        return write_wav(joined, rate), joined, rate

    def synthesize(self, text: str) -> dict:
        if not self.tts:
            raise VoiceError("voice.unavailable", "; ".join(self.reasons) or "sintesi vocale non disponibile")
        text = text.strip()
        if not text:
            raise VoiceError("voice.invalid_text", "testo vuoto")
        if len(text) > MAX_TTS_CHARS:
            raise VoiceError("voice.too_long", f"testo oltre {MAX_TTS_CHARS} caratteri")
        spoken = speech_text(text) or text
        sentences = split_sentences(spoken) if len(spoken) > 60 else []
        try:
            audio, samples, rate = self._render(spoken, sentences)
        except Exception as exc:
            if not isinstance(self.tts, XttsTTS):
                raise
            logging.getLogger(__name__).debug("XTTS non disponibile: %s", type(exc).__name__)
            self.reasons.append("XTTS v2 non disponibile; controlla che le dipendenze e il modello siano installati")
            self.tts = self.fallback_tts
            if self.tts is None:
                raise VoiceError("voice.unavailable", self.reasons[-1]) from None
            audio, samples, rate = self._render(spoken, sentences)
        if samples:
            samples = polish(samples, rate)
            audio = write_wav(samples, rate)
        return {
            "contract": "jarvis.voice-synthesis/1",
            "audio_format": "wav",
            "sample_rate": rate,
            "duration_ms": int(1000 * len(samples) / max(rate, 1)),
            "engine": self.tts.info,
            "voice": self.tts.voice,
            "text_chars": len(text),
            "audio_b64": base64.b64encode(audio).decode("ascii"),
            "visemes": visemes.timeline(text, samples, rate),
        }
