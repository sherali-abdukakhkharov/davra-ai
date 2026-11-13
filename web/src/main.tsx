import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "react-router/dom";
import { PrefsProvider } from "@/lib/prefs";
import { createAppRouter } from "@/router";
import "./index.css";

const el = document.getElementById("root");
if (!el) throw new Error("#root missing");

// Dev only: ?mock=1 (or ?mock=fresh) serves the API from src/lib/mock.ts. Stripped from production builds.
if (import.meta.env.DEV) {
  const flag = new URLSearchParams(location.search).get("mock");
  if (flag) sessionStorage.setItem("davra-mock", flag);
  if (flag === "0") sessionStorage.removeItem("davra-mock");
  if (sessionStorage.getItem("davra-mock")) {
    const { installMock } = await import("@/lib/mock");
    installMock({ fresh: sessionStorage.getItem("davra-mock") === "fresh" });
  }
}

const router = createAppRouter();

createRoot(el).render(
  <StrictMode>
    <PrefsProvider>
      <RouterProvider router={router} />
    </PrefsProvider>
  </StrictMode>,
);
