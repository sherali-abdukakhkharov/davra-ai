import { useMemo } from "react";
import { fill, usePrefs } from "./prefs";

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

/** A hub calendar day (YYYY-MM-DD) as a Date at UTC midnight, so formatting never shifts it. */
export const dayDate = (day: string) => new Date(`${day}T00:00:00Z`);

/** The `n` calendar days starting at `since`, as YYYY-MM-DD. */
export function dayRange(since: string, n: number): string[] {
  const start = dayDate(since).getTime();
  return Array.from({ length: n }, (_, i) => new Date(start + i * 86_400_000).toISOString().slice(0, 10));
}

/** Locale-aware date and number helpers for the signed-in app. */
export function useFormat() {
  const { lang, t } = usePrefs();
  return useMemo(() => {
    const locale = lang === "uz" ? "uz-Latn" : "en";
    const rel = new Intl.RelativeTimeFormat(locale, { numeric: "auto", style: "long" });
    const full = new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" });
    const parts = new Intl.DateTimeFormat("en-GB", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false });
    const num = new Intl.NumberFormat(locale);
    const compact = new Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 });
    const usd2 = new Intl.NumberFormat(locale, { style: "currency", currency: "USD", minimumFractionDigits: 2, maximumFractionDigits: 2 });
    const usd0 = new Intl.NumberFormat(locale, { style: "currency", currency: "USD", maximumFractionDigits: 0 });
    const usdCompact = new Intl.NumberFormat(locale, { style: "currency", currency: "USD", notation: "compact", maximumFractionDigits: 1 });
    const pct = new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: 0 });
    const u = t.app.usage.units;
    const cal = t.app.usage;
    // Browsers' Uzbek calendar data is thin ("M11 9"), so day labels are spelled out from the dictionary.
    const dayParts = (day: string) => {
      const d = dayDate(day);
      return { d: d.getUTCDate(), month: cal.months[d.getUTCMonth()]!, wd: cal.weekdays[d.getUTCDay()]! };
    };
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
      /** 950, 12.9K, 1.2M. */
      compact: (n: number) => (Math.abs(n) < 1000 ? num.format(Math.round(n)) : compact.format(n)),
      /** $3.40, <$0.01, $1.2K. */
      usd(n: number) {
        if (n > 0 && n < 0.01) return `<${usd2.format(0.01)}`;
        return Math.abs(n) >= 10_000 ? usdCompact.format(n) : usd2.format(n);
      },
      /** Axis ticks: whole dollars once the step allows it. */
      usdTick: (n: number, step: number) => (step >= 1 ? (n >= 10_000 ? usdCompact.format(n) : usd0.format(n)) : usd2.format(n)),
      pct: (n: number) => pct.format(n),
      /** 2h 15m, 45m, <1m. */
      duration(secs: number) {
        if (secs <= 0) return fill(u.m, { m: 0 });
        if (secs < 60) return u.lessMin;
        const h = Math.floor(secs / 3600);
        const m = Math.round((secs % 3600) / 60);
        if (!h) return fill(u.m, { m });
        if (h >= 100 || m === 0 || m === 60) return fill(u.h, { h: num.format(m === 60 ? h + 1 : h) });
        return fill(u.hm, { h, m });
      },
      /** "Nov 3" for a YYYY-MM-DD day. */
      day: (day: string) => fill(cal.dayShort, dayParts(day)),
      /** "Tue, Nov 3". */
      dayLong: (day: string) => fill(cal.dayLong, dayParts(day)),
    };
  }, [lang, t]);
}
