import { getApps, initializeApp } from "@firebase/app";
import { getMessaging, onBackgroundMessage } from "@firebase/messaging/sw";
import type { FirebaseOptions } from "@firebase/app";

declare const __JARVIS_FIREBASE_CONFIG__: FirebaseOptions;

const config = __JARVIS_FIREBASE_CONFIG__;
if (config.apiKey && config.projectId && config.appId && config.messagingSenderId) {
  const app = getApps().find((item) => item.name === "[DEFAULT]") || initializeApp(config);
  const messaging = getMessaging(app);
  const worker = globalThis as typeof globalThis & { registration: ServiceWorkerRegistration };
  onBackgroundMessage(messaging, (payload) => {
    const title = payload.notification?.title || payload.data?.title || "JARVIS";
    const body = payload.notification?.body || "Hai una nuova notifica.";
    return worker.registration.showNotification(title, {
      body,
      icon: "/icon.svg",
      tag: payload.data?.notification_id || "jarvis",
      data: { url: "/" },
    });
  });
}
