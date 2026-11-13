import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Dev hub to proxy to. Defaults to the seeded local dev hub; set VITE_HUB to point elsewhere.
const hub = process.env.VITE_HUB ?? "http://127.0.0.1:8788";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  server: {
    proxy: {
      "/api": { target: hub, changeOrigin: false },
      "/.well-known": { target: hub, changeOrigin: false },
    },
  },
  preview: {
    proxy: {
      "/api": { target: hub, changeOrigin: false },
      "/.well-known": { target: hub, changeOrigin: false },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
  },
});
