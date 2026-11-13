import { existsSync, renameSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

// Davra Desktop UI: builds only desktop.html into dist-desktop for the Tauri shell
// (desktop/src-tauri). Relative asset paths, because Tauri serves it from its own origin.
// `npm run dev:desktop` serves it in a browser with the mock bridge (src/desktop/mock.ts).

/** Serves desktop.html at "/" in dev and writes it as index.html, Tauri's default entry. */
function desktopIndex(): Plugin {
  return {
    name: "davra-desktop-index",
    configureServer(server) {
      server.middlewares.use((req, _res, next) => {
        if (req.url === "/" || req.url?.startsWith("/?")) req.url = `/desktop.html${req.url.slice(1)}`;
        next();
      });
    },
    writeBundle(options) {
      const dir = options.dir ?? "dist-desktop";
      const from = join(dir, "desktop.html");
      if (existsSync(from)) renameSync(from, join(dir, "index.html"));
    },
  };
}

export default defineConfig({
  base: "./",
  plugins: [react(), tailwindcss(), desktopIndex()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  // The website's public/ (llms.txt, robots.txt…) does not belong in the app.
  publicDir: false,
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "dist-desktop",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
    rolldownOptions: {
      input: { desktop: fileURLToPath(new URL("./desktop.html", import.meta.url)) },
    },
  },
});
