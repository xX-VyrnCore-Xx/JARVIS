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
  choices: { stt_models: string[]; tts_voices: string[] };
  saved?: { plan?: { stt_model?: string; tts_voice?: string } };
  sizes_mb: { voice_packages: number; tts: number; stt: Record<string, number> };
}

export interface SetupHandle {
  open: () => void;
}

export const SetupDialog = forwardRef<SetupHandle>(function SetupDialog(_, ref) {
  const { refreshStatus, show } = useApp();
  const dialog = useRef<HTMLDialogElement>(null);
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const [stt, setStt] = useState("small");
  const [tts, setTts] = useState("it_IT-paola-medium");
  const [steps, setSteps] = useState<SetupStatus["steps"] | null>(null);
  const [running, setRunning] = useState(false);
  const [finished, setFinished] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const watchGeneration = useRef(0);

  const watch = () => {
    clearTimeout(timer.current);
    const generation = ++watchGeneration.current;
    const poll = async () => {
      try {
        const s = await api<SetupStatus>("/api/setup");
        if (generation !== watchGeneration.current) return;
        setStatus(s);
        setSteps(s.steps);
        if (!s.running) {
          timer.current = undefined;
          setRunning(false);
          const failed = s.steps.filter((x) => x.status === "failed").length;
          setFinished(failed ? `Configurazione terminata con ${failed} passi non riusciti: leggi i dettagli e riprova.` : "Configurazione completata. La voce è pronta.");
          refreshStatus();
          return;
        }
        setFinished(null);
        timer.current = window.setTimeout(() => void poll(), 700);
      } catch (e) {
        if (generation !== watchGeneration.current) return;
        const reason = e instanceof Error ? e.message : "errore di rete";
        setFinished(`Connessione al Core interrotta (${reason}). Riprovo automaticamente…`);
        timer.current = window.setTimeout(() => void poll(), 1500);
      }
    };
    void poll();
  };

  const open = async () => {
    const s = await api<SetupStatus>("/api/setup");
    const saved = s.saved?.plan || {};
    setStatus(s);
    setStt(saved.stt_model || "small");
    setTts(saved.tts_voice || "it_IT-paola-medium");
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
    return () => {
      watchGeneration.current += 1;
      clearTimeout(timer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const size = () => {
    if (!status) return "";
    const mb = status.sizes_mb;
    const total = mb.voice_packages + (mb.stt[stt] || 0) + mb.tts;
    return total ? `Download stimato: circa ${(total / 1024).toFixed(1)} GB.` : "Niente da scaricare.";
  };

  const start = async () => {
    try {
      await post("/api/setup/start", { stt_model: stt, tts_voice: tts });
      setRunning(true);
      setSteps([]);
      watch();
    } catch (e) {
      setFinished(e instanceof Error ? e.message : "Impossibile avviare la configurazione.");
    }
  };

  function close() {
    try {
      sessionStorage.setItem("jarvis-setup-later", "1");
    } catch {
      /* niente */
    }
    dialog.current?.close();
  }

  return (
    <dialog id="setup-dialog" className="setup" aria-labelledby="setup-title" ref={dialog}>
      <form method="dialog" id="setup-form" className="form setup-form" aria-busy={running} onSubmit={(e) => e.preventDefault()}>
        <h2 id="setup-title">Configura la voce</h2>
        <div className="setup-body">
          <p className="hint">Scarica i componenti per usare la voce anche offline.</p>
          <fieldset>
            <legend>Voce locale</legend>
            <p className="hint">Riconoscimento vocale (faster-whisper), voce di sintesi (Piper) e attivazione con «Ehi Jarvis» (openWakeWord).</p>
            <div className="row">
              <label>Riconoscimento
                <select name="stt_model" value={stt} disabled={running} onChange={(e) => setStt(e.target.value)}>
                  {(status?.choices.stt_models || []).map((v) => <option key={v} value={v}>{STT_LABEL[v] || v}</option>)}
                </select>
              </label>
              <label>Voce
                <select name="tts_voice" value={tts} disabled={running} onChange={(e) => setTts(e.target.value)}>
                  {(status?.choices.tts_voices || []).map((v) => <option key={v} value={v}>{v}</option>)}
                </select>
              </label>
            </div>
          </fieldset>
          <p className="hint" id="setup-size" role="status" aria-live="polite" aria-atomic="true">{finished || size()}</p>
          <ol id="setup-steps" className="setup-steps" hidden={!steps}>
            {(steps || []).map((s, i) => (
              <li key={i} className={`step ${s.status}`}>
                <span className="icon" aria-hidden="true">{ICON[s.status] || "○"}</span> {s.label}
                {s.progress != null && s.status === "running" ? <progress max={100} value={s.progress} /> : null}
                {s.detail ? <div className="sub">{s.detail}</div> : null}
              </li>
            ))}
          </ol>
        </div>
        <div className="row setup-actions">
          <button type="button" className="primary" id="setup-start" disabled={running} onClick={start}>Installa</button>
          <button type="button" className="ghost" id="setup-later" onClick={close}>
            {finished ? "Chiudi" : "Più tardi"}
          </button>
          <button type="button" className="ghost" onClick={() => {
            close();
            show("system");
          }}>Vai ai modelli cloud</button>
        </div>
      </form>
    </dialog>
  );
});
