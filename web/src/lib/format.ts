import { useMemo } from "react";
import { usePrefs } from "./prefs";

// Intl has thin Uzbek data (it prints "-4 min"), so Uzbek relative times are spelled out here.
const UZ: Record<string, string> = { year: "yil", month: "oy", week: "hafta", day: "kun", hour: "soat", minute: "daqiqa" };

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["week", 7 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
];

/** Locale-aware date helpers for the signed-in app. */
export function useFormat() {
  const { lang } = usePrefs();
  return useMemo(() => {
    const locale = lang === "uz" ? "uz-Latn" : "en";
    const rel = new Intl.RelativeTimeFormat(locale, { numeric: "auto", style: "long" });
    const full = new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" });
    const parts = new Intl.DateTimeFormat("en-GB", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false });
    const num = new Intl.NumberFormat(locale);
    return {
      /** "11-12 14:03", the same shape davra mcp prints. */
      line(iso: string) {
        const p = Object.fromEntries(parts.formatToParts(new Date(iso)).map((x) => [x.type, x.value]));
        return `${p.month}-${p.day} ${p.hour}:${p.minute}`;
      },
      full: (iso: string) => full.format(new Date(iso)),
      relative(iso: string) {
        const secs = (Date.parse(iso) - Date.now()) / 1000;
        for (const [unit, size] of UNITS) {
          if (Math.abs(secs) >= size) {
            const n = Math.round(secs / size);
            if (lang !== "uz") return rel.format(n, unit);
            if (unit === "day" && n === -1) return "kecha";
            return n < 0 ? `${-n} ${UZ[unit]} oldin` : `${n} ${UZ[unit]}dan keyin`;
          }
        }
        return lang === "uz" ? "hozirgina" : rel.format(0, "minute");
      },
      num: (n: number) => num.format(n),
    };
  }, [lang]);
}
