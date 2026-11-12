import { ArrowRight, Plus, Send } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import { ErrorPanel, RoleBadge, Skeleton } from "@/components/app/States";
import { TelegramConnect } from "@/components/app/TelegramConnect";
import { useResource } from "@/components/app/useResource";
import { LogoMark } from "@/components/site/Logo";
import { Button } from "@/components/ui/button";
import { useFormat } from "@/lib/format";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { web, type ProjectItem } from "@/lib/web";

export const shell = "mx-auto w-full max-w-[76rem] px-4 sm:px-6 lg:px-8";

export default function Overview() {
  const { t } = usePrefs();
  const o = t.app.overview;
  const res = useResource((s) => web.overview(s), []);
  const data = res.data;
  // Keep the Telegram card mounted after linking succeeds, so its confirmation stays on screen.
  const [showTg, setShowTg] = useState(false);
  if (data && !data.telegram.linked && !showTg) setShowTg(true);

  return (
    <div className={cn(shell, "flex flex-col gap-10 py-10 sm:py-14")}>
      {showTg ? (
        <TelegramConnect
          poll={async () => {
            const d = await res.reload({ quiet: true });
            return Boolean(d?.telegram.linked);
          }}
        />
      ) : null}

      <section aria-labelledby="projects-title" className="flex flex-col gap-6">
        <div className="flex flex-wrap items-end justify-between gap-4">
          <div>
            <h1 id="projects-title" className="display text-[clamp(1.9rem,4vw,2.6rem)]">
              {o.title}
            </h1>
            <p className="mt-2 text-fg-muted">{o.sub}</p>
          </div>
          {data && data.projects.length > 0 ? (
            <Button asChild variant="outline">
              <Link to="/app/new">
                <Plus aria-hidden="true" />
                {o.newProject}
              </Link>
            </Button>
          ) : null}
        </div>

        {res.loading && !data ? (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3" aria-busy="true">
            <Skeleton className="h-48" />
            <Skeleton className="h-48" />
            <Skeleton className="hidden h-48 lg:block" />
          </div>
        ) : res.error && !data ? (
          <ErrorPanel error={res.error} onRetry={() => void res.reload()} />
        ) : data && data.projects.length === 0 ? (
          <EmptyState />
        ) : data ? (
          <ul className="grid gap-4 *:min-w-0 sm:grid-cols-2 lg:grid-cols-3">
            {data.projects.map((p) => (
              <li key={`${p.org}/${p.slug}`}>
                <ProjectCard p={p} />
              </li>
            ))}
          </ul>
        ) : null}
      </section>
    </div>
  );
}

function EmptyState() {
  const { t } = usePrefs();
  const o = t.app.overview;
  return (
    <div className="flex flex-col items-start gap-6 rounded-lg border border-dashed border-rule-strong px-6 py-12 sm:flex-row sm:items-center sm:px-10">
      <LogoMark className="size-12 shrink-0 text-primary" />
      <div className="flex-1">
        <h2 className="text-xl font-semibold">{o.emptyTitle}</h2>
        <p className="mt-2 max-w-[38rem] text-fg-muted">{o.emptyBody}</p>
      </div>
      <Button asChild size="lg">
        <Link to="/app/new">
          <Plus aria-hidden="true" />
          {o.create}
        </Link>
      </Button>
    </div>
  );
}

function ProjectCard({ p }: { p: ProjectItem }) {
  const { t } = usePrefs();
  const f = useFormat();
  const o = t.app.overview;
  const stats: [number, string][] = [
    [p.members, o.people],
    [p.agents, o.agents],
    [p.messages, o.messages],
    [p.open_handoffs, o.handoffs],
  ];
  return (
    <Link
      to={`/app/${encodeURIComponent(p.org)}/${encodeURIComponent(p.slug)}`}
      className="group flex h-full flex-col rounded-lg border border-rule bg-bg-raised p-5 transition-[border-color,box-shadow] duration-200 hover:border-rule-strong hover:shadow-[0_12px_32px_-16px_hsl(var(--shadow-color)/0.35)]"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="truncate text-lg font-semibold">{p.name}</h2>
          <p className="mt-0.5 truncate font-mono text-[0.72rem] text-fg-muted" translate="no">
            {p.org}/{p.slug}
          </p>
        </div>
        <RoleBadge role={p.role} className="mt-1 shrink-0" />
      </div>
      <ul className="mt-5 grid grid-cols-2 gap-x-4 gap-y-3">
        {stats.map(([n, label]) => (
          <li key={label} className="flex min-w-0 items-baseline gap-1.5">
            <span className="tnum text-xl font-semibold">{f.num(n)}</span>
            <span className="truncate text-sm text-fg-muted">{label}</span>
          </li>
        ))}
      </ul>
      <div className="mt-auto flex items-center gap-3 border-t border-rule pt-4 text-[0.8rem] text-fg-muted">
        <span title={p.telegram ? o.telegramOn : o.telegramOff} className={cn("inline-flex", p.telegram ? "text-accent" : "opacity-40")}>
          <Send className="size-3.5" aria-hidden="true" />
          <span className="sr-only">{p.telegram ? o.telegramOn : o.telegramOff}</span>
        </span>
        <span className="min-w-0 flex-1 truncate">
          {p.last_activity ? (
            <time dateTime={p.last_activity} title={f.full(p.last_activity)}>
              {fill(o.active, { time: f.relative(p.last_activity) })}
            </time>
          ) : (
            o.noActivity
          )}
        </span>
        <span className="hidden font-mono text-[0.7rem] sm:inline" translate="no">
          @{p.handle}
        </span>
        <ArrowRight className="size-4 shrink-0 transition-transform duration-200 group-hover:translate-x-0.5" aria-hidden="true" />
      </div>
    </Link>
  );
}
