import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Bundle unico e a nome fisso (app.js, app.css) in core/server/static: il Core lo include con include_str!
// e lo serve con la CSP `script-src 'self'`. Formato IIFE perché boot.js lo carica come script classico.
export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "../../core/server/static",
    emptyOutDir: false,
    target: "es2020",
    sourcemap: false,
    minify: true,
    cssCodeSplit: false,
    lib: {
      entry: "src/main.tsx",
      formats: ["iife"],
      name: "JarvisDashboard",
      fileName: () => "app.js",
      cssFileName: "app",
    },
  },
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
});
