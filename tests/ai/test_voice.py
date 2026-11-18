"""Test della voce locale (RF-09): python3 -m unittest discover -s tests/ai -v

I motori finti sono sempre provati. openWakeWord reale viene provato se il pacchetto è installato
(include il modello «hey_jarvis»); faster-whisper e Piper richiedono modelli non disponibili in CI.
"""

from __future__ import annotations

import base64
import json
import math
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "ai"))

from jarvis_ai.contracts import validation_errors  # noqa: E402
from jarvis_ai.voice import VoiceError, VoiceService, read_wav, write_wav  # noqa: E402
from jarvis_ai.worker import handle_line, assistant_handler  # noqa: E402

FIXTURES = ROOT / "tests" / "contracts" / "fixtures" / "valid"


def tone(seconds=1.0, amp=4000, rate=16000):
    return [int(amp * math.sin(2 * math.pi * 440 * i / rate)) for i in range(int(rate * seconds))]


def b64(data: bytes) -> str:
    return base64.b64encode(data).decode()


class FakeEngines(unittest.TestCase):
    def setUp(self):
        self.v = VoiceService({"JARVIS_VOICE_ENGINE": "fake", "JARVIS_FAKE_STT_TEXT": "testo di prova"})

    def test_transcription_contract_and_marker(self):
        r = self.v.transcribe(b64(write_wav(tone(), 16000, {"jtxt": "leggi note.txt".encode()})), "push_to_talk")
        self.assertEqual(validation_errors("voice-transcription", r), ([], []))
        self.assertEqual(r["text"], "leggi note.txt")
        self.assertNotIn("audio_b64", r, "l'audio non viene restituito né conservato")

    def test_default_text_and_silence(self):
        self.assertEqual(self.v.transcribe(b64(write_wav(tone(), 16000)), "push_to_talk")["text"], "testo di prova")
        self.assertEqual(self.v.transcribe(b64(write_wav([0] * 8000, 16000)), "push_to_talk")["text"], "")

    def test_wake_word_by_transcript(self):
        for phrase, command in [("Jarvis, che ore sono", "che ore sono"), ("ehi jarvis info sistema", "info sistema"), ("Hey Jarvis! cerca rust", "cerca rust")]:
            r = self.v.transcribe(b64(write_wav(tone(), 16000, {"jtxt": phrase.encode()})), "wake_word")
            self.assertEqual((r["wake"]["detected"], r["wake"]["command"]), (True, command), phrase)
        r = self.v.transcribe(b64(write_wav(tone(), 16000, {"jtxt": "il giardino di Jarvis".encode()})), "wake_word")
        self.assertFalse(r["wake"]["detected"], "la parola deve aprire la frase")

    def test_synthesis_produces_valid_wav(self):
        r = self.v.synthesize("Ciao, sono JARVIS.")
        self.assertEqual(validation_errors("voice-synthesis", r), ([], []))
        samples, rate, _ = read_wav(base64.b64decode(r["audio_b64"]))
        self.assertEqual(rate, r["sample_rate"])
        self.assertGreater(len(samples), 0)

    def test_limits_and_errors(self):
        with self.assertRaises(VoiceError) as e:
            self.v.transcribe(b64(b"non wav"), "push_to_talk")
        self.assertEqual(e.exception.code, "voice.invalid_audio")
        with self.assertRaises(VoiceError) as e:
            self.v.transcribe(b64(write_wav([0] * (16000 * 61), 16000)), "push_to_talk")
        self.assertEqual(e.exception.code, "voice.too_long")
        with self.assertRaises(VoiceError):
            self.v.synthesize("x" * 4001)
        with self.assertRaises(VoiceError):
            self.v.synthesize("   ")

    def test_stereo_is_downmixed(self):
        import io, struct, wave
        buf = io.BytesIO()
        with wave.open(buf, "wb") as w:
            w.setnchannels(2); w.setsampwidth(2); w.setframerate(16000)
            w.writeframes(struct.pack("<4h", 1000, 3000, -1000, -3000))
        samples, _, _ = read_wav(buf.getvalue())
        self.assertEqual(samples, [2000, -2000])


class Availability(unittest.TestCase):
    def test_off_and_missing_engines_explain_why(self):
        off = VoiceService({"JARVIS_VOICE_ENGINE": "off"}).status()
        self.assertFalse(off["available"])
        self.assertIn("disattivata", off["reasons"][0])
        with self.assertRaises(VoiceError) as e:
            VoiceService({"JARVIS_VOICE_ENGINE": "off"}).transcribe("", "push_to_talk")
        self.assertEqual(e.exception.code, "voice.unavailable")

    def test_local_without_models_is_unavailable_but_harmless(self):
        status = VoiceService({"JARVIS_VOICE_ENGINE": "auto"}).status()
        if status["available"]:
            self.skipTest("motori reali con modelli presenti")
        self.assertTrue(status["reasons"])


class WorkerProtocol(unittest.TestCase):
    def envelope(self, kind, payload):
        env = json.loads((FIXTURES / "envelope.delegation-request.json").read_text(encoding="utf-8"))
        env.update(type=kind, payload=payload)
        return json.dumps(env)

    def test_voice_messages_through_the_bridge(self):
        import jarvis_ai.worker as worker

        worker._VOICE = VoiceService({"JARVIS_VOICE_ENGINE": "fake"})
        ok = handle_line(self.envelope("voice.transcribe", {"audio_b64": b64(write_wav(tone(), 16000, {"jtxt": b"ciao"})), "mode": "push_to_talk"}), assistant_handler)
        self.assertEqual((ok["status"], ok["type"], ok["payload"]["text"]), ("succeeded", "voice.result", "ciao"))
        self.assertEqual(validation_errors("envelope", ok), ([], []))
        bad = handle_line(self.envelope("voice.transcribe", {"audio_b64": "%%%"}), assistant_handler)
        self.assertEqual((bad["status"], bad["error"]["code"]), ("failed", "voice.invalid_audio"))
        worker._VOICE = None


class RealWakeWord(unittest.TestCase):
    def test_openwakeword_rejects_non_speech(self):
        try:
            from jarvis_ai.voice import OpenWakeWord
            w = OpenWakeWord({})
        except Exception as exc:  # noqa: BLE001
            self.skipTest(f"openwakeword non disponibile: {exc}")
        for audio in ([0] * 32000, tone(2.0, 8000)):
            self.assertLess(w.score(audio, 16000), w.threshold)


if __name__ == "__main__":
    unittest.main()


class Visemes(unittest.TestCase):
    """Labiale (ADR-0029): visemi dal testo italiano, allineati a durata e silenzi dell'audio."""

    def test_italian_text_becomes_mouth_shapes(self):
        from jarvis_ai import visemes

        seq = [v for v, _ in visemes.sequence("Mamma, ciao!")]
        self.assertEqual(seq[0], "M", "labbra chiuse per la m")
        self.assertIn("X", seq, "pausa alla virgola")
        self.assertEqual(seq[-1], "O")
        self.assertEqual(visemes.sequence("   ...  "), [])

    def test_timeline_fills_the_audio_and_lands_pauses_on_silence(self):
        import math

        from jarvis_ai import visemes

        rate = 16000
        tone = [int(6000 * math.sin(i / 5)) for i in range(rate)]
        samples = [0] * (rate // 5) + tone + [0] * rate + tone
        items = visemes.timeline("Ciao. Sono qui", samples, rate)
        self.assertEqual(items[0]["t"], 200, "il parlato inizia dopo il silenzio iniziale")
        pause = next(i for i in items if i["v"] == "X" and i["d"] > 500)
        self.assertEqual((pause["t"], pause["d"]), (1200, 1000), "la pausa del punto cade sul silenzio")
        self.assertLessEqual(items[-1]["t"] + items[-1]["d"], 3200)
        self.assertTrue(all(b["t"] >= a["t"] for a, b in zip(items, items[1:])), "tempi crescenti")

    def test_synthesis_result_carries_valid_visemes(self):
        from jarvis_ai.contracts import validation_errors
        from jarvis_ai.voice import VoiceService

        out = VoiceService({"JARVIS_VOICE_ENGINE": "fake"}).synthesize("Buongiorno, sono JARVIS.")
        self.assertEqual(validation_errors("voice-synthesis", out), ([], []))
        self.assertGreater(len(out["visemes"]), 5)
