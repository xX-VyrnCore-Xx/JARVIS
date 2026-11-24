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
from jarvis_ai.voice import (  # noqa: E402
    VoiceError, VoiceService, correct_transcript, normalize_command, parse_it_number, polish, preprocess, read_wav,
    speech_ratio, speech_text, split_sentences, strip_hallucinations, wake_decision, voice_intent, voice_is_male,
    write_wav,
)
from jarvis_ai.worker import handle_line, assistant_handler  # noqa: E402

FIXTURES = ROOT / "tests" / "contracts" / "fixtures" / "valid"


def tone(seconds=1.0, amp=4000, rate=16000):
    return [int(amp * math.sin(2 * math.pi * 440 * i / rate)) for i in range(int(rate * seconds))]


def b64(data: bytes) -> str:
    return base64.b64encode(data).decode()


class TranscriptCorrectionTest(unittest.TestCase):
    def test_italian_corrections_preserve_code_and_paths(self):
        self.assertEqual(correct_transcript("Qual'è il prossimo passo e perchè", "it"), "Qual è il prossimo passo e perché")
        for text in ("Scrivi note.txt: qual'è", "un pò in src/main.py", "password=un pò"):
            self.assertEqual(correct_transcript(text, "it"), text)
        self.assertEqual(correct_transcript("qual é", "es"), "qual é")


class MaleVoiceTest(unittest.TestCase):
    def test_only_male_voices_are_accepted(self):
        for stem in ("it_IT-riccardo-x_low", "en_US-ryan-medium", "en_GB-alan-low"):
            self.assertTrue(voice_is_male(stem, {}), stem)
        for stem in ("it_IT-paola-medium", "en_US-lessac-medium", "en_US-amy-low"):
            self.assertFalse(voice_is_male(stem, {"JARVIS_TTS_VOICE_GENDER": "male"}), "una voce femminile nota non si può forzare")
        self.assertFalse(voice_is_male("voce-sconosciuta", {}))
        self.assertTrue(voice_is_male("voce-sconosciuta", {"JARVIS_TTS_VOICE_GENDER": "male"}))

    def test_a_female_voice_is_refused_with_an_explanation(self):
        import tempfile, os
        from jarvis_ai import voice
        with tempfile.TemporaryDirectory() as d:
            model = Path(d) / "it_IT-paola-medium.onnx"
            model.write_bytes(b"x")
            service = VoiceService({"JARVIS_VOICE_ENGINE": "local", "JARVIS_TTS_VOICE": str(model)})
            self.assertIsNone(service.tts)
            self.assertTrue(any("maschil" in r or "non installato" in r or "non disponibile" in r for r in service.reasons), service.reasons)
        err = voice.not_male("it_IT-paola-medium")
        self.assertEqual(err.code, "voice.not_male")
        self.assertIn("riccardo", str(err))

    def test_the_synthesis_status_declares_the_male_voice(self):
        v = VoiceService({"JARVIS_VOICE_ENGINE": "fake"})
        self.assertEqual(v.status()["tts"]["gender"], "male")


class CommandNormalizationTest(unittest.TestCase):
    def test_spoken_numbers_become_digits_before_units_only(self):
        self.assertEqual(normalize_command("ricordami tra venti minuti", "it"), "ricordami tra 20 minuti")
        self.assertEqual(normalize_command("fra trentacinque minuti", "it"), "fra 35 minuti")
        self.assertEqual(normalize_command("duecento euro", "it"), "200 euro")
        self.assertEqual(normalize_command("tra un minuto", "it"), "tra un minuto")
        self.assertEqual(normalize_command("una nota importante", "it"), "una nota importante")
        self.assertEqual(normalize_command("twenty minutes", "en"), "twenty minutes")
        for word, value in (("quarantuno", 41), ("ventotto", 28), ("centocinquanta", 150), ("duecentoventi", 220), ("dieci", 10)):
            self.assertEqual(parse_it_number(word), value, word)
        self.assertIsNone(parse_it_number("uno"))
        self.assertIsNone(parse_it_number("casa"))

    def test_control_intents_only_when_they_are_the_whole_phrase(self):
        for phrase in ("Stop", "basta!", "Jarvis, zitto", "silenzio per favore"):
            self.assertEqual(voice_intent(phrase), "stop", phrase)
        self.assertEqual(voice_intent("Ripeti"), "repeat")
        self.assertEqual(voice_intent("annulla"), "cancel")
        for phrase in ("non fermarti ora", "stop alla missione di domani", "ripeti il test python"):
            self.assertIsNone(voice_intent(phrase), phrase)

    def test_wake_word_survives_common_mis_transcriptions(self):
        v = VoiceService({"JARVIS_VOICE_ENGINE": "fake"})
        for phrase in ("Giarvis, che ore sono", "ehi Garvis che ore sono", "Jervis che ore sono"):
            r = v.transcribe(b64(write_wav(tone(), 16000, {"jtxt": phrase.encode()})), "wake_word")
            self.assertEqual((r["wake"]["detected"], r["wake"]["command"]), (True, "che ore sono"), phrase)

    def test_the_transcription_carries_normalization_and_intent(self):
        v = VoiceService({"JARVIS_VOICE_ENGINE": "fake"})
        r = v.transcribe(b64(write_wav(tone(), 16000, {"jtxt": "Jarvis ricordami tra venti minuti di chiamare".encode()})), "wake_word")
        self.assertEqual(r["normalized"], "ricordami tra 20 minuti di chiamare")
        self.assertEqual(validation_errors("voice-transcription", r), ([], []))
        r = v.transcribe(b64(write_wav(tone(), 16000, {"jtxt": "Jarvis, basta".encode()})), "wake_word")
        self.assertEqual(r["intent"], "stop")
        self.assertEqual(validation_errors("voice-transcription", r), ([], []))


class SpeechPreparationTest(unittest.TestCase):
    def test_markdown_code_links_and_symbols_are_spoken_naturally(self):
        text = "**Fatto.** Ho letto https://example.org/x e `main.rs`.\n\n```py\nprint(1)\n```\nCosta 12,50 € e fa 21 °C, batteria al 80%."
        spoken = speech_text(text)
        for forbidden in ("**", "https", "`", "print(1)", "€", "°", "%"):
            self.assertNotIn(forbidden, spoken, forbidden)
        for expected in ("12,50 euro", "21 gradi", "80 per cento", "codice"):
            self.assertIn(expected, spoken, expected)

    def test_polish_levels_fades_and_trims_the_tail(self):
        samples = [0] * 800 + [int(3000 * math.sin(i / 7)) for i in range(8000)] + [0] * 16000
        out = polish(samples, 16000)
        self.assertLess(len(out), len(samples))
        self.assertEqual(out[0], 0)
        self.assertGreater(max(abs(s) for s in out), 20000)
        self.assertLessEqual(max(abs(s) for s in out), 32767)
        self.assertEqual(polish([], 16000), [])


class RecognitionQualityTest(unittest.TestCase):
    def test_preprocess_removes_offset_and_levels_quiet_audio(self):
        quiet = [int(800 * math.sin(i / 5)) + 1500 for i in range(16000)]
        out = preprocess(quiet, 16000)
        self.assertLess(abs(sum(out) / len(out)), 200, "la componente continua è tolta")
        self.assertGreater(max(abs(s) for s in out), 15000, "il volume è livellato")
        self.assertEqual(preprocess([], 16000), [])

    def test_speech_ratio_tells_voice_from_noise_and_silence(self):
        import random
        random.seed(1)
        noise = [random.randint(-120, 120) for _ in range(16000)]
        voice = noise[:6000] + [int(4000 * math.sin(i / 6)) + n for i, n in enumerate(noise[6000:12000])] + noise[12000:]
        self.assertLess(speech_ratio(noise, 16000), 0.08)
        self.assertLess(speech_ratio([0] * 16000, 16000), 0.05)
        self.assertGreater(speech_ratio(voice, 16000), 0.25)

    def test_invented_phrases_and_loops_are_removed(self):
        self.assertEqual(strip_hallucinations("Sottotitoli creati dalla comunità Amara.org"), "")
        self.assertEqual(strip_hallucinations("Grazie a tutti."), "")
        self.assertEqual(strip_hallucinations("ciao ciao ciao ciao ciao"), "ciao")
        self.assertEqual(strip_hallucinations("Ricordami di chiamare Marco"), "Ricordami di chiamare Marco")
        self.assertEqual(strip_hallucinations("Grazie a tutti per il lavoro di oggi"), "Grazie a tutti per il lavoro di oggi")

    def test_wake_needs_voice_and_a_medium_score_needs_the_transcript(self):
        self.assertEqual(wake_decision(0.9, 0.5, False, 0.3), (True, "score"))
        self.assertEqual(wake_decision(0.9, 0.5, True, 0.3), (True, "score+transcript"))
        self.assertEqual(wake_decision(0.4, 0.5, True, 0.3), (True, "score+transcript"))
        self.assertEqual(wake_decision(0.4, 0.5, False, 0.3), (False, ""))
        self.assertEqual(wake_decision(0.2, 0.5, True, 0.3), (False, ""))
        self.assertEqual(wake_decision(0.95, 0.5, True, 0.02), (False, ""), "rumore senza voce non attiva")
        self.assertEqual(wake_decision(None, 0.5, True, 0.3), (True, "transcript"))
        self.assertEqual(wake_decision(None, 0.5, False, 0.3), (False, ""))

    def test_the_wake_result_explains_how_it_was_confirmed(self):
        v = VoiceService({"JARVIS_VOICE_ENGINE": "fake"})
        r = v.transcribe(b64(write_wav(tone(), 16000, {"jtxt": "Jarvis, che ore sono".encode()})), "wake_word")
        self.assertEqual(r["wake"]["confirmed_by"], "transcript")
        self.assertGreater(r["wake"]["speech_ratio"], 0.5)
        self.assertEqual(validation_errors("voice-transcription", r), ([], []))
        silent = v.transcribe(b64(write_wav([0] * 16000, 16000, {"jtxt": "Jarvis, che ore sono".encode()})), "wake_word")
        self.assertFalse(silent["wake"]["detected"], "senza voce non si attiva nemmeno con il testo")


class SentenceSynthesisTest(unittest.TestCase):
    def test_long_text_is_split_into_sentences_with_pauses(self):
        parts = split_sentences("Ho controllato il sistema. Tutto funziona? Sì, ma c'è un avviso: il disco è quasi pieno.")
        self.assertGreaterEqual(len(parts), 3)
        self.assertTrue(all(pause >= 0.18 for _, pause in parts))
        self.assertEqual(split_sentences("   ..."), [])
        short = split_sentences("Sì. Ok. Certamente, signore.")
        self.assertLess(len(short), 3, "le frasi brevissime si uniscono")

    def test_synthesis_of_long_text_has_pauses_between_sentences(self):
        v = VoiceService({"JARVIS_VOICE_ENGINE": "fake"})
        short = v.synthesize("Ciao a tutti e buon lavoro")
        long = v.synthesize("Ho controllato il sistema e tutto funziona. Il disco è quasi pieno, quindi conviene fare pulizia presto. Vuoi che ti aiuti?")
        self.assertEqual(validation_errors("voice-synthesis", long), ([], []))
        self.assertGreater(long["duration_ms"], short["duration_ms"])


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
