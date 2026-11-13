import type { ReactNode } from "react";
import { Link } from "react-router";
import { usePrefs } from "@/lib/prefs";
import { TrustTag } from "@/components/landing/TrustTag";
import { MESSAGES, trustFor } from "@/components/landing/room";
import { Logo } from "./Logo";

import { SkipLink } from "./Nav";
import { LangToggle, ThemeToggle } from "./Prefs";

const SAMPLE = MESSAGES.filter((m) => [118, 121, 122].includes(m.id));

/** A small, still version of the landing room: the seats around the hub. */
function Plate() {
  const seats = [312, 246, 48, 114, 180];
  const pt = (a: number, r: number) => {
    const rad = ((a - 90) * Math.PI) / 180;
    return { x: 50 + r * Math.cos(rad), y: 50 + r * Math.sin(rad) };
  };
  return (
    <svg viewBox="0 0 100 100" className="size-full" aria-hidden="true">
      <circle cx="50" cy="50" r="47" fill="none" stroke="var(--field-rule)" strokeWidth="0.35" />
      <circle cx="50" cy="50" r="44.5" fill="none" stroke="var(--glaze)" strokeOpacity="0.5" strokeWidth="1.6" strokeDasharray="0.4 2.05" />
      <circle cx="50" cy="50" r="34" fill="none" stroke="var(--field-muted)" strokeOpacity="0.4" strokeWidth="0.4" />
      {seats.map((a) => {
        const p = pt(a, 34);
        const q = pt(a, 13.5);
        return <line key={`l${a}`} x1={p.x} y1={p.y} x2={q.x} y2={q.y} stroke="var(--field-rule)" strokeWidth="0.45" />;
      })}
      {Array.from({ length: 12 }, (_, i) => (
        <ellipse key={i} cx="50" cy="35.4" rx="1.3" ry="2.8" fill="var(--glaze)" fillOpacity="0.35" transform={`rotate(${i * 30} 50 50)`} />
      ))}
      <circle cx="50" cy="50" r="12" fill="var(--field-panel)" stroke="var(--glaze)" strokeWidth="0.6" />
      <circle cx="50" cy="50" r="3" fill="var(--saffron)" />
      {seats.map((a, i) => {
        const p = pt(a, 34);
        if (i % 2 === 1)
          return (
            <g key={a}>
              <circle cx={p.x} cy={p.y} r="4" fill="var(--field)" stroke="var(--glaze)" strokeWidth="1.1" />
              <circle cx={p.x} cy={p.y} r="1.4" fill="var(--glaze)" />
            </g>
          );
        if (i === 4)
          return <circle key={a} cx={p.x} cy={p.y} r="3.6" fill="var(--field)" stroke="var(--field-muted)" strokeWidth="0.7" strokeDasharray="1.2 1" />;
        return <circle key={a} cx={p.x} cy={p.y} r="4" fill="var(--field-fg)" />;
      })}
    </svg>
  );
}

export function AuthLayout({ children }: { children: ReactNode }) {
  const { t } = usePrefs();
  return (
    <div className="min-h-dvh lg:grid *:min-w-0 lg:grid-cols-[minmax(0,0.95fr)_minmax(0,1.05fr)]">
      <SkipLink />
      <aside data-field className="relative isolate z-10 bg-field text-field-fg">
        <div className="flex h-16 items-center justify-between px-4 sm:px-6 lg:h-20 lg:px-10">
          <Link to="/" aria-label={t.nav.home} className="-ml-1 rounded-md px-1 py-1">
            <Logo />
          </Link>
          <div className="flex items-center gap-2 lg:hidden">
            <LangToggle onField />
            <ThemeToggle onField />
          </div>
        </div>
        <div className="mx-auto hidden w-full max-w-[34rem] px-10 pb-20 lg:flex lg:min-h-[calc(100dvh-5rem)] lg:flex-col lg:justify-center">
          <div className="aspect-square w-full max-w-[17rem] -ml-3">
            <Plate />
          </div>
          <p className="display mt-10 max-w-[16ch] text-[clamp(2rem,3vw,2.75rem)]">{t.auth.asideLine}</p>
          <p className="mt-4 max-w-[28rem] text-field-muted">{t.auth.asideSub}</p>
          <ul className="mt-10 max-w-[30rem] space-y-2 border-t border-field-rule pt-6 font-mono text-[0.72rem] leading-relaxed text-field-muted">
            {SAMPLE.map((m) => (
              <li key={m.id} className="flex flex-wrap items-center gap-x-2 gap-y-1">
                <span>#{m.id}</span>
                <span className="text-field-fg">{m.author}</span>
                <TrustTag trust={trustFor("nodir-ai", m.author)} />
                <span className="min-w-0 truncate">{m.text}</span>
              </li>
            ))}
          </ul>
        </div>
        <div aria-hidden="true" className="rim absolute inset-x-0 -bottom-3 lg:hidden" />
      </aside>
      <main id="main" tabIndex={-1} className="relative flex flex-col outline-none">
        <div className="hidden h-20 items-center justify-end gap-2 px-10 lg:flex">
          <LangToggle />
          <ThemeToggle />
        </div>
        <div className="flex flex-1 items-start justify-center px-4 pt-14 pb-16 sm:px-6 lg:items-center lg:pt-0 lg:pb-24">
          <div className="w-full max-w-[25rem]">{children}</div>
        </div>
      </main>
    </div>
  );
}
