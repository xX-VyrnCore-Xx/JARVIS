import { useId, useState } from "react";
import { Icon } from "./Icon";
import { useApp, useVoice } from "../state";
import { voice } from "../lib/voice";

export const AVATAR_LABELS: Record<string, string> = {
  idle: "Pronto",
  thinking: "Elaborazione",
  listening: "In ascolto",
  transcribing: "Trascrizione",
  speaking: "Risposta vocale",
};

/** Stato dell'avatar: la voce ha la precedenza; senza voce attiva, una richiesta in corso è «thinking». */
export function useAvatarState() {
  useVoice();
  const { thinking } = useApp();
  return voice.state === "idle" && thinking ? "thinking" : voice.state;
}

/**
 * Nucleo olografico: anelli concentrici in prospettiva 3D, un'iride centrale e la «bocca» che segue
 * l'intensità dell'audio locale (variabile CSS --mouth-opening impostata dal motore vocale).
 */
export function Avatar({ id, size = "small" }: { id?: string; size?: "small" | "large" }) {
  const state = useAvatarState();
  const { show } = useApp();
  const [open, setOpen] = useState(false);
  const more = useId();
  return (
    <div className={`avatar-control avatar-control-${size}`} onKeyDown={(event) => {
      if (event.key === "Escape") {
        setOpen(false);
        event.currentTarget.querySelector("button")?.focus();
      }
    }}>
      <button className="avatar-trigger" type="button" aria-expanded={open} aria-controls={more} aria-label="Controlli avatar di JARVIS"
        onClick={() => setOpen(!open)}
        onPointerMove={(event) => {
          if (event.pointerType === "touch" || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
          const area = event.currentTarget.getBoundingClientRect();
          event.currentTarget.style.setProperty("--avatar-x", `${(event.clientX - area.left - area.width / 2) * .10}px`);
          event.currentTarget.style.setProperty("--avatar-y", `${(event.clientY - area.top - area.height / 2) * .10}px`);
        }}
        onPointerLeave={(event) => {
          event.currentTarget.style.removeProperty("--avatar-x");
          event.currentTarget.style.removeProperty("--avatar-y");
        }}>
    <span className={`avatar avatar-${size}`} id={id} data-avatar="" data-state={state} aria-hidden="true">
      <span></span>
      <span></span>
      <span></span>
      {size === "large" ? (
        <>
          <i className="halo"></i>
          <i className="ticks"></i>
          <b className="iris"></b>
          <b className="mouth"></b>
        </>
      ) : null}
    </span>
      </button>
      <div className="avatar-tools" id={more} hidden={!open}>
        <button type="button" className="ghost small" disabled={!voice.available && state !== "speaking"}
          onClick={() => {
            if (state === "speaking") voice.stopSpeaking();
            else void voice.togglePushToTalk();
          }}>
          <Icon name={state === "speaking" ? "stop" : "mic"} size={14} />
          {state === "speaking" ? "Interrompi risposta" : voice.pushing ? "Completa registrazione" : "Parla"}
        </button>
        <button type="button" className="ghost small" onClick={() => { setOpen(false); show("chat"); }}>
          <Icon name="chat" size={14} /> Apri chat
        </button>
      </div>
    </div>
  );
}
