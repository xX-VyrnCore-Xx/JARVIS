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

export async function api<T = any>(path: string, options: RequestInit = {}): Promise<T> {
  const res = await fetch(path, {
    ...options,
    headers: { "Content-Type": "application/json", ...AUTH, ...((options.headers as Record<string, string>) || {}) },
  });
  const data = await res.json().catch(() => ({}));
  if (DEVICE_MODE && res.status === 401 && !window.jarvisLeaving) {
    // Dispositivo revocato: una sola navigazione, anche se più richieste falliscono insieme.
    window.jarvisLeaving = true;
    try {
      localStorage.removeItem("jarvis-device");
    } catch {
      /* niente */
    }
    location.replace("/pair");
  }
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
