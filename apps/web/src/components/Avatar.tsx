import { useId, useState } from "react";
import { Icon } from "./Icon";
import { useApp, useVoice } from "../state";
import { voice } from "../lib/voice";
import { pref } from "../lib/api";
import { ACTIVITY_LABELS } from "../lib/avatarActivity";
import type { AvatarActivity } from "../lib/avatarActivity";

const AVATAR_LABELS = {
  idle: "Pronto",
  thinking: "Elaborazione",
  listening: "In ascolto",
  transcribing: "Trascrizione",
  speaking: "Risposta vocale",
};

function useAvatarState(backgroundActivity = false) {
  useVoice();
  const { thinking } = useApp();
  return voice.state === "idle" && (thinking || backgroundActivity) ? "thinking" : voice.state;
}

export function AvatarStatus({ id, activity }: { id?: string; activity?: AvatarActivity }) {
  const state = useAvatarState(!!activity);
  const { avatarActivity } = useApp();
  const { category, outcome } = activity || avatarActivity;
  const detail = outcome === "attention" ? "Attende conferma" : outcome === "error" ? "Errore" : "";
  return (
    <div className="avatar-status" data-tone={outcome || category} role="status">
      <span className="avatar-status-dot" aria-hidden="true" />
      <span id={id}>{AVATAR_LABELS[state]}</span>
      <span className="avatar-status-category">{ACTIVITY_LABELS[category]}{detail && ` · ${detail}`}</span>
    </div>
  );
}

function VoiceAction({ primary = false }: { primary?: boolean }) {
  const state = useAvatarState();
  const { reachable } = useApp();
  const stopping = state === "speaking";
  return (
    <button type="button" className={primary ? "primary avatar-talk" : "ghost small"}
      disabled={!stopping && (!reachable || !voice.available || state === "transcribing" || state === "thinking")}
      aria-pressed={voice.pushing}
      onClick={() => {
        if (stopping) voice.stopSpeaking();
        else void voice.togglePushToTalk();
      }}>
      <Icon name={stopping ? "stop" : "mic"} size={17} />
      {stopping ? "Interrompi risposta" : voice.pushing ? "Invia registrazione" : primary ? "Parla con JARVIS" : "Parla"}
    </button>
  );
}

export function Avatar({ id, size = "small", activity }: { id?: string; size?: "small" | "large"; activity?: AvatarActivity }) {
  const state = useAvatarState(!!activity);
  const { show, avatarActivity } = useApp();
  const activeActivity = activity || avatarActivity;
  const [open, setOpen] = useState(false);
  const more = useId();
  return (
    <div className={`avatar-control avatar-control-${size}`} onKeyDown={(event) => {
      if (event.key === "Escape" && open) {
        setOpen(false);
        event.currentTarget.querySelector("button")?.focus();
      }
    }}>
      <button className="avatar-trigger" type="button" aria-expanded={open} aria-controls={more} aria-label="Controlli avatar di JARVIS"
        onClick={() => setOpen(!open)}>
        <span className={`avatar avatar-${size} jarvis-avatar`} id={id} data-avatar="" data-state={state}
          data-tone={activeActivity.outcome || activeActivity.category} aria-hidden="true">
          <i className="jarvis-ring jarvis-ring-outer" />
          <i className="jarvis-ring jarvis-ring-inner" />
          <i className="jarvis-face" />
          <i className="jarvis-speech" />
        </span>
      </button>
      <div className="avatar-tools" id={more} hidden={!open}>
        <VoiceAction />
        <button type="button" className="ghost small" onClick={() => { setOpen(false); show("chat"); }}>
          <Icon name="chat" size={14} /> Apri chat
        </button>
      </div>
    </div>
  );
}

export function VoiceReplySwitch({ id }: { id?: string }) {
  useVoice();
  return (
    <label className="check avatar-replies">
      <input type="checkbox" id={id} checked={voice.replies} disabled={!(voice.available && voice.status?.tts)}
        onChange={(event) => {
          pref("jarvis.voice.replies", event.target.checked);
          voice.setReplies(event.target.checked);
        }} />
      Risposte vocali
    </label>
  );
}

export function AvatarConversation() {
  const { show } = useApp();
  return (
    <div className="avatar-conversation">
      <VoiceAction primary />
      <button type="button" className="ghost avatar-chat-link" onClick={() => show("chat")}>
        <Icon name="chat" size={17} /> Vai alla chat
      </button>
      <VoiceReplySwitch />
    </div>
  );
}
