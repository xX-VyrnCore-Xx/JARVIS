// Stato condiviso dell'interfaccia: vista corrente, salute del Core, notifiche e toast, comando alla chat.

import { createContext, useCallback, useContext, useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { ReactNode } from "react";
import { api, DEVICE_MODE, pref } from "./lib/api";
import { voice } from "./lib/voice";
import type { AvatarActivity } from "./lib/avatarActivity";

export type View =
  | "dashboard" | "chat" | "tasks" | "automations" | "memory" | "skills" | "workspace"
  | "connectors" | "devices" | "audit" | "security" | "system";

export interface Notification {
  notification_id: string;
  severity: "info" | "success" | "warning" | "error";
  category: string;
  title: string;
  body?: string;
  created_at: string;
  read_at?: string | null;
  dedupe_key?: string | null;
  task_id?: string | null;
  link?: { view?: View } | null;
}

export interface Health {
  core?: { version: string; status: string; secrets_store?: string };
  ai?: { status: string; gateway?: any; error?: { message: string } };
  audit?: { events: number; chain_valid: boolean };
  web?: { enabled: boolean; provider?: string; provider_url?: string };
  busy?: boolean;
}

interface Ctx {
  view: View;
  show: (v: View) => void;
  health: Health | null;
  reachable: boolean;
  refreshStatus: () => void;
  toasts: Notification[];
  toast: (n: Notification) => void;
  dismissToast: (key: string) => void;
  unread: number;
  setUnread: (n: number) => void;
  pollNotifications: () => void;
  notifTick: number;
  thinking: boolean;
  setThinking: (b: boolean) => void;
  avatarActivity: AvatarActivity;
  setAvatarActivity: (activity: AvatarActivity) => void;
  /** Invia un messaggio alla chat (registrato dalla vista Chat). */
  sendRef: React.MutableRefObject<(text: string) => void>;
  /** Contatore che cambia dopo ogni risposta: le viste dati si aggiornano. */
  activity: number;
  bumpActivity: () => void;
}

const AppContext = createContext<Ctx | null>(null);

export function useApp(): Ctx {
  const c = useContext(AppContext);
  if (!c) throw new Error("AppContext assente");
  return c;
}

/** Stato del motore vocale come valore React. */
export function useVoice() {
  return useSyncExternalStore(
    (fn) => voice.subscribe(fn),
    () => `${voice.state}|${voice.micOpen}|${voice.pushing}|${voice.available}|${voice.handsFree}|${voice.replies}|${!!voice.status?.tts}`,
  );
}

const toastKey = (n: Notification) => n.dedupe_key || n.notification_id;

export function AppProvider({ children }: { children: ReactNode }) {
  const [view, setView] = useState<View>(DEVICE_MODE ? "chat" : "dashboard");
  const [health, setHealth] = useState<Health | null>(null);
  const [reachable, setReachable] = useState(true);
  const [toasts, setToasts] = useState<Notification[]>([]);
  const [unread, setUnread] = useState(0);
  const [notifTick, setNotifTick] = useState(0);
  const [thinking, setThinking] = useState(false);
  const [avatarActivity, setAvatarActivity] = useState<AvatarActivity>({ category: "general" });
  const [activity, setActivity] = useState(0);
  const sendRef = useRef<(text: string) => void>(() => {});
  const notifSince = useRef<string | null>(null);
  const notifPrimed = useRef(false);
  const lastUnread = useRef<number | null>(null);

  const refreshStatus = useCallback(() => {
    api<Health>("/api/health")
      .then((h) => {
        setHealth(h);
        setReachable(true);
        voice.setStatus(h.ai?.gateway?.voice ?? null);
      })
      .catch(() => setReachable(false));
  }, []);

  const dismissToast = useCallback((key: string) => setToasts((t) => t.filter((x) => toastKey(x) !== key)), []);

  const toast = useCallback((n: Notification) => {
    setToasts((t) => [...t.filter((x) => toastKey(x) !== toastKey(n)), n]);
    setTimeout(() => setToasts((t) => t.filter((x) => x !== n)), 8000);
    if ("Notification" in window && Notification.permission === "granted" && document.hidden && !pref("jarvis.push.web")) {
      try {
        new Notification(`JARVIS · ${n.title}`, { body: n.body || "", tag: toastKey(n) });
      } catch {
        /* non supportato */
      }
    }
  }, []);

  const pollNotifications = useCallback(() => {
    const since = notifSince.current ? "&since=" + encodeURIComponent(notifSince.current) : "";
    api<{ unread: number; items: Notification[] }>(`/api/notifications?limit=20${since}`)
      .then((data) => {
        setUnread(data.unread);
        if (data.items.length) notifSince.current = data.items[0].created_at;
        if (notifPrimed.current) data.items.filter((n) => !n.read_at).reverse().forEach(toast);
        notifPrimed.current = true;
        if (data.items.length || lastUnread.current !== data.unread) setNotifTick((x) => x + 1);
        lastUnread.current = data.unread;
      })
      .catch(() => {
        /* il Core può essere momentaneamente occupato */
      });
  }, [toast]);

  useEffect(() => {
    refreshStatus();
    pollNotifications();
    const a = setInterval(refreshStatus, 15000);
    const b = setInterval(pollNotifications, 5000);
    const online = () => {
      refreshStatus();
      pollNotifications();
    };
    window.addEventListener("online", online);
    return () => {
      clearInterval(a);
      clearInterval(b);
      window.removeEventListener("online", online);
    };
  }, [refreshStatus, pollNotifications]);

  const value: Ctx = {
    view,
    show: setView,
    health,
    reachable,
    refreshStatus,
    toasts,
    toast,
    dismissToast,
    unread,
    setUnread,
    pollNotifications,
    notifTick,
    thinking,
    setThinking,
    avatarActivity,
    setAvatarActivity,
    sendRef,
    activity,
    bumpActivity: useCallback(() => setActivity((x) => x + 1), []),
  };
  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

/** Carica dati quando la vista è attiva (e a ogni `deps`), con ricarica manuale. */
export function useViewData<T>(active: boolean, load: () => Promise<T>, deps: unknown[] = [], pollMs?: number) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(() => {
    load()
      .then((d) => {
        setData(d);
        setError(null);
      })
      .catch((e: Error) => setError(e.message));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  useEffect(() => {
    if (!active) return;
    reload();
    if (!pollMs) return;
    const poll = () => { if (!document.hidden) reload(); };
    const timer = window.setInterval(poll, pollMs);
    document.addEventListener("visibilitychange", poll);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", poll);
    };
  }, [active, reload, pollMs]);
  return { data, error, reload, setData };
}
