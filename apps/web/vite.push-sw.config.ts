import { defineConfig, loadEnv } from "vite";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "VITE_");
  const config = {
    apiKey: env.VITE_FIREBASE_API_KEY || "",
    projectId: env.VITE_FIREBASE_PROJECT_ID || "",
    appId: env.VITE_FIREBASE_APP_ID || "",
    messagingSenderId: env.VITE_FIREBASE_SENDER_ID || "",
  };
  return {
    define: { __JARVIS_FIREBASE_CONFIG__: JSON.stringify(config) },
    build: {
      outDir: "../../core/server/static",
      emptyOutDir: false,
      target: "es2020",
      sourcemap: false,
      minify: true,
      lib: {
        entry: "src/push-sw.ts",
        formats: ["iife"],
        name: "JarvisPushWorker",
        fileName: () => "fcm-sw.js",
      },
    },
  };
});
