import { Moon, Sun } from "lucide-react";
import { usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import type { Lang } from "@/i18n/dict";

const LANGS: { id: Lang; label: string; name: string }[] = [
  { id: "en", label: "EN", name: "English" },
  { id: "uz", label: "UZ", name: "Ózbekça" },
];

export function LangToggle({ onField = false }: { onField?: boolean }) {
  const { lang, setLang, t } = usePrefs();
  return (
    <div
      role="group"
      aria-label={t.nav.language}
      className={cn(
        "inline-flex h-9 items-center rounded-md border p-0.5 font-mono text-[0.7rem] font-medium tracking-wide",
        onField ? "border-field-rule" : "border-rule-strong",
      )}
    >
      {LANGS.map((l) => {
        const active = l.id === lang;
        return (
          <button
            key={l.id}
            type="button"
            lang={l.id}
            aria-pressed={active}
            aria-label={l.name}
            onClick={() => setLang(l.id)}
            className={cn(
              "h-full cursor-pointer rounded-[5px] px-2.5 transition-colors duration-150 touch-manipulation",
              onField
                ? active
                  ? "bg-field-fg text-field"
                  : "text-field-muted hover:text-field-fg"
                : active
                  ? "bg-fg text-bg"
                  : "text-fg-muted hover:text-fg",
            )}
          >
            {l.label}
          </button>
        );
      })}
    </div>
  );
}

export function ThemeToggle({ onField = false }: { onField?: boolean }) {
  const { theme, toggleTheme, t } = usePrefs();
  const label = theme === "dark" ? t.nav.themeToLight : t.nav.themeToDark;
  return (
    <button
      type="button"
      onClick={toggleTheme}
      aria-label={label}
      title={label}
      className={cn(
        "inline-flex size-9 cursor-pointer items-center justify-center rounded-md border transition-colors duration-150 touch-manipulation",
        onField
          ? "border-field-rule text-field-fg hover:bg-field-deep"
          : "border-rule-strong text-fg hover:bg-bg-sunk",
      )}
    >
      {theme === "dark" ? <Sun className="size-4" aria-hidden="true" /> : <Moon className="size-4" aria-hidden="true" />}
    </button>
  );
}
