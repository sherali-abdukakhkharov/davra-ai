import { Activity, ShieldCheck } from "lucide-react";
import { useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useSearchParams } from "react-router";
import { CodeBlock } from "@/components/site/CodeBlock";
import { Button } from "@/components/ui/button";
import { dayRange, useFormat } from "@/lib/format";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { USAGE_DAYS, web, type Usage, type UsageDay, type UsageMember, type UsageModel, type UsageReport } from "@/lib/web";
import { ErrorPanel, Skeleton } from "./States";
import { useResource } from "./useResource";

const DEFAULT_DAYS = 14;

const tokensOf = (u: Pick<Usage, "input_tokens" | "output_tokens" | "cache_read_tokens" | "cache_write_tokens">) =>
  u.input_tokens + u.output_tokens + u.cache_read_tokens + u.cache_write_tokens;

const isEmpty = (r: UsageReport) => r.daily.length === 0 && r.members.length === 0 && r.models.length === 0;

export function UsageTab({ org, slug }: { org: string; slug: string }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  const [params, setParams] = useSearchParams();
  const raw = Number(params.get("days"));
  const days = (USAGE_DAYS as readonly number[]).includes(raw) ? raw : DEFAULT_DAYS;
  const res = useResource((s) => web.usage(org, slug, days, s), [org, slug, days]);

  // A range change refetches; keep the previous report on screen (dimmed) instead of flashing a skeleton.
  const key = `${org}/${slug}`;
  const last = useRef<{ key: string; data: UsageReport } | null>(null);
  if (res.data) last.current = { key, data: res.data };
  const data = res.data ?? (last.current?.key === key ? last.current.data : undefined);
  const stale = !res.data && !!data;

  const setDays = (n: number) =>
    setParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (n === DEFAULT_DAYS) next.delete("days");
        else next.set("days", String(n));
        return next;
      },
      { replace: true },
    );

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-end justify-between gap-x-6 gap-y-4">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{u.title}</h2>
          <p className="mt-1 max-w-[44rem] text-sm text-fg-muted">{fill(u.sub, { tz: data?.tz ?? "UTC" })}</p>
        </div>
        <RangePicker value={days} onChange={setDays} />
      </div>

      {res.error && !data ? (
        <ErrorPanel error={res.error} onRetry={() => void res.reload()} />
      ) : !data ? (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_18rem]">
          <Skeleton className="h-80" />
          <Skeleton className="h-80" />
        </div>
      ) : isEmpty(data) ? (
        <Empty days={days} setUp={data.first_day != null} onLonger={days < 90 ? () => setDays(90) : undefined} />
      ) : (
        <div aria-busy={stale} className={cn("flex flex-col gap-8 transition-opacity duration-200", stale && "opacity-55")}>
          <Report r={data} />
        </div>
      )}
    </div>
  );
}

function RangePicker({ value, onChange }: { value: number; onChange: (n: number) => void }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const onKey = (e: KeyboardEvent, i: number) => {
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const j = (i + step + USAGE_DAYS.length) % USAGE_DAYS.length;
    onChange(USAGE_DAYS[j]!);
    refs.current[j]?.focus();
  };
  return (
    <div role="radiogroup" aria-label={u.range} className="inline-flex rounded-md border border-rule-strong bg-bg-sunk/60 p-0.5">
      {USAGE_DAYS.map((n, i) => {
        const on = n === value;
        return (
          <button
            key={n}
            ref={(el) => {
              refs.current[i] = el;
            }}
            type="button"
            role="radio"
            aria-checked={on}
            aria-label={fill(u.rangeLong, { n })}
            tabIndex={on ? 0 : -1}
            onClick={() => onChange(n)}
            onKeyDown={(e) => onKey(e, i)}
            className={cn(
              "tnum h-9 min-w-14 cursor-pointer rounded-[6px] px-3 text-sm transition-colors touch-manipulation",
              on ? "bg-bg-raised font-medium text-fg shadow-[0_1px_2px_hsl(var(--shadow-color)/0.12)]" : "text-fg-muted hover:text-fg",
            )}
          >
            {fill(u.rangeShort, { n })}
          </button>
        );
      })}
    </div>
  );
}

function Report({ r }: { r: UsageReport }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  const f = useFormat();
  const x = r.totals;
  const decided = x.edits_accepted + x.edits_rejected;
  const tiles: { label: string; value: string; sub?: string }[] = [
    { label: u.activeTime, value: f.duration(x.active_secs) },
    { label: u.sessions, value: f.compact(x.sessions), sub: x.sessions ? fill(u.perSession, { time: f.duration(x.active_secs / x.sessions) }) : undefined },
    { label: u.linesAdded, value: f.compact(x.lines_added), sub: fill(u.linesRemoved, { n: f.compact(x.lines_removed) }) },
    { label: u.commits, value: f.compact(x.commits) },
    { label: u.pullRequests, value: f.compact(x.pull_requests) },
    {
      label: u.editsAccepted,
      value: decided ? f.pct(x.edits_accepted / decided) : "–",
      sub: decided ? fill(u.editsOf, { a: f.compact(x.edits_accepted), n: f.compact(decided) }) : u.noEdits,
    },
  ];
  return (
    <>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_18rem]">
        <CostPanel r={r} />
        <TokensPanel x={x} />
      </div>

      <ul className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-rule bg-rule sm:grid-cols-3 xl:grid-cols-6">
        {tiles.map((s) => (
          <li key={s.label} className="flex min-w-0 flex-col bg-bg-raised px-4 py-3.5">
            <span className="text-[0.8rem] text-fg-muted">{s.label}</span>
            <span className="mt-1 text-2xl font-semibold tracking-tight">{s.value}</span>
            {s.sub ? <span className="mt-0.5 text-[0.78rem] text-fg-muted">{s.sub}</span> : null}
          </li>
        ))}
      </ul>

      {r.members.length ? <MembersTable members={r.members} /> : null}
      {r.models.length ? <Models models={r.models} total={r.models.reduce((a, m) => a + m.cost_usd, 0)} /> : null}
    </>
  );
}

function Panel({ className, children, labelledBy }: { className?: string; children: ReactNode; labelledBy?: string }) {
  return (
    <section aria-labelledby={labelledBy} className={cn("min-w-0 rounded-lg border border-rule bg-bg-raised p-4 sm:p-5", className)}>
      {children}
    </section>
  );
}

function CostPanel({ r }: { r: UsageReport }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  const f = useFormat();
  const id = useId();
  const days = useMemo(() => dayRange(r.since, r.days), [r.since, r.days]);
  const byDay = useMemo(() => new Map(r.daily.map((d) => [d.day, d])), [r.daily]);
  return (
    <Panel labelledBy={id}>
      <p id={id} className="text-sm text-fg-muted">
        {u.cost}
      </p>
      <p className="mt-1 text-[3rem] leading-none font-semibold tracking-tight">{f.usd(r.totals.cost_usd)}</p>
      <p className="mt-2 text-sm text-fg-muted">
        {fill(u.costSub, { avg: f.usd(r.totals.cost_usd / r.days), active: r.daily.length, n: r.days })}
      </p>
      <h3 className="mt-6 text-sm font-medium">{u.chartTitle}</h3>
      <DailyChart days={days} byDay={byDay} />
      <details className="group mt-2">
        <summary className="inline-flex cursor-pointer rounded-sm py-1 text-sm text-fg-muted select-none hover:text-fg">{u.table}</summary>
        <div className="mt-2 max-h-80 overflow-auto rounded-md border border-rule">
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-bg-sunk text-left text-[0.75rem] text-fg-muted">
              <tr>
                <th scope="col" className="px-3 py-2 font-medium">{u.day}</th>
                <th scope="col" className="px-3 py-2 text-right font-medium">{u.cost}</th>
                <th scope="col" className="px-3 py-2 text-right font-medium">{u.tokens}</th>
                <th scope="col" className="px-3 py-2 text-right font-medium">{u.sessions}</th>
                <th scope="col" className="px-3 py-2 text-right font-medium">{u.activeTime}</th>
              </tr>
            </thead>
            <tbody className="tnum divide-y divide-rule">
              {[...days].reverse().map((day) => {
                const d = byDay.get(day);
                return (
                  <tr key={day} className={d ? "" : "text-fg-muted"}>
                    <th scope="row" className="px-3 py-1.5 text-left font-normal whitespace-nowrap">{f.dayLong(day)}</th>
                    <td className="px-3 py-1.5 text-right">{f.usd(d?.cost_usd ?? 0)}</td>
                    <td className="px-3 py-1.5 text-right">{f.compact(d ? tokensOf(d) : 0)}</td>
                    <td className="px-3 py-1.5 text-right">{f.num(d?.sessions ?? 0)}</td>
                    <td className="px-3 py-1.5 text-right whitespace-nowrap">{f.duration(d?.active_secs ?? 0)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </details>
    </Panel>
  );
}

function TokensPanel({ x }: { x: Usage }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  const f = useFormat();
  const id = useId();
  const total = tokensOf(x);
  const rows: [string, number][] = [
    [u.input, x.input_tokens],
    [u.output, x.output_tokens],
    [u.cacheRead, x.cache_read_tokens],
    [u.cacheWrite, x.cache_write_tokens],
  ];
  return (
    <Panel labelledBy={id} className="flex flex-col">
      <p id={id} className="text-sm text-fg-muted">
        {u.tokens}
      </p>
      <p className="mt-1 text-[2rem] leading-none font-semibold tracking-tight">{f.compact(total)}</p>
      <dl className="mt-5 flex flex-col divide-y divide-rule border-t border-rule">
        {rows.map(([label, n]) => (
          <div key={label} className="flex items-baseline justify-between gap-3 py-2.5">
            <dt className="text-sm text-fg-muted">{label}</dt>
            <dd className="tnum flex items-baseline gap-2 text-sm">
              <span className="font-medium">{f.compact(n)}</span>
              <span className="w-10 text-right text-[0.75rem] text-fg-muted">{total ? f.pct(n / total) : "–"}</span>
            </dd>
          </div>
        ))}
      </dl>
    </Panel>
  );
}

/** Measures an element's content width, so the SVG draws at real pixels (crisp hairlines, unscaled text). */
function useWidth<T extends HTMLElement>(fallback: number) {
  const ref = useRef<T>(null);
  const [w, setW] = useState(fallback);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setW(el.clientWidth || fallback);
    const ro = new ResizeObserver(([e]) => e && setW(Math.round(e.contentRect.width)));
    ro.observe(el);
    return () => ro.disconnect();
  }, [fallback]);
  return [ref, w] as const;
}

/** 1, 2, 2.5 or 5 times a power of ten: a step that gives about `count` clean gridlines up to `max`. */
function niceScale(max: number, count = 4) {
  if (!(max > 0)) return { top: 1, step: 0.25 };
  const rough = max / count;
  const pow = 10 ** Math.floor(Math.log10(rough));
  const step = ([1, 2, 2.5, 5, 10].find((m) => m * pow >= rough) ?? 10) * pow;
  return { top: Math.ceil(max / step - 1e-9) * step, step };
}

/** Rounded data-end, square at the baseline. */
function barPath(x: number, y: number, w: number, h: number) {
  const r = Math.min(4, w / 2, h);
  return `M${x},${y + h}V${y + r}Q${x},${y} ${x + r},${y}H${x + w - r}Q${x + w},${y} ${x + w},${y + r}V${y + h}Z`;
}

function DailyChart({ days, byDay }: { days: string[]; byDay: Map<string, UsageDay> }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  const f = useFormat();
  const [ref, width] = useWidth<HTMLDivElement>(640);
  const [active, setActive] = useState<number | null>(null);
  const n = days.length;

  const W = Math.max(240, width);
  const gutter = 52;
  const padR = 12;
  const padT = 22;
  const plotH = W < 480 ? 150 : 190;
  const axisH = 26;
  const H = padT + plotH + axisH;
  const plotW = W - gutter - padR;
  const band = plotW / n;
  const barW = Math.max(1, Math.min(24, band * 0.72, band - 2));

  const values = days.map((d) => byDay.get(d)?.cost_usd ?? 0);
  const max = Math.max(0, ...values);
  const { top, step } = niceScale(max);
  const ticks = Array.from({ length: Math.round(top / step) + 1 }, (_, i) => i * step);
  const yOf = (v: number) => padT + plotH - (v / top) * plotH;
  const cx = (i: number) => gutter + band * i + band / 2;
  const base = padT + plotH;
  const peak = max > 0 ? values.indexOf(max) : -1;
  // Space x labels at least ~64px apart, counting back from the latest day.
  const every = Math.max(1, Math.ceil(64 / band));

  const onKey = (e: KeyboardEvent) => {
    const cur = active ?? n - 1;
    const next =
      e.key === "ArrowRight" ? Math.min(n - 1, cur + 1)
      : e.key === "ArrowLeft" ? Math.max(0, cur - 1)
      : e.key === "Home" ? 0
      : e.key === "End" ? n - 1
      : e.key === "Escape" ? null
      : undefined;
    if (next === undefined) return;
    e.preventDefault();
    setActive(next);
  };

  const a = active === null ? null : { day: days[active]!, d: byDay.get(days[active]!) };
  const tipLeft = active === null ? 0 : Math.min(Math.max(cx(active), 96), W - 96);

  return (
    <div
      ref={ref}
      tabIndex={0}
      role="group"
      aria-label={fill(u.chartLabel, { n })}
      onKeyDown={onKey}
      onFocus={() => setActive((v) => v ?? n - 1)}
      onBlur={() => setActive(null)}
      className="relative mt-3 rounded-md focus-visible:outline-offset-4"
    >
      <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} className="block overflow-visible" aria-hidden="true" onPointerLeave={() => setActive(null)}>
        {ticks.map((v) => {
          const y = Math.round(yOf(v)) + 0.5;
          return (
            <g key={v}>
              <line x1={gutter} x2={W - padR} y1={y} y2={y} stroke={v === 0 ? "var(--rule-strong)" : "var(--rule)"} strokeWidth={1} />
              <text x={gutter - 8} y={y} textAnchor="end" dominantBaseline="middle" className="tnum" fontSize={11} fill="var(--fg-muted)">
                {f.usdTick(v, step)}
              </text>
            </g>
          );
        })}
        {active !== null ? (
          <rect x={gutter + band * active} y={padT - 6} width={band} height={plotH + 6} rx={4} fill="var(--bg-sunk)" />
        ) : null}
        {values.map((v, i) => {
          if (v <= 0) return null;
          const y = yOf(v);
          return (
            <path
              key={days[i]}
              d={barPath(cx(i) - barW / 2, y, barW, base - y)}
              fill={i === active ? "var(--primary-hover)" : "var(--primary)"}
            />
          );
        })}
        {peak >= 0 ? (
          <text
            x={Math.min(Math.max(cx(peak), gutter + 22), W - padR - 22)}
            y={yOf(max) - 7}
            textAnchor="middle"
            fontSize={11}
            fontWeight={600}
            fill="var(--fg)"
            className="tnum"
          >
            {f.usd(max)}
          </text>
        ) : null}
        {days.map((d, i) =>
          (n - 1 - i) % every === 0 ? (
            <text key={d} x={cx(i)} y={base + 17} textAnchor="middle" fontSize={11} fill="var(--fg-muted)">
              {f.day(d)}
            </text>
          ) : null,
        )}
        {/* Hit targets: the whole day column, not just the painted bar. */}
        {days.map((d, i) => (
          <rect key={d} x={gutter + band * i} y={padT - 6} width={band} height={plotH + 6} fill="transparent" onPointerEnter={() => setActive(i)} onPointerDown={() => setActive(i)} />
        ))}
      </svg>

      {a ? (
        <div
          className="pointer-events-none absolute top-0 z-10 w-max min-w-36 -translate-x-1/2 -translate-y-full rounded-md border border-rule bg-bg-raised px-3 py-2 text-xs shadow-[0_8px_24px_-12px_hsl(var(--shadow-color)/0.45)]"
          style={{ left: tipLeft }}
        >
          <p className="text-fg-muted">{f.dayLong(a.day)}</p>
          <p className="tnum mt-0.5 text-base font-semibold text-fg">{f.usd(a.d?.cost_usd ?? 0)}</p>
          <p className="tnum mt-1 text-fg-muted">{fill(u.tipTokens, { n: f.compact(a.d ? tokensOf(a.d) : 0) })}</p>
          <p className="tnum text-fg-muted">
            {fill(u.tipSessions, { n: f.num(a.d?.sessions ?? 0) })} · {f.duration(a.d?.active_secs ?? 0)}
          </p>
        </div>
      ) : null}
      <p className="sr-only" aria-live="polite">
        {a ? `${f.dayLong(a.day)}: ${f.usd(a.d?.cost_usd ?? 0)}, ${fill(u.tipTokens, { n: f.compact(a.d ? tokensOf(a.d) : 0) })}, ${fill(u.tipSessions, { n: f.num(a.d?.sessions ?? 0) })}` : ""}
      </p>
    </div>
  );
}

function MembersTable({ members }: { members: UsageMember[] }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  const f = useFormat();
  const id = useId();
  const max = Math.max(...members.map((m) => m.cost_usd), 0);
  const num = "px-3 py-3 text-right whitespace-nowrap";
  return (
    <section aria-labelledby={id} className="flex min-w-0 flex-col gap-3">
      <div>
        <h3 id={id} className="font-semibold">
          {u.members}
        </h3>
        <p className="mt-0.5 text-sm text-fg-muted">{u.membersSub}</p>
      </div>
      <div className="overflow-x-auto rounded-lg border border-rule bg-bg-raised [scrollbar-width:thin]">
        <table className="w-full min-w-[46rem] text-sm">
          <thead className="border-b border-rule text-left text-[0.75rem] text-fg-muted">
            <tr>
              <th scope="col" className="sticky left-0 bg-bg-raised px-4 py-2.5 font-medium">{u.member}</th>
              <th scope="col" className="px-3 py-2.5 text-right font-medium">{u.cost}</th>
              <th scope="col" className="px-3 py-2.5 text-right font-medium">{u.tokens}</th>
              <th scope="col" className="px-3 py-2.5 text-right font-medium">{u.sessions}</th>
              <th scope="col" className="px-3 py-2.5 text-right font-medium">{u.activeTime}</th>
              <th scope="col" className="px-3 py-2.5 text-right font-medium">{u.lines}</th>
              <th scope="col" className="px-3 py-2.5 text-right font-medium">{u.commits}</th>
              <th scope="col" className="px-3 py-2.5 text-right font-medium">{u.pullRequests}</th>
              <th scope="col" className="px-4 py-2.5 text-right font-medium">{u.lastActive}</th>
            </tr>
          </thead>
          <tbody className="tnum divide-y divide-rule">
            {members.map((m) => (
              <tr key={m.handle}>
                <th scope="row" className="sticky left-0 bg-bg-raised px-4 py-3 text-left font-normal">
                  <span translate="no" className="block font-mono text-[0.8rem] font-medium whitespace-nowrap">
                    @{m.handle}
                  </span>
                  {m.owner ? (
                    <span translate="no" className="block text-[0.75rem] text-fg-muted">
                      {fill(u.ownedBy, { owner: m.owner })}
                    </span>
                  ) : null}
                </th>
                <td className={num}>
                  <span className="font-medium">{f.usd(m.cost_usd)}</span>
                  <span aria-hidden="true" className="mt-1.5 ml-auto block h-1 w-20 rounded-full bg-bg-sunk">
                    <span className="block h-full rounded-full bg-primary" style={{ width: `${max ? Math.max(2, (m.cost_usd / max) * 100) : 0}%` }} />
                  </span>
                </td>
                <td className={num}>{f.compact(tokensOf(m))}</td>
                <td className={num}>{f.num(m.sessions)}</td>
                <td className={num}>{f.duration(m.active_secs)}</td>
                <td className={num}>
                  +{f.compact(m.lines_added)} <span className="text-fg-muted">−{f.compact(m.lines_removed)}</span>
                </td>
                <td className={num}>{f.num(m.commits)}</td>
                <td className={num}>{f.num(m.pull_requests)}</td>
                <td className="px-4 py-3 text-right whitespace-nowrap text-fg-muted">{f.day(m.last_day)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function Models({ models, total }: { models: UsageModel[]; total: number }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  const id = useId();
  const max = Math.max(...models.map((m) => m.cost_usd), 0);
  return (
    <section aria-labelledby={id} className="flex min-w-0 flex-col gap-3">
      <div>
        <h3 id={id} className="font-semibold">
          {u.models}
        </h3>
        <p className="mt-0.5 text-sm text-fg-muted">{u.modelsSub}</p>
      </div>
      <ul className="flex flex-col divide-y divide-rule rounded-lg border border-rule bg-bg-raised px-4">
        {models.map((m) => (
          <ModelRow key={m.model} m={m} max={max} total={total} />
        ))}
      </ul>
    </section>
  );
}

function ModelRow({ m, max, total }: { m: UsageModel; max: number; total: number }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  const f = useFormat();
  return (
    <li className="py-3.5">
      <div className="flex items-baseline justify-between gap-3">
        <span translate="no" className="min-w-0 truncate font-mono text-[0.8rem] font-medium" title={m.model}>
          {m.model}
        </span>
        <span className="tnum shrink-0 font-semibold">{f.usd(m.cost_usd)}</span>
      </div>
      <div aria-hidden="true" className="mt-2 h-2">
        <div className="h-full rounded-r-[4px] bg-primary" style={{ width: `${max ? Math.max(0.5, (m.cost_usd / max) * 100) : 0}%` }} />
      </div>
      <p className="tnum mt-1.5 text-[0.78rem] text-fg-muted">
        {fill(u.tokensInOut, { in: f.compact(m.input_tokens), out: f.compact(m.output_tokens), cache: f.compact(m.cache_read_tokens + m.cache_write_tokens) })}
        {total > 0 ? ` · ${fill(u.share, { pct: f.pct(m.cost_usd / total) })}` : null}
      </p>
    </li>
  );
}

function Empty({ days, onLonger, setUp }: { days: number; onLonger?: () => void; setUp: boolean }) {
  const { t } = usePrefs();
  const u = t.app.usage;
  return (
    <section className="rounded-lg border border-dashed border-rule-strong px-5 py-8 sm:px-8 sm:py-10">
      <div className="max-w-[40rem]">
        <Activity className="size-5 text-accent" aria-hidden="true" />
        <h3 className="mt-3 text-lg font-semibold">{fill(u.emptyTitle, { n: days })}</h3>
        {setUp ? null : (
          <>
            <p className="mt-2 text-fg-muted">{u.emptyBody}</p>
            <CodeBlock className="mt-4" lines={["davra claude"]} label={u.codeLabel} />
            <p className="mt-4 text-sm text-fg-muted">{u.emptyHint}</p>
            <p className="mt-4 flex items-start gap-2 text-sm">
              <ShieldCheck className="mt-0.5 size-4 shrink-0 text-accent" aria-hidden="true" />
              {u.privacy}
            </p>
          </>
        )}
        {onLonger ? (
          <p className="mt-6 flex flex-wrap items-baseline gap-x-2 text-sm text-fg-muted">
            {u.longer}
            <Button variant="link" className="text-sm" onClick={onLonger}>
              {fill(u.rangeLong, { n: 90 })}
            </Button>
          </p>
        ) : null}
      </div>
    </section>
  );
}
