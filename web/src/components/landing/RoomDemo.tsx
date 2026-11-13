import { Pause, Play } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { MESSAGES, READERS, SEATS, mayInstruct, pointOnRing, seat, trustFor, type Reader } from "./room";
import { TrustTag } from "./TrustTag";

const SEAT_R = 34;
const HUB_R = 12;
const STEP_MS = 3000;
const TRAVEL_MS = 1150;

function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(() => window.matchMedia("(prefers-reduced-motion: reduce)").matches);
  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const on = (e: MediaQueryListEvent) => setReduced(e.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  return reduced;
}

/** The hero's working sample: a room drawn as a circle, and the lines an agent would read from it. */
export function RoomDemo() {
  const { t } = usePrefs();
  const reduced = useReducedMotion();
  const [reader, setReader] = useState<Reader>("frontend-ai");
  const [active, setActive] = useState(0);
  const [hovering, setHovering] = useState(false);
  const [paused, setPaused] = useState(false);
  const [visible, setVisible] = useState(true);
  const rootRef = useRef<HTMLDivElement>(null);
  const dotRef = useRef<SVGCircleElement>(null);

  // Pause the loop when the demo is off-screen.
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const io = new IntersectionObserver(([e]) => setVisible(Boolean(e?.isIntersecting)), { threshold: 0.15 });
    io.observe(el);
    return () => io.disconnect();
  }, []);

  useEffect(() => {
    if (reduced || hovering || paused || !visible) return;
    const id = window.setInterval(() => setActive((i) => (i + 1) % MESSAGES.length), STEP_MS);
    return () => window.clearInterval(id);
  }, [reduced, hovering, paused, visible]);

  const msg = MESSAGES[active] ?? MESSAGES[0]!;
  const from = seat(msg.author);
  const to = msg.to ? seat(msg.to) : undefined;

  // Move one dot along author → hub → recipient. Written to the DOM directly; no re-render per frame.
  useEffect(() => {
    const dot = dotRef.current;
    if (!dot || !from) return;
    const a = pointOnRing(from.angle, SEAT_R);
    const b = to ? pointOnRing(to.angle, SEAT_R) : { x: 50, y: 50 };
    if (reduced) {
      dot.setAttribute("opacity", "0");
      return;
    }
    let raf = 0;
    const start = performance.now();
    const ease = (x: number) => 1 - Math.pow(1 - x, 3);
    const tick = (now: number) => {
      const p = Math.min(1, (now - start) / TRAVEL_MS);
      const e = ease(p);
      let x: number;
      let y: number;
      if (!to) {
        x = a.x + (50 - a.x) * e;
        y = a.y + (50 - a.y) * e;
      } else if (e < 0.5) {
        const k = e / 0.5;
        x = a.x + (50 - a.x) * k;
        y = a.y + (50 - a.y) * k;
      } else {
        const k = (e - 0.5) / 0.5;
        x = 50 + (b.x - 50) * k;
        y = 50 + (b.y - 50) * k;
      }
      dot.setAttribute("cx", x.toFixed(2));
      dot.setAttribute("cy", y.toFixed(2));
      dot.setAttribute("opacity", p < 0.92 ? "1" : String((1 - p) / 0.08));
      if (p < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [active, from, to, reduced]);

  const petals = useMemo(() => Array.from({ length: 12 }, (_, i) => i * 30), []);

  return (
    <div ref={rootRef} className="relative" aria-label={t.demo.label} role="figure">
      <div className="mb-4 flex flex-wrap items-center gap-x-3 gap-y-2">
        <span id="read-as" className="text-sm text-field-muted">
          {t.demo.readAs}
        </span>
        <div role="group" aria-labelledby="read-as" className="inline-flex rounded-md border border-field-rule p-0.5">
          {READERS.map((r) => (
            <button
              key={r}
              type="button"
              aria-pressed={reader === r}
              onClick={() => setReader(r)}
              className={cn(
                "h-8 cursor-pointer rounded-[5px] px-3 font-mono text-[0.72rem] transition-colors duration-200 touch-manipulation",
                reader === r ? "bg-saffron text-[oklch(0.22_0.08_268)]" : "text-field-muted hover:text-field-fg",
              )}
            >
              {r}
            </button>
          ))}
        </div>
        {reduced ? null : (
          <button
            type="button"
            onClick={() => setPaused((p) => !p)}
            aria-label={paused ? t.demo.play : t.demo.pause}
            title={paused ? t.demo.play : t.demo.pause}
            className="ml-auto inline-flex size-8 cursor-pointer items-center justify-center rounded-md border border-field-rule text-field-muted transition-colors hover:text-field-fg touch-manipulation"
          >
            {paused ? <Play className="size-3.5" aria-hidden="true" /> : <Pause className="size-3.5" aria-hidden="true" />}
          </button>
        )}
      </div>

      <div className="relative mx-auto aspect-square w-full max-w-[27rem]">
        <svg viewBox="0 0 100 100" className="absolute inset-0 size-full overflow-visible" aria-hidden="true">
          {/* plate rim */}
          <circle cx="50" cy="50" r="47" fill="none" stroke="var(--field-rule)" strokeWidth="0.35" />
          <circle cx="50" cy="50" r="44.5" fill="none" stroke="var(--glaze)" strokeOpacity="0.55" strokeWidth="1.6" strokeDasharray="0.4 2.05" />
          <circle
            cx="50"
            cy="50"
            r={SEAT_R}
            fill="none"
            stroke="var(--field-muted)"
            strokeOpacity="0.45"
            strokeWidth="0.4"
            className="ring-draw"
            style={{ ["--len" as string]: 2 * Math.PI * SEAT_R }}
          />
          {/* spokes */}
          {SEATS.map((s) => {
            const p = pointOnRing(s.angle, SEAT_R);
            const q = pointOnRing(s.angle, HUB_R + 1.5);
            const lit = s.handle === msg.author || s.handle === msg.to;
            return (
              <line
                key={s.handle}
                x1={p.x}
                y1={p.y}
                x2={q.x}
                y2={q.y}
                stroke={lit ? "var(--glaze)" : "var(--field-rule)"}
                strokeWidth={lit ? 0.7 : 0.45}
                style={{ transition: "stroke 400ms ease-out" }}
              />
            );
          })}
          {/* hub medallion */}
          <g>
            {petals.map((a) => (
              <ellipse
                key={a}
                cx="50"
                cy={50 - HUB_R - 2.6}
                rx="1.3"
                ry="2.8"
                fill="var(--glaze)"
                fillOpacity="0.35"
                transform={`rotate(${a} 50 50)`}
              />
            ))}
            <circle cx="50" cy="50" r={HUB_R} fill="var(--field-panel)" stroke="var(--glaze)" strokeWidth="0.6" />
            <circle
              key={`pulse-${active}`}
              cx="50"
              cy="50"
              r={HUB_R}
              fill="none"
              stroke="var(--saffron)"
              strokeWidth="0.5"
              className={reduced ? "hidden" : "hub-pulse"}
            />
          </g>
          {/* seats */}
          {SEATS.map((s, i) => {
            const p = pointOnRing(s.angle, SEAT_R);
            const isReader = s.handle === reader;
            return (
              <g key={s.handle} className="seat-in" style={{ ["--d" as string]: `${300 + i * 90}ms` }}>
                {isReader ? (
                  <circle cx={p.x} cy={p.y} r="6.6" fill="none" stroke="var(--saffron)" strokeWidth="0.7" />
                ) : null}
                {s.kind === "human" ? (
                  <circle cx={p.x} cy={p.y} r="4" fill="var(--field-fg)" />
                ) : s.kind === "agent" ? (
                  <>
                    <circle cx={p.x} cy={p.y} r="4" fill="var(--field)" stroke="var(--glaze)" strokeWidth="1.1" />
                    <circle cx={p.x} cy={p.y} r="1.4" fill="var(--glaze)" />
                  </>
                ) : (
                  <circle cx={p.x} cy={p.y} r="3.6" fill="var(--field)" stroke="var(--field-muted)" strokeWidth="0.7" strokeDasharray="1.2 1" />
                )}
              </g>
            );
          })}
          <circle ref={dotRef} cx="50" cy="50" r="1.5" fill="var(--saffron)" opacity="0" />
        </svg>

        {/* hub label */}
        <span className="pointer-events-none absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 font-mono text-[0.62rem] tracking-wide text-field-muted">
          davra-hub
        </span>

        {/* seat labels (HTML, so they stay legible at every size) */}
        {SEATS.map((s) => {
          const p = pointOnRing(s.angle, SEAT_R);
          const above = p.y < 50;
          const trust = trustFor(reader, s.handle);
          return (
            <div
              key={s.handle}
              className={cn(
                "absolute flex -translate-x-1/2 flex-col items-center gap-1 text-center",
                above ? "-translate-y-[calc(100%+1.45rem)]" : "translate-y-[1.45rem]",
              )}
              style={{ left: `${p.x}%`, top: `${p.y}%` }}
            >
              {above ? (
                <>
                  <SeatName handle={s.handle} kind={s.kind} client={s.client} t={t.demo} />
                  <TrustTag trust={trust} />
                </>
              ) : (
                <>
                  <TrustTag trust={trust} />
                  <SeatName handle={s.handle} kind={s.kind} client={s.client} t={t.demo} />
                </>
              )}
            </div>
          );
        })}
      </div>

      {/* what the selected agent actually reads */}
      <ol
        translate="no"
        className="mt-8 space-y-px overflow-hidden rounded-lg border border-field-rule bg-field-panel py-2 font-mono text-[0.7rem] leading-[1.7] sm:text-[0.72rem]"
        onMouseLeave={() => setHovering(false)}
      >
        {MESSAGES.map((m, i) => {
          const trust = trustFor(reader, m.author);
          const isActive = i === active;
          const addressed = m.to === reader;
          return (
            <li
              key={m.id}
              onMouseEnter={() => {
                setHovering(true);
                setActive(i);
              }}
              className={cn(
                "relative px-3.5 py-1 transition-colors duration-300 sm:px-4",
                isActive ? "bg-field-deep" : "",
              )}
            >
              <span
                aria-hidden="true"
                className={cn(
                  "absolute top-[0.85em] left-1.5 size-1 rounded-full transition-opacity duration-300 sm:left-2",
                  addressed ? "bg-saffron opacity-100" : "opacity-0",
                )}
              />
              <span className="text-field-muted">#{m.id}</span>{" "}
              <span className="hidden text-field-muted sm:inline">{m.time} </span>
              <span className="text-field-fg">{m.author}</span> <TrustTag trust={trust} />
              {m.kind ? <span className="text-field-muted"> ({m.kind})</span> : null}
              {m.replyTo ? <span className="text-field-muted"> re#{m.replyTo}</span> : null}
              <span className="text-field-muted">:</span>{" "}
              <span className={mayInstruct(trust) ? "text-field-fg" : "text-field-fg/80"}>
                {m.text.split(/(\[redacted\])/).map((part, k) =>
                  part === "[redacted]" ? (
                    <mark key={k} className="rounded-[3px] bg-field-deep px-0.5 text-glaze">
                      {part}
                    </mark>
                  ) : (
                    <span key={k}>{part}</span>
                  ),
                )}
              </span>
            </li>
          );
        })}
      </ol>
      <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2 text-[0.75rem] text-field-muted">
        <span className="inline-flex items-center gap-1.5">
          <span className="inline-block size-2.5 rounded-[2px] bg-saffron" aria-hidden="true" />
          {t.demo.instruct}
        </span>
        <span className="inline-flex items-center gap-1.5">
          <span className="inline-block size-2.5 rounded-[2px] border border-field-muted" aria-hidden="true" />
          {t.demo.info}
        </span>
        <span className="basis-full sm:basis-auto sm:ml-auto">{t.demo.synthetic}</span>
      </div>
    </div>
  );
}

function SeatName({
  handle,
  kind,
  client,
  t,
}: {
  handle: string;
  kind: "human" | "agent" | "guest";
  client?: string;
  t: { roleHuman: string; roleAgent: string; roleGuest: string };
}) {
  const role = kind === "human" ? t.roleHuman : kind === "agent" ? (client ?? t.roleAgent) : t.roleGuest;
  return (
    <span className="flex flex-col items-center leading-tight">
      <span className="font-mono text-[0.72rem] text-field-fg">{handle}</span>
      <span className="text-[0.66rem] text-field-muted">{role}</span>
    </span>
  );
}
