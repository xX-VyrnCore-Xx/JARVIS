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
  return (
    <div className={`avatar avatar-${size}`} id={id} data-avatar="" data-state={state} aria-hidden="true">
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
    </div>
  );
}
