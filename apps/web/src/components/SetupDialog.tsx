// Configurazione guidata (ADR-0019): al primo avvio si apre da sola con le scelte consigliate.

import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import { api, post } from "../lib/api";
import { useApp } from "../state";

const STT_LABEL: Record<string, string> = {
  tiny: "tiny (veloce, meno preciso)", base: "base", small: "small (consigliato)", medium: "medium (più preciso, più lento)",
};
const ICON: Record<string, string> = { pending: "○", running: "◐", done: "✓", failed: "✗", skipped: "–" };

interface SetupStatus {
  first_run: boolean;
  running: boolean;
  steps: { label: string; status: string; progress?: number | null; detail?: string }[];
  choices: { stt_models: string[]; tts_voices: string[]; ollama_models: string[] };
  detected: { ollama: boolean; ollama_models: string[] };
  saved?: { plan?: { stt_model?: string; tts_voice?: string; ollama_model?: string } };
  sizes_mb: { voice_packages: number; tts: number; stt: Record<string, number>; ollama: Record<string, number> };
}

export interface SetupHandle {
  open: () => void;
}

export const SetupDialog = forwardRef<SetupHandle>(function SetupDialog(_, ref) {
  const { refreshStatus } = useApp();
  const dialog = useRef<HTMLDialogElement>(null);
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const [voiceOn, setVoiceOn] = useState(true);
  const [stt, setStt] = useState("small");
  const [tts, setTts] = useState("it_IT-paola-medium");
  const [ollama, setOllama] = useState(false);
  const [model, setModel] = useState("qwen2.5:3b");
  const [steps, setSteps] = useState<SetupStatus["steps"] | null>(null);
  const [running, setRunning] = useState(false);
  const [finished, setFinished] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);

  const watch = () => {
    clearInterval(timer.current);
    timer.current = window.setInterval(async () => {
      const s = await api<SetupStatus>("/api/setup").catch(() => null);
      if (!s) return;
      setSteps(s.steps);
      if (!s.running) {
        clearInterval(timer.current);
        setRunning(false);
        const failed = s.steps.filter((x) => x.status === "failed").length;
        setFinished(failed ? `Configurazione terminata con ${failed} passi non riusciti: leggi i dettagli e riprova.` : "Configurazione completata: voce e modello sono pronti.");
        refreshStatus();
      }
    }, 700);
  };

  const open = async () => {
    const s = await api<SetupStatus>("/api/setup");
    const saved = s.saved?.plan || {};
    setStatus(s);
    setStt(saved.stt_model || "small");
    setTts(saved.tts_voice || "it_IT-paola-medium");
    setModel(saved.ollama_model || "qwen2.5:3b");
    setOllama(s.detected.ollama);
    setFinished(null);
    setRunning(s.running);
    if (s.running) {
      setSteps(s.steps);
      watch();
    } else setSteps(null);
    if (dialog.current && !dialog.current.open) dialog.current.showModal();
  };

  useImperativeHandle(ref, () => ({ open: () => void open().catch((e) => alert(e.message)) }));

  useEffect(() => {
    api<SetupStatus>("/api/setup")
      .then((s) => {
        let later = false;
        try {
          later = sessionStorage.getItem("jarvis-setup-later") === "1";
        } catch {
          /* niente */
        }
        if (s.first_run && !later) void open();
      })
      .catch(() => {});
    return () => clearInterval(timer.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const size = () => {
    if (!status) return "";
    const mb = status.sizes_mb;
    let total = 0;
    if (voiceOn) total += mb.voice_packages + (mb.stt[stt] || 0) + mb.tts;
    if (ollama && status.detected.ollama) total += mb.ollama[model] || 0;
    return total ? `Download stimato: circa ${(total / 1024).toFixed(1)} GB.` : "Niente da scaricare.";
  };

  const start = async () => {
    try {
      await post("/api/setup/start", { voice: voiceOn, stt_model: stt, tts_voice: tts, ollama_model: ollama && status?.detected.ollama ? model : null });
      setRunning(true);
      setSteps([]);
      watch();
    } catch (e) {
      alert((e as Error).message);
    }
  };

  return (
    <dialog id="setup-dialog" className="setup" aria-labelledby="setup-title" ref={dialog}>
      <form method="dialog" id="setup-form" className="form" onSubmit={(e) => e.preventDefault()}>
        <h2 id="setup-title">Configura JARVIS</h2>
        <p className="hint">Scegli cosa scaricare ora. Tutto resta su questo computer; puoi ripetere la configurazione dalla sezione Sistema.</p>
        <fieldset>
          <legend><label className="check"><input type="checkbox" name="voice" checked={voiceOn} onChange={(e) => setVoiceOn(e.target.checked)} /> Voce locale</label></legend>
          <p className="hint">Riconoscimento vocale (faster-whisper), voce di sintesi (Piper) e attivazione con «Ehi Jarvis» (openWakeWord).</p>
          <div className="row">
            <label>Riconoscimento
              <select name="stt_model" value={stt} onChange={(e) => setStt(e.target.value)}>
                {(status?.choices.stt_models || []).map((v) => <option key={v} value={v}>{STT_LABEL[v] || v}</option>)}
              </select>
            </label>
            <label>Voce
              <select name="tts_voice" value={tts} onChange={(e) => setTts(e.target.value)}>
                {(status?.choices.tts_voices || []).map((v) => <option key={v} value={v}>{v}</option>)}
              </select>
            </label>
          </div>
        </fieldset>
        <fieldset>
          <legend><label className="check"><input type="checkbox" name="ollama" checked={ollama} disabled={!status?.detected.ollama} onChange={(e) => setOllama(e.target.checked)} /> Modello AI locale (Ollama)</label></legend>
          <p className="hint" id="setup-ollama-hint">
            {status?.detected.ollama ? (
              status.detected.ollama_models.length ? `Ollama è attivo. Modelli già presenti: ${status.detected.ollama_models.join(", ")}.` : "Ollama è attivo: il modello scelto verrà scaricato."
            ) : (
              <>
                Ollama non è attivo. Installalo da <a href="https://ollama.com/download" target="_blank" rel="noopener">ollama.com/download</a>, aprilo e
                riapri questa finestra. Senza Ollama JARVIS funziona in modalità offline con comandi semplici.
              </>
            )}
          </p>
          <label>Modello
            <select name="ollama_model" value={model} onChange={(e) => setModel(e.target.value)}>
              {(status?.choices.ollama_models || []).map((v) => <option key={v} value={v}>{v}</option>)}
            </select>
          </label>
        </fieldset>
        <p className="hint" id="setup-size">{finished || size()}</p>
        <ol id="setup-steps" className="setup-steps" hidden={!steps}>
          {(steps || []).map((s, i) => (
            <li key={i} className={`step ${s.status}`}>
              <span className="icon" aria-hidden="true">{ICON[s.status] || "○"}</span> {s.label}
              {s.progress != null && s.status === "running" ? <progress max={100} value={s.progress} /> : null}
              {s.detail ? <div className="sub">{s.detail}</div> : null}
            </li>
          ))}
        </ol>
        <div className="row">
          <button type="button" className="primary" id="setup-start" disabled={running} onClick={start}>Installa</button>
          <button type="button" className="ghost" id="setup-later" onClick={() => {
            try {
              sessionStorage.setItem("jarvis-setup-later", "1");
            } catch {
              /* niente */
            }
            dialog.current?.close();
          }}>
            {finished ? "Chiudi" : "Più tardi"}
          </button>
        </div>
      </form>
    </dialog>
  );
});
