import { getApps, initializeApp } from "@firebase/app";
import { deleteToken, getMessaging, getToken, isSupported } from "@firebase/messaging";
import { api, post, pref } from "./api";

const config = {
  apiKey: import.meta.env.VITE_FIREBASE_API_KEY || "",
  projectId: import.meta.env.VITE_FIREBASE_PROJECT_ID || "",
  appId: import.meta.env.VITE_FIREBASE_APP_ID || "",
  messagingSenderId: import.meta.env.VITE_FIREBASE_SENDER_ID || "",
};
const vapidKey = import.meta.env.VITE_FIREBASE_VAPID_KEY || "";

const configured = Object.values(config).every(Boolean) && Boolean(vapidKey);

async function messaging() {
  if (!configured) {
    throw new Error("Push Web non configurato: completa i parametri Firebase/VAPID.");
  }
  if (!(await isSupported())) {
    throw new Error("Questo browser non supporta Firebase Cloud Messaging.");
  }
  const app = getApps().find((item) => item.name === "[DEFAULT]") || initializeApp(config);
  return getMessaging(app);
}

export async function registerBrowserPush() {
  if (!("Notification" in window) || Notification.permission !== "granted") {
    throw new Error("Consenti le notifiche del browser prima di attivare il push.");
  }
  const worker = await navigator.serviceWorker.ready;
  const token = await getToken(await messaging(), { vapidKey, serviceWorkerRegistration: worker });
  if (!token) throw new Error("Firebase non ha restituito un token push.");
  await post("/api/push/subscriptions", { token, platform: "web" });
  pref("jarvis.push.web", true);
}

export async function unregisterBrowserPush() {
  const fcm = await messaging();
  await deleteToken(fcm);
  await api("/api/push/subscriptions", { method: "DELETE" });
  pref("jarvis.push.web", false);
}
