import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
    // To evaluate Preact at the Milestone 1 gate (see docs/PLAN.md appendix), add:
    //   react: "preact/compat", "react-dom": "preact/compat"
  },
  clearScreen: false,
  build: {
    // WKWebView (macOS 13+) and WebView2 (Edge 110+) are the only targets we ship to.
    target: ["safari16", "edge110"],
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks: {
          // keep canvas + device views off the cold-start path
          canvas: ["@/canvas/index"],
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
});
