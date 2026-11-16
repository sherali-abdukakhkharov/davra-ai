import { Check, Copy, FileText, Gavel, Search } from "lucide-react";
import { Fragment, useEffect, useId, useMemo, useState, type MouseEvent, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useFormat } from "@/lib/format";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { web, type Decision } from "@/lib/web";
import { Markdown } from "./Markdown";
import { ErrorPanel, Skeleton } from "./States";
import { useResource } from "./useResource";

const PAGE = 30;

/** Replaces {key} placeholders with nodes, for copy that carries code or links. */
function rich(template: string, parts: Record<string, ReactNode>) {
  return template.split(/(\{\w+\})/g).map((s, i) => {
    const k = s.match(/^\{(\w+)\}$/)?.[1];
    return k && k in parts ? <Fragment key={i}>{parts[k]}</Fragment> : s;
  });
}

const matches = (x: Decision, q: string) =>
  [`#${x.id} d${x.id}`, x.ticket ?? "", x.title, x.decision, x.why, x.instead ?? "", x.decided_by, x.author ?? ""].join("\n").toLowerCase().includes(q);

const newestFirst = (a: Decision, b: Decision) => Date.parse(b.created_at) - Date.parse(a.created_at) || b.id - a.id;

const code = "rounded-[4px] bg-bg-sunk px-1 py-px font-mono text-[0.85em] text-fg";

export function DecisionsTab({ org, slug }: { org: string; slug: string }) {
  const { t } = usePrefs();
  const d = t.app.decisions;
  const res = useResource((s) => web.decisions(org, slug, s), [org, slug]);

  return (
    <section aria-labelledby="decisions-title" className="flex flex-col gap-4">
      <h2 id="decisions-title" className="sr-only">
        {d.title}
      </h2>
      {res.error && !res.data ? (
        <ErrorPanel error={res.error} onRetry={() => void res.reload()} />
      ) : !res.data ? (
        <div className="flex flex-col gap-2">
          <Skeleton className="h-10 max-w-[24rem]" />
          <Skeleton className="h-48" />
          <Skeleton className="h-48" />
        </div>
      ) : res.data.decisions.length === 0 ? (
        <div className="rounded-lg border border-dashed border-rule-strong px-6 py-10 text-center">
          <Gavel className="mx-auto size-5 text-fg-muted" aria-hidden="true" />
          <p className="mt-3 font-medium">{d.emptyTitle}</p>
          <p className="mx-auto mt-1.5 max-w-[34rem] text-sm text-pretty text-fg-muted">
            {rich(d.empty, {
              tool: <code className={code}>decision_record</code>,
              brief: <code className={code}>team_brief</code>,
              dir: <code className={code}>docs/memory/decisions/</code>,
            })}
          </p>
        </div>
      ) : (
        <DecisionLog decisions={res.data.decisions} />
      )}
    </section>
  );
}

function DecisionLog({ decisions }: { decisions: Decision[] }) {
  const { t } = usePrefs();
  const d = t.app.decisions;
  const [query, setQuery] = useState("");
  const [showReplaced, setShowReplaced] = useState(false);
  const [limit, setLimit] = useState(PAGE);
  const [jump, setJump] = useState<number | null>(null);
  const [flash, setFlash] = useState<number | null>(null);
  const q = query.trim().toLowerCase();

  const all = useMemo(() => [...decisions].sort(newestFirst), [decisions]);
  const byId = useMemo(() => new Map(all.map((x) => [x.id, x] as const)), [all]);
  const accepted = useMemo(() => all.filter((x) => x.status !== "superseded"), [all]);
  const replaced = all.length - accepted.length;
  const pool = showReplaced ? all : accepted;
  const shown = useMemo(() => (q ? pool.filter((x) => matches(x, q)) : pool), [pool, q]);

  /** Bring a decision into view: show replaced ones, drop a filter that hides it, page down to it. */
  const goTo = (id: number) => {
    const target = byId.get(id);
    if (!target) return;
    if (target.status === "superseded") setShowReplaced(true);
    if (q && !matches(target, q)) setQuery("");
    setJump(id);
  };

  useEffect(() => {
    if (jump === null) return;
    const el = document.getElementById(`d-${jump}`);
    if (!el) {
      const at = shown.findIndex((x) => x.id === jump);
      if (at >= limit) setLimit(at + 1);
      else setJump(null);
      return;
    }
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    el.scrollIntoView({ block: "center", behavior: reduce ? "auto" : "smooth" });
    el.focus({ preventScroll: true });
    setFlash(jump);
    setJump(null);
  }, [jump, shown, limit]);

  useEffect(() => {
    if (flash === null) return;
    const timer = window.setTimeout(() => setFlash(null), 2400);
    return () => window.clearTimeout(timer);
  }, [flash]);

  return (
    <>
      <div className="flex flex-wrap items-center gap-x-5 gap-y-3">
        <label htmlFor="decision-filter" className="sr-only">
          {d.filter}
        </label>
        <div className="relative w-full max-w-[24rem]">
          <Search className="pointer-events-none absolute top-1/2 left-3.5 size-4 -translate-y-1/2 text-fg-muted" aria-hidden="true" />
          <Input
            id="decision-filter"
            type="search"
            autoComplete="off"
            spellCheck={false}
            placeholder={d.filterPlaceholder}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setLimit(PAGE);
            }}
            className="h-10 pl-10"
          />
        </div>
        {replaced > 0 ? (
          <label className="inline-flex min-h-10 cursor-pointer items-center gap-2.5 text-sm select-none">
            <input
              type="checkbox"
              checked={showReplaced}
              onChange={(e) => {
                setShowReplaced(e.target.checked);
                setLimit(PAGE);
              }}
              className="size-4 cursor-pointer accent-primary"
            />
            {d.showReplaced}
            <span className="tnum text-fg-muted">({replaced})</span>
          </label>
        ) : null}
        <p className="tnum text-sm text-fg-muted sm:ml-auto" aria-live="polite">
          {showReplaced ? fill(d.countAll, { n: shown.length, total: all.length }) : fill(d.count, { n: shown.length, total: accepted.length })}
        </p>
      </div>
      {shown.length === 0 ? (
        <p className="text-fg-muted">{q ? fill(t.app.search.none, { q: query.trim() }) : d.noneAccepted}</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {shown.slice(0, limit).map((x) => (
            <DecisionItem key={x.id} x={x} known={byId} flash={flash === x.id} onJump={goTo} />
          ))}
        </ul>
      )}
      {shown.length > limit ? (
        <div>
          <Button variant="outline" onClick={() => setLimit((n) => n + PAGE)}>
            {fill(d.more, { n: Math.min(PAGE, shown.length - limit) })}
          </Button>
        </div>
      ) : null}
    </>
  );
}

/** The local calendar day, e.g. "Nov 17", with the year when it is not this year. */
function useDay() {
  const f = useFormat();
  return (iso: string) => {
    const dt = new Date(iso);
    const pad = (n: number) => String(n).padStart(2, "0");
    const day = f.day(`${dt.getFullYear()}-${pad(dt.getMonth() + 1)}-${pad(dt.getDate())}`);
    return dt.getFullYear() === new Date().getFullYear() ? day : `${day}, ${dt.getFullYear()}`;
  };
}

function Handle({ name }: { name: string }) {
  return (
    <span translate="no" className="rounded-[4px] bg-bg-sunk px-1.5 py-px font-mono text-[0.75rem] text-fg">
      @{name}
    </span>
  );
}

function DecisionItem({
  x,
  known,
  flash,
  onJump,
}: {
  x: Decision;
  known: Map<number, Decision>;
  flash: boolean;
  onJump: (id: number) => void;
}) {
  const { t } = usePrefs();
  const d = t.app.decisions;
  const f = useFormat();
  const day = useDay();
  const old = x.status === "superseded";
  const titleId = `d-${x.id}-title`;

  const ref = (id: number) =>
    known.has(id) ? (
      <a
        href={`#d-${id}`}
        onClick={(e: MouseEvent) => {
          e.preventDefault();
          onJump(id);
        }}
        className="rounded-sm font-mono text-accent underline decoration-current/40 underline-offset-2 hover:decoration-current"
      >
        D{id}
      </a>
    ) : (
      <span className="font-mono">D{id}</span>
    );

  return (
    <li>
      <article
        id={`d-${x.id}`}
        tabIndex={-1}
        aria-labelledby={titleId}
        className={cn(
          "scroll-mt-24 rounded-lg border px-4 py-4 transition-colors duration-500 sm:px-5",
          old ? "border-dashed border-rule-strong text-fg-muted" : "border-rule bg-bg-raised",
          flash && "border-saffron bg-saffron/15",
        )}
      >
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-[0.8rem]">
          <span translate="no" className="tnum font-mono text-fg-muted">
            D{x.id}
          </span>
          {x.ticket ? (
            <span translate="no" className="rounded-[4px] border border-rule-strong px-1.5 py-px font-mono text-[0.72rem] font-medium text-fg">
              {x.ticket}
            </span>
          ) : null}
          {old ? <span className="rounded-full bg-bg-sunk px-2 py-px text-[0.72rem] text-fg-muted">{d.replaced}</span> : null}
          <time dateTime={x.created_at} title={f.full(x.created_at)} className="text-fg-muted sm:ml-auto">
            {fill(d.recorded, { time: day(x.created_at) })}
          </time>
        </div>

        <h3 id={titleId} className={cn("mt-2 text-[1.05rem] leading-snug font-semibold text-balance [overflow-wrap:anywhere]", old ? "text-fg-muted" : "text-fg")}>
          {x.title}
        </h3>

        {old && x.superseded_by !== null ? (
          <p className="mt-1 text-sm">{rich(d.replacedBy, { id: ref(x.superseded_by) })}</p>
        ) : null}
        {x.supersedes !== null ? <p className="mt-1 text-sm text-fg-muted">{rich(d.replaces, { id: ref(x.supersedes) })}</p> : null}

        <div className="mt-3">
          <Markdown source={x.decision} />
        </div>

        <dl className="mt-4 grid grid-cols-1 gap-x-4 gap-y-1 text-[0.92rem] leading-relaxed sm:grid-cols-[9rem_minmax(0,1fr)] sm:gap-y-2.5">
          <dt className="text-sm font-medium text-fg-muted">{d.why}</dt>
          <dd className="mb-2 whitespace-pre-line [overflow-wrap:anywhere] sm:mb-0">{x.why}</dd>
          {x.instead ? (
            <>
              <dt className="text-sm font-medium text-fg-muted">{d.instead}</dt>
              <dd className="whitespace-pre-line [overflow-wrap:anywhere]">{x.instead}</dd>
            </>
          ) : null}
        </dl>

        <dl className="mt-4 flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-rule pt-3 text-[0.8rem] text-fg-muted">
          <div className="flex items-center gap-1.5">
            <dt>{d.decidedBy}</dt>
            <dd>
              <Handle name={x.decided_by} />
            </dd>
          </div>
          {x.author ? (
            <div className="flex items-center gap-1.5">
              <dt>{d.recordedBy}</dt>
              <dd>
                <Handle name={x.author} />
              </dd>
            </div>
          ) : null}
          {x.source_message !== null ? (
            <div className="flex items-center gap-1.5">
              <dt>{d.source}</dt>
              <dd translate="no" className="font-mono text-fg">
                #{x.source_message}
              </dd>
            </div>
          ) : null}
        </dl>

        {x.file_path ? <FilePath path={x.file_path} /> : null}
      </article>
    </li>
  );
}

function FilePath({ path }: { path: string }) {
  const { t } = usePrefs();
  const d = t.app.decisions;
  const [copied, setCopied] = useState(false);
  const id = useId();
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(path);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      // Clipboard refused: select the path so it can be copied by hand.
      const el = document.getElementById(id);
      if (el) window.getSelection()?.selectAllChildren(el);
    }
  };
  return (
    <div className="mt-3 flex items-center gap-2 text-fg-muted">
      <FileText className="size-3.5 shrink-0" aria-hidden="true" />
      <span className="sr-only">{d.file}: </span>
      <code id={id} translate="no" className="min-w-0 flex-1 font-mono text-[0.72rem] [overflow-wrap:anywhere] select-all">
        {path}
      </code>
      <Button variant="ghost" size="sm" onClick={() => void copy()} className="h-8 shrink-0 px-2.5 text-fg-muted hover:text-fg" aria-label={d.copyPath}>
        {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
        <span className="hidden sm:inline" aria-live="polite">
          {copied ? d.copied : d.copy}
        </span>
      </Button>
    </div>
  );
}
