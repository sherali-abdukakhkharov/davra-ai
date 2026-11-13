import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import type { Dict } from "@/i18n/dict";
import { PrefsProvider } from "@/lib/prefs";
import { App } from "./App";
import "@/index.css";

const el = document.getElementById("root");
if (!el) throw new Error("#root missing");

const title = (t: Dict) => t.desktop.title;

createRoot(el).render(
  <StrictMode>
    <PrefsProvider title={title}>
      <App />
    </PrefsProvider>
  </StrictMode>,
);
