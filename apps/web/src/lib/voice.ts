// Voce (RF-09, ADR-0010): audio catturato nel browser, convertito in WAV 16 kHz e trascritto dal Core con
// motori locali. Le conferme non si danno a voce: restano un clic esplicito.

import { AUTH } from "./api";

export type VoiceState = "idle" | "listening" | "transcribing" | "speaking";

export interface VoiceStatus {
  available: boolean;
  stt: { name: string; model: string };
  tts: { name: string; voice: string } | null;
  wake?: string;
  reasons?: string[];
}

interface Handlers {
  onCommand: (text: string) => void;
  onError: (message: string) => void;
  onNothingHeard: () => void;
}

const SR_TARGET = 16000;

function encodeWav(float32: Float32Array, rate: number): Blob {
  const ratio = rate / SR_TARGET;
  const n = Math.floor(float32.length / ratio);
  const buf = new ArrayBuffer(44 + n * 2);
  const v = new DataView(buf);
  const str = (o: number, t: string) => {
    for (let i = 0; i < t.length; i++) v.setUint8(o + i, t.charCodeAt(i));
  };
  str(0, "RIFF");
  v.setUint32(4, 36 + n * 2, true);
  str(8, "WAVEfmt ");
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true);
  v.setUint16(22, 1, true);
  v.setUint32(24, SR_TARGET, true);
  v.setUint32(28, SR_TARGET * 2, true);
  v.setUint16(32, 2, true);
  v.setUint16(34, 16, true);
  str(36, "data");
  v.setUint32(40, n * 2, true);
  for (let i = 0; i < n; i++) {
    const x = Math.max(-1, Math.min(1, float32[Math.floor(i * ratio)]));
    v.setInt16(44 + i * 2, x < 0 ? x * 0x8000 : x * 0x7fff, true);
  }
  return new Blob([buf], { type: "audio/wav" });
}

const concat = (chunks: Float32Array[]) => {
  const out = new Float32Array(chunks.reduce((a, c) => a + c.length, 0));
  let o = 0;
  for (const c of chunks) {
    out.set(c, o);
    o += c.length;
  }
  return out;
};
const level = (frame: Float32Array) => Math.sqrt(frame.reduce((a, x) => a + x * x, 0) / frame.length);

/** Motore vocale unico per la pagina; i componenti si iscrivono ai cambi di stato. */
export class VoiceEngine {
  state: VoiceState = "idle";
  status: VoiceStatus | null = null;
  handsFree = false;
  replies = false;
  private audioCtx: AudioContext | null = null;
  private micStream: MediaStream | null = null;
  private pushChunks: Float32Array[] | null = null;
  private player: HTMLAudioElement | null = null;
  private speechRequest: AbortController | null = null;
  private speechFrame = 0;
  private vad = { speaking: false, chunks: [] as Float32Array[], loud: 0, quiet: 0, awaitingCommand: false };
  private listeners = new Set<() => void>();
  handlers: Handlers = { onCommand: () => {}, onError: () => {}, onNothingHeard: () => {} };

  subscribe(fn: () => void) {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }
  private emit() {
    this.listeners.forEach((fn) => fn());
  }

  get micOpen() {
    return !!this.micStream;
  }
  get pushing() {
    return !!this.pushChunks;
  }
  get available() {
    return !!this.status?.available;
  }

  setStatus(status: VoiceStatus | null) {
    this.status = status;
    this.emit();
  }

  private setState(s: VoiceState) {
    this.state = s;
    this.emit();
  }

  private async openMic() {
    if (this.micStream) return;
    this.micStream = await navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });
    this.audioCtx = new AudioContext();
    await this.audioCtx.audioWorklet.addModule("/voice-worklet.js");
    const source = this.audioCtx.createMediaStreamSource(this.micStream);
    const node = new AudioWorkletNode(this.audioCtx, "jarvis-capture");
    node.port.onmessage = (e) => this.onFrame(e.data as Float32Array);
    source.connect(node);
    this.emit();
  }

  private closeMic() {
    this.micStream?.getTracks().forEach((t) => t.stop());
    this.audioCtx?.close().catch(() => {});
    this.micStream = null;
    this.audioCtx = null;
    this.emit();
  }

  private onFrame(frame: Float32Array) {
    if (this.pushChunks) {
      this.pushChunks.push(frame);
      return;
    }
    if (!this.handsFree || !this.audioCtx) return;
    // Rilevamento del parlato per energia: inizio dopo ~170 ms sopra soglia, fine dopo ~0,8 s di silenzio.
    const vad = this.vad;
    const loud = level(frame) > 0.02;
    if (loud && this.state === "speaking") this.stopSpeaking(); // interruzione della risposta (barge-in)
    if (!vad.speaking) {
      vad.loud = loud ? vad.loud + 1 : 0;
      vad.chunks.push(frame);
      if (vad.chunks.length > 4) vad.chunks.shift();
      if (vad.loud >= 2) {
        vad.speaking = true;
        vad.quiet = 0;
        this.setState("listening");
      }
    } else {
      vad.chunks.push(frame);
      vad.quiet = loud ? 0 : vad.quiet + 1;
      const seconds = vad.chunks.reduce((a, c) => a + c.length, 0) / this.audioCtx.sampleRate;
      if (vad.quiet >= 9 || seconds > 15) {
        const audio = encodeWav(concat(vad.chunks), this.audioCtx.sampleRate);
        vad.speaking = false;
        vad.chunks = [];
        vad.loud = 0;
        void this.handleUtterance(audio);
      }
    }
  }

  private async transcribe(blob: Blob, wakeWord: boolean) {
    this.setState("transcribing");
    const res = await fetch(`/api/voice/transcribe${wakeWord ? "?mode=wake_word" : ""}`, {
      method: "POST",
      headers: { "Content-Type": "audio/wav", ...AUTH },
      body: blob,
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(data.error ? data.error.message : `HTTP ${res.status}`);
    return data as { text: string; wake: { detected: boolean; command?: string } };
  }

  private async handleUtterance(blob: Blob) {
    try {
      const t = await this.transcribe(blob, !this.vad.awaitingCommand);
      this.setState("idle");
      if (this.vad.awaitingCommand) {
        this.vad.awaitingCommand = false;
        if (t.text) this.handlers.onCommand(t.text);
        return;
      }
      if (!t.wake.detected) return; // parlato senza «Jarvis»: ignorato e non conservato
      if (t.wake.command) this.handlers.onCommand(t.wake.command);
      else {
        this.vad.awaitingCommand = true;
        this.setState("listening");
      }
    } catch (e) {
      this.setState("idle");
      this.handlers.onError(`Voce: ${(e as Error).message}`);
    }
  }

  async togglePushToTalk() {
    if (this.pushChunks && this.audioCtx) {
      const chunks = this.pushChunks;
      this.pushChunks = null;
      const audio = encodeWav(concat(chunks), this.audioCtx.sampleRate);
      if (!this.handsFree) this.closeMic();
      try {
        const t = await this.transcribe(audio, false);
        this.setState("idle");
        if (t.text) this.handlers.onCommand(t.text);
        else this.handlers.onNothingHeard();
      } catch (e) {
        this.setState("idle");
        this.handlers.onError(`Voce: ${(e as Error).message}`);
      }
      return;
    }
    this.stopSpeaking();
    try {
      await this.openMic();
      this.pushChunks = [];
      this.setState("listening");
    } catch (e) {
      this.closeMic();
      this.handlers.onError(`Microfono non disponibile: ${(e as Error).message}`);
    }
  }

  async setHandsFree(on: boolean) {
    this.handsFree = on;
    this.emit();
    if (on) {
      try {
        await this.openMic();
      } catch (e) {
        this.handsFree = false;
        this.handlers.onError(`Microfono non disponibile: ${(e as Error).message}`);
      }
    } else if (!this.pushChunks) {
      this.vad = { speaking: false, chunks: [], loud: 0, quiet: 0, awaitingCommand: false };
      this.closeMic();
      this.setState("idle");
    }
    this.emit();
  }

  setReplies(on: boolean) {
    this.replies = on;
    if (!on) this.stopSpeaking();
    this.emit();
  }

  async speak(text: string) {
    if (!this.replies || !this.status?.tts) return;
    this.stopSpeaking();
    const clean = text.replace(/https?:\/\/\S+/g, "").slice(0, 1500).trim();
    if (!clean) return;
    const request = new AbortController();
    this.speechRequest = request;
    try {
      const res = await fetch("/api/voice/speak", {
        method: "POST",
        headers: { "Content-Type": "application/json", ...AUTH },
        body: JSON.stringify({ text: clean }),
        signal: request.signal,
      });
      if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error?.message || `HTTP ${res.status}`);
      const blob = await res.blob();
      if (request.signal.aborted) return;
      const audio = new Audio(URL.createObjectURL(blob));
      this.player = audio;
      audio.onplaying = () => {
        if (this.player === audio) this.setState("speaking");
      };
      audio.onended = () => {
        if (this.player === audio) this.stopSpeaking();
      };
      audio.onerror = () => {
        if (this.player !== audio) return;
        this.stopSpeaking();
        this.handlers.onError("Riproduzione della risposta vocale interrotta da un errore.");
      };
      void this.animateSpeech(audio, blob);
      await audio.play();
    } catch (e) {
      if (request.signal.aborted) return;
      this.stopSpeaking();
      this.handlers.onError(`Risposta vocale non disponibile: ${(e as Error).message}`);
    }
  }

  /** L'apertura della bocca dell'avatar segue l'intensità dell'audio locale. */
  private async animateSpeech(audio: HTMLAudioElement, blob: Blob) {
    if (typeof OfflineAudioContext !== "function" || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    let decoded: AudioBuffer;
    try {
      const context = new OfflineAudioContext(1, 1, 16000);
      decoded = await context.decodeAudioData(await blob.arrayBuffer());
    } catch {
      return;
    }
    if (this.player !== audio) return;
    const samples = decoded.getChannelData(0);
    const update = () => {
      const start = Math.floor(audio.currentTime * decoded.sampleRate);
      const end = Math.min(samples.length, start + 480);
      let peak = 0;
      if (!audio.paused) for (let i = start; i < end; i += 4) peak = Math.max(peak, Math.abs(samples[i]));
      setMouth(Math.min(1, peak * 4).toFixed(2));
      this.speechFrame = requestAnimationFrame(update);
    };
    this.speechFrame = requestAnimationFrame(update);
  }

  stopSpeaking() {
    this.speechRequest?.abort();
    this.speechRequest = null;
    cancelAnimationFrame(this.speechFrame);
    setMouth(null);
    if (this.player) {
      const audio = this.player;
      this.player = null;
      audio.pause();
      URL.revokeObjectURL(audio.src);
    }
    if (this.state === "speaking") this.setState("idle");
  }
}

function setMouth(value: string | null) {
  document.querySelectorAll<HTMLElement>("[data-avatar]").forEach((a) => {
    if (value == null) a.style.removeProperty("--mouth-opening");
    else a.style.setProperty("--mouth-opening", value);
  });
}

export const voice = new VoiceEngine();
