import { useCallback, useEffect, useRef, useState } from "react";
import { api, chatStream, fmtTime, pref } from "../lib/api";
import { voice } from "../lib/voice";
import { Avatar, AVATAR_LABELS, useAvatarState } from "../components/Avatar";
import { Icon } from "../components/Icon";
import { useApp, useVoice } from "../state";
import { PASSKEY_SUPPORTED, decideConfirmation, registerPasskey } from "../lib/passkey";

interface Step {
  kind: string;
  agent?: string;
  role?: string;
  name?: string;
  backend?: { name: string };
  tool?: string;
  decision?: string;
  risk?: string;
  approved?: boolean;
  status?: string;
  error?: string;
}
interface Source {
  url: string;
  title?: string;
  retrieved_at?: string;
  content_sha256?: string;
  provider?: string;
}
export interface Confirmation {
  confirmation_id: string;
  tool: string;
  risk: string;
  expected_effect: string;
  target: string;
  expires_at: string;
}
type MsgItem = { id: number; kind: "msg"; role: "user" | "assistant"; text: string; steps?: Step[]; sources?: Source[]; error?: boolean; streaming?: boolean };
type ConfirmItem = { id: number; kind: "confirm"; c: Confirmation; decided?: boolean; approved?: boolean };
type Item = MsgItem | ConfirmItem;
type NewItem = Omit<MsgItem, "id"> | Omit<ConfirmItem, "id">;

const SUGGESTIONS = [
  "ciao", "che ore sono", "info sistema", "scrivi note.txt: la mia prima nota", "leggi note.txt",
  "ricorda che preferisco risposte brevi", "cosa ricordi", "ogni ora info sistema", "elenca automazioni",
  "cerca intelligenza artificiale",
];

/** Passkey del dispositivo associato (ADR-0042): con una passkey le approvazioni richiedono impronta, volto o PIN. */
function PasskeySetup() {
  const [count, setCount] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");
  useEffect(() => {
    if (!PASSKEY_SUPPORTED) return;
    api<{ items: unknown[] }>("/api/passkeys").then((d) => setCount(d.items.length), () => setCount(null));
  }, []);
  if (!PASSKEY_SUPPORTED || count === null) return null;
  if (count > 0) return <span className="pill ok" id="passkey-status" title="Le approvazioni da questo dispositivo richiedono la passkey">Passkey attiva ✓</span>;
  return (
    <span className="passkey-setup">
      <button type="button" className="ghost small" id="passkey-setup" disabled={busy}
        onClick={async () => {
          setBusy(true);
          setMsg("");
          try {
            await registerPasskey();
            setCount(1);
          } catch (e) {
            setMsg((e as Error).message);
          } finally {
            setBusy(false);
          }
        }}>
        <Icon name="shield" size={14} /> Proteggi le approvazioni con una passkey
      </button>
      {msg ? <span className="sub" role="alert"> {msg}</span> : null}
    </span>
  );
}

/** Specializzazioni degli agenti (ADR-0038). */
const AGENTS: Record<string, string> = {
  research: "ricerca", system: "sistema", automation: "automazioni", memory: "memoria", coding: "codice",
};

function StepChips({ steps }: { steps?: Step[] }) {
  if (!steps || !steps.length) return null;
  const chips: React.ReactElement[] = [];
  steps.forEach((s, i) => {
    if (s.kind === "delegation" && s.backend) chips.push(<span key={i} className="step">agente · {s.backend.name}</span>);
    if (s.kind === "delegation" && s.agent && AGENTS[s.agent])
      chips.push(<span key={`${i}-a`} className="step specialist">specialista · {AGENTS[s.agent]}</span>);
    if (s.kind === "subagent")
      chips.push(<span key={i} className={`step specialist ${s.status}`}>agente temporaneo · {s.role}</span>);
    if (s.kind === "skill") chips.push(<span key={i} className="step specialist">competenza · {s.name}</span>);
    if (s.kind === "loop_guard") chips.push(<span key={i} className="step deny">ripetizione bloccata · {s.tool}</span>);
    if (s.kind === "scope") chips.push(<span key={i} className="step deny">fuori ambito · {s.tool}</span>);
    if (s.kind === "tool_call") chips.push(<span key={i} className="step">strumento · {s.tool}</span>);
    if (s.kind === "policy") chips.push(<span key={i} className={`step ${s.decision}`}>policy · {s.decision} · rischio {s.risk}</span>);
    if (s.kind === "confirmation")
      chips.push(<span key={i} className={`step ${s.approved ? "allow" : "deny"}`}>{s.approved ? "confermato" : "rifiutato"}</span>);
    if (s.kind === "tool_result")
      chips.push(<span key={i} className={`step ${s.status}`}>esito · {s.status}{s.error ? " · " + s.error : ""}</span>);
  });
  return chips.length ? <div className="meta">{chips}</div> : null;
}

function Sources({ sources }: { sources?: Source[] }) {
  const safe = (sources || []).filter((s) => /^https?:\/\//i.test(s.url || ""));
  if (!safe.length) return null;
  return (
    <div className="sources">
      <div className="sources-title">Fonti</div>
      <ol>
        {safe.map((s, i) => (
          <li key={i}>
            <a href={s.url} target="_blank" rel="noopener noreferrer nofollow">{s.title || s.url}</a>
            <span className="src-meta">
              {` · ${new URL(s.url).hostname} · ${fmtTime(s.retrieved_at)}${s.content_sha256 ? " · sha256 " + s.content_sha256.slice(0, 12) + "…" : ""}${s.provider ? " · " + s.provider : ""}`}
            </span>
          </li>
        ))}
      </ol>
    </div>
  );
}

export function Chat({ active }: { active: boolean }) {
  const app = useApp();
  const { refreshStatus, pollNotifications, dismissToast, thinking, setThinking, sendRef, bumpActivity } = app;
  const [items, setItems] = useState<Item[]>([]);
  const [pending, setPending] = useState(false);
  const sending = useRef(false);
  const [text, setText] = useState("");
  const nextId = useRef(1);
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLTextAreaElement>(null);
  useVoice();
  const avatarState = useAvatarState();

  const add = useCallback((item: NewItem) => {
    setItems((list) => [...list, { ...item, id: nextId.current++ } as Item]);
  }, []);
  const say = useCallback(
    (textOut: string, extra: { steps?: Step[]; sources?: Source[]; error?: boolean } = {}) =>
      add({ kind: "msg", role: "assistant", text: textOut, ...extra }),
    [add],
  );

  useEffect(() => {
    box.current?.scrollTo({ top: box.current.scrollHeight });
  }, [items, pending]);

  const send = useCallback(
    async (raw: string) => {
      const message = raw.trim();
      if (!message || sending.current) return;
      sending.current = true;
      add({ kind: "msg", role: "user", text: message });
      setPending(true);
      setThinking(true);
      // Il messaggio in arrivo compare al primo frammento e cresce finché l'esito non lo sostituisce.
      let streamId: number | null = null;
      let streamed = "";
      const onDelta = (delta: string) => {
        streamed += delta;
        if (streamId == null) {
          streamId = nextId.current++;
          const id = streamId;
          setPending(false);
          setItems((list) => [...list, { id, kind: "msg", role: "assistant", text: streamed, streaming: true }]);
        } else {
          const id = streamId, text = streamed;
          setItems((list) => list.map((x) => (x.id === id ? { ...x, text } : x)));
        }
      };
      try {
        const out = await chatStream(message, onDelta);
        const final = { kind: "msg" as const, role: "assistant" as const, text: out.reply || "(nessuna risposta)", steps: out.steps, sources: out.sources,
          error: out.task && out.task.state === "failed" };
        if (streamId != null) {
          const id = streamId;
          setItems((list) => list.map((x) => (x.id === id ? { id, ...final } : x)));
        } else add(final);
        if (out.confirmation) add({ kind: "confirm", c: out.confirmation });
        if (out.reply) void voice.speak(out.reply);
      } catch (e) {
        if (streamId != null) {
          const id = streamId;
          setItems((list) => list.map((x) => (x.id === id && x.kind === "msg" ? { ...x, streaming: false } : x)));
        }
        say(`Errore: ${(e as Error).message}`, { error: true });
      } finally {
        sending.current = false;
        setPending(false);
        setThinking(false);
        refreshStatus();
        pollNotifications();
        bumpActivity();
      }
    },
    [add, say, setThinking, refreshStatus, pollNotifications, bumpActivity],
  );

  sendRef.current = send;

  useEffect(() => {
    voice.handlers = {
      onCommand: (t) => void send(t),
      onError: (m) => say(m, { error: true }),
      onNothingHeard: () => say("Non ho sentito nulla: riprova parlando più vicino al microfono."),
    };
    voice.setReplies(pref("jarvis.voice.replies"));
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") voice.stopSpeaking();
    };
    document.addEventListener("keydown", esc);
    return () => document.removeEventListener("keydown", esc);
  }, [send, say]);

  useEffect(() => {
    (async () => {
      try {
        const { items: history } = await api("/api/conversation?limit=50");
        const loaded: Item[] = [];
        if (!history.length)
          loaded.push({ id: nextId.current++, kind: "msg", role: "assistant", text: "Ciao, sono JARVIS. Scrivimi qualcosa o prova uno dei suggerimenti qui sotto." });
        for (const m of history)
          loaded.push({ id: nextId.current++, kind: "msg", role: m.role === "user" ? "user" : "assistant", text: m.content, sources: m.sources });
        const conf = await api("/api/confirmations");
        for (const c of conf.items) loaded.push({ id: nextId.current++, kind: "confirm", c });
        setItems((list) => [...loaded, ...list]);
      } catch (e) {
        say(`Errore nel caricamento: ${(e as Error).message}`, { error: true });
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (active) input.current?.focus();
  }, [active]);

  const decide = async (item: ConfirmItem, approve: boolean) => {
    setItems((list) => list.map((x) => (x.id === item.id ? { ...item, decided: true } : x)));
    dismissToast(`confirmation:${item.c.confirmation_id}`);
    try {
      const out = await decideConfirmation<any>(item.c.confirmation_id, approve);
      setItems((list) => list.map((x) => (x.id === item.id ? { ...item, decided: true, approved: approve } : x)));
      say(out.reply, { steps: out.steps, sources: out.sources });
    } catch (e) {
      setItems((list) => list.filter((x) => x.id !== item.id));
      say(`Errore: ${(e as Error).message}`, { error: true });
    }
    refreshStatus();
    pollNotifications();
    bumpActivity();
  };

  const submit = (e?: React.FormEvent) => {
    e?.preventDefault();
    if (sending.current) return;
    const t = text;
    setText("");
    if (input.current) input.current.style.height = "";
    void send(t);
  };

  const available = voice.available;
  const st = voice.status;
  return (
    <section className={`view${active ? " active" : ""}`} id="view-chat" aria-label="Chat">
      <div className="avatar-panel">
        <Avatar id="avatar" />
        <span id="avatar-status" role="status">{AVATAR_LABELS[avatarState]}</span>
      </div>
      <div className="messages" id="messages" aria-live="polite" ref={box}>
        {items.map((it) =>
          it.kind === "msg" ? (
            <div key={it.id} className={`msg ${it.role}${it.error ? " error" : ""}${it.streaming ? " streaming" : ""}`} aria-busy={it.streaming || undefined}
              data-who={it.role === "user" ? "Tu" : "JARVIS"}>
              {it.text}
              <Sources sources={it.sources} />
              <StepChips steps={it.steps} />
            </div>
          ) : it.decided && it.approved !== undefined ? (
            <div key={it.id} className="confirm">
              <p className="small">{it.approved ? "✓ Confermato" : "✗ Rifiutato"}</p>
            </div>
          ) : (
            <div key={it.id} className="confirm" role="group" aria-label="Conferma richiesta">
              <h3>Conferma richiesta · {it.c.tool} · rischio {it.c.risk}</h3>
              <p>{it.c.expected_effect}</p>
              <p className="small">
                Destinazione: {it.c.target} · scade alle {new Date(it.c.expires_at).toLocaleTimeString("it-IT")} · valida una sola volta
              </p>
              <div className="actions">
                <button className="primary" disabled={it.decided} onClick={() => decide(it, true)}>Approva</button>
                <button className="danger" disabled={it.decided} onClick={() => decide(it, false)}>Rifiuta</button>
              </div>
            </div>
          ),
        )}
        {pending ? (
          <div className="msg typing" aria-label="JARVIS sta elaborando">
            <span className="dots" aria-hidden="true"><i></i><i></i><i></i></span> JARVIS sta elaborando…
          </div>
        ) : null}
      </div>
      <div className="suggestions" id="suggestions">
        {SUGGESTIONS.map((s) => (
          <button key={s} className="chip" onClick={() => void send(s)}>{s}</button>
        ))}
      </div>
      <div className="voice-bar" id="voice-bar">
        <label className="check">
          <input
            type="checkbox" id="voice-replies" checked={voice.replies} disabled={!(available && st?.tts)}
            onChange={(e) => {
              pref("jarvis.voice.replies", e.target.checked);
              voice.setReplies(e.target.checked);
            }}
          />{" "}
          Risposte vocali
        </label>
        <label className="check">
          <input type="checkbox" id="voice-handsfree" checked={voice.handsFree} disabled={!available}
            onChange={(e) => void voice.setHandsFree(e.target.checked)} />{" "}
          Mani libere («Ehi Jarvis»)
        </label>
        <button type="button" className="ghost small" id="voice-stop" hidden={voice.state !== "speaking"} onClick={() => voice.stopSpeaking()}>
          <Icon name="stop" size={14} /> Interrompi
        </button>
        <span className="mic-live" id="mic-live" hidden={!voice.micOpen} aria-live="polite">● Microfono attivo</span>
        <PasskeySetup />
      </div>
      <form className="composer" id="composer" onSubmit={submit}>
        <button
          type="button" className="mic" id="mic" aria-pressed={voice.state === "listening" && voice.pushing}
          disabled={!available} aria-label="Parla a JARVIS"
          title={available ? "Parla (clic per iniziare e per terminare)" : `Voce non disponibile: ${(st?.reasons || []).join("; ")}`}
          onClick={() => void voice.togglePushToTalk()}
        >
          <Icon name="mic" size={20} />
        </button>
        <textarea
          id="input" ref={input} rows={1} placeholder="Scrivi a JARVIS…" maxLength={8000} value={text}
          title="Invio per inviare, Maiusc+Invio per andare a capo"
          aria-label="Messaggio (Invio per inviare, Maiusc+Invio per andare a capo)"
          onChange={(e) => {
            setText(e.target.value);
            e.target.style.height = "";
            e.target.style.height = Math.min(e.target.scrollHeight, 200) + "px";
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
          }}
        />
        <button type="submit" id="send" className="primary send" disabled={thinking}><Icon name="send" size={16} /> Invia</button>
      </form>
    </section>
  );
}
