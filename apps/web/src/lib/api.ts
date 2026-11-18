// Accesso all'API del Core. Il token di sessione arriva nella pagina solo sul computer del Core; un
// dispositivo associato usa il proprio token (RF-11, ADR-0014).

const meta = document.querySelector<HTMLMetaElement>('meta[name="jarvis-token"]');
export const TOKEN = meta ? meta.content : "";
export const DEVICE_MODE = !TOKEN;
const DEVICE_TOKEN = (() => {
  try {
    return localStorage.getItem("jarvis-device") || "";
  } catch {
    return "";
  }
})();
export const AUTH: Record<string, string> = DEVICE_MODE ? { "X-Jarvis-Device": DEVICE_TOKEN } : { "X-Jarvis-Token": TOKEN };

declare global {
  interface Window {
    jarvisLeaving?: boolean;
  }
}

export class ApiError extends Error {}

/** Dispositivo revocato: una sola navigazione verso l'associazione, anche se più richieste falliscono insieme. */
function leaveIfRevoked() {
  if (!DEVICE_MODE || window.jarvisLeaving) return;
  window.jarvisLeaving = true;
  try {
    localStorage.removeItem("jarvis-device");
  } catch {
    /* niente */
  }
  location.replace("/pair");
}

export async function api<T = any>(path: string, options: RequestInit = {}): Promise<T> {
  const res = await fetch(path, {
    ...options,
    headers: { "Content-Type": "application/json", ...AUTH, ...((options.headers as Record<string, string>) || {}) },
  });
  const data = await res.json().catch(() => ({}));
  if (res.status === 401) leaveIfRevoked();
  if (!res.ok) throw new ApiError(data.error ? data.error.message : `HTTP ${res.status}`);
  return data as T;
}

export const post = <T = any>(path: string, body?: unknown) =>
  api<T>(path, { method: "POST", body: body === undefined ? undefined : JSON.stringify(body) });

export async function download(path: string, name: string) {
  const res = await fetch(path, { headers: { ...AUTH } });
  if (!res.ok) {
    alert(`Esportazione non riuscita (HTTP ${res.status})`);
    return;
  }
  const url = URL.createObjectURL(await res.blob());
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export const fmtTime = (iso?: string | null) => (iso ? new Date(iso).toLocaleString("it-IT") : "");
export const fmtClock = (iso?: string | null) => (iso ? new Date(iso).toLocaleTimeString("it-IT") : "");
export const fmtUptime = (s: number) =>
  s < 3600 ? `${Math.floor(s / 60)} min` : `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min`;

export const pref = (k: string, v?: boolean): boolean => {
  try {
    if (v === undefined) return localStorage.getItem(k) === "1";
    localStorage.setItem(k, v ? "1" : "0");
  } catch {
    /* archiviazione non disponibile */
  }
  return !!v;
};

/** Esegue un'azione e mostra l'errore all'utente, come faceva l'interfaccia precedente. */
export const guard = (fn: () => Promise<unknown>) => () => {
  fn().catch((e: Error) => {
    if (e.message !== "Annullato") alert(e.message);
  });
};

/**
 * Chat in streaming (ADR-0025): `onDelta` riceve il testo man mano che il modello lo genera; la promessa
 * restituisce l'esito completo, identico a `POST /api/chat`.
 */
export async function chatStream(message: string, onDelta: (text: string) => void): Promise<any> {
  const res = await fetch("/api/chat/stream", {
    method: "POST",
    headers: { "Content-Type": "application/json", ...AUTH },
    body: JSON.stringify({ message }),
  });
  if (!res.ok || !res.body) {
    const data = await res.json().catch(() => ({}));
    if (res.status === 401) leaveIfRevoked();
    throw new ApiError(data.error ? data.error.message : `HTTP ${res.status}`);
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (value) buffer += decoder.decode(value, { stream: true });
    let nl: number;
    while ((nl = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, nl).trim();
      buffer = buffer.slice(nl + 1);
      if (!line) continue;
      const event = JSON.parse(line);
      if (event.type === "delta") onDelta(event.text);
      else if (event.type === "done") return event.result;
      else if (event.type === "error") throw new ApiError(event.error?.message || "errore del Core");
    }
    if (done) throw new ApiError("risposta interrotta prima della fine");
  }
}
