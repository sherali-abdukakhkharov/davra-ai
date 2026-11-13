import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { dictionaries, type Dict, type Lang } from "@/i18n/dict";

type Theme = "light" | "dark";

type Prefs = {
  lang: Lang;
  setLang: (l: Lang) => void;
  t: Dict;
  theme: Theme;
  toggleTheme: () => void;
};

const PrefsContext = createContext<Prefs | null>(null);

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Private mode or blocked storage: the choice just won't persist.
  }
}

function initialTheme(): Theme {
  const attr = document.documentElement.getAttribute("data-theme");
  return attr === "dark" ? "dark" : "light";
}

/** `title` picks the document title from the dictionary; the website uses meta.title. */
export function PrefsProvider({ children, title }: { children: ReactNode; title?: (t: Dict) => string }) {
  const [lang, setLangState] = useState<Lang>(() => (read("davra-lang") === "uz" ? "uz" : "en"));
  const [theme, setTheme] = useState<Theme>(initialTheme);

  useEffect(() => {
    document.documentElement.lang = lang;
    document.title = title ? title(dictionaries[lang]) : dictionaries[lang].meta.title;
  }, [lang, title]);

  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute("data-theme", theme);
    root.style.colorScheme = theme;
    // Browser chrome matches the cobalt field the page opens on.
    const field = getComputedStyle(root).getPropertyValue("--field").trim();
    if (field) {
      document.querySelectorAll('meta[name="theme-color"]').forEach((m) => m.remove());
      const meta = document.createElement("meta");
      meta.name = "theme-color";
      meta.content = field;
      document.head.appendChild(meta);
    }
  }, [theme]);

  // Follow the OS until the visitor picks a theme explicitly.
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) => {
      const stored = read("davra-theme");
      if (stored !== "light" && stored !== "dark") setTheme(e.matches ? "dark" : "light");
    };
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const setLang = useCallback((l: Lang) => {
    setLangState(l);
    write("davra-lang", l);
  }, []);

  const toggleTheme = useCallback(() => {
    setTheme((prev) => {
      const next = prev === "dark" ? "light" : "dark";
      write("davra-theme", next);
      return next;
    });
  }, []);

  const value = useMemo<Prefs>(
    () => ({ lang, setLang, t: dictionaries[lang], theme, toggleTheme }),
    [lang, setLang, theme, toggleTheme],
  );
  return <PrefsContext.Provider value={value}>{children}</PrefsContext.Provider>;
}

export function usePrefs(): Prefs {
  const ctx = useContext(PrefsContext);
  if (!ctx) throw new Error("usePrefs outside PrefsProvider");
  return ctx;
}

/** Replaces {key} placeholders. */
export function fill(template: string, vars: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}
