import { ArrowLeft, Send } from "lucide-react";
import { Link, useParams, useSearchParams } from "react-router";
import { AgentsTab } from "@/components/app/AgentsTab";
import { HandoffsTab } from "@/components/app/HandoffsTab";
import { MembersTab } from "@/components/app/MembersTab";
import { RoomTab } from "@/components/app/RoomTab";
import { SearchTab } from "@/components/app/SearchTab";
import { UsageTab } from "@/components/app/UsageTab";
import { ErrorPanel, RoleBadge, Skeleton } from "@/components/app/States";
import { useResource } from "@/components/app/useResource";
import { useFormat } from "@/lib/format";
import { usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { web, type Member, type ProjectDetail } from "@/lib/web";
import { shell } from "./Overview";

const TABS = ["room", "search", "handoffs", "members", "agents", "usage"] as const;
type Tab = (typeof TABS)[number];

export default function Project() {
  const { t } = usePrefs();
  const p = t.app.project;
  const { org = "", slug = "" } = useParams();
  const [params] = useSearchParams();
  const raw = params.get("tab");
  const tab: Tab = (TABS as readonly string[]).includes(raw ?? "") ? (raw as Tab) : "room";
  const res = useResource((s) => web.project(org, slug, s), [org, slug]);
  const d = res.data;

  const onMember = (m: Member) =>
    res.setData((prev) => {
      if (!prev) return prev;
      const exists = prev.members.some((x) => x.handle === m.handle);
      const members = exists ? prev.members.map((x) => (x.handle === m.handle ? { ...m, tokens: m.tokens.length ? m.tokens : x.tokens } : x)) : [...prev.members, m];
      return { ...prev, members, stats: { ...prev.stats, agents: members.filter((x) => x.kind === "agent").length } };
    });

  return (
    <div className="flex flex-1 flex-col">
      <div className="border-b border-rule bg-bg-sunk/50">
        <div className={cn(shell, "pt-6 sm:pt-8")}>
          <Link to="/app" className="inline-flex items-center gap-2 rounded-sm text-sm text-fg-muted hover:text-fg">
            <ArrowLeft className="size-4" aria-hidden="true" />
            {p.back}
          </Link>
          {d ? <Header d={d} /> : res.error ? null : <Skeleton className="mt-5 h-20 max-w-[36rem]" />}
          <nav aria-label={p.tabsLabel} className="-mx-4 mt-6 overflow-x-auto px-4 [scrollbar-width:none] sm:mx-0 sm:px-0">
            <ul className="flex min-w-max gap-1">
              {TABS.map((id) => {
                const active = id === tab;
                const count = d ? { handoffs: d.handoffs.length, members: d.members.length, agents: d.stats.agents }[id as "handoffs"] : undefined;
                return (
                  <li key={id}>
                    <Link
                      to={id === "room" ? "?" : `?tab=${id}`}
                      replace
                      aria-current={active ? "page" : undefined}
                      ref={active ? (el) => el?.scrollIntoView({ block: "nearest", inline: "nearest" }) : undefined}
                      className={cn(
                        "relative inline-flex h-11 items-center gap-2 rounded-t-md px-3.5 text-[0.92rem] transition-colors",
                        active ? "font-medium text-fg" : "text-fg-muted hover:text-fg",
                      )}
                    >
                      {p.tabs[id]}
                      {count !== undefined && count > 0 ? (
                        <span className="tnum rounded-full bg-bg-sunk px-1.5 text-[0.7rem] text-fg-muted">{count}</span>
                      ) : null}
                      <span
                        aria-hidden="true"
                        className={cn("absolute inset-x-2 -bottom-px h-[3px] rounded-full transition-colors", active ? "bg-primary" : "bg-transparent")}
                      />
                    </Link>
                  </li>
                );
              })}
            </ul>
          </nav>
        </div>
      </div>

      <div className={cn(shell, "flex-1 py-8 sm:py-10")}>
        {res.error && !d ? (
          <ErrorPanel error={res.error} onRetry={() => void res.reload()} />
        ) : !d ? (
          <div className="space-y-2">
            <Skeleton className="h-72" />
          </div>
        ) : tab === "room" ? (
          <RoomTab key={`${org}/${slug}`} project={d.project} initial={d.messages} members={d.members} onPosted={() => void res.reload({ quiet: true })} />
        ) : tab === "search" ? (
          <SearchTab org={org} slug={slug} />
        ) : tab === "usage" ? (
          <UsageTab org={org} slug={slug} />
        ) : tab === "handoffs" ? (
          <HandoffsTab handoffs={d.handoffs} />
        ) : tab === "members" ? (
          <MembersTab
            project={d.project}
            members={d.members}
            onMembers={(members) => res.setData((prev) => (prev ? { ...prev, members: members.map((x) => ({ ...x, tokens: prev.members.find((y) => y.handle === x.handle)?.tokens ?? x.tokens })) } : prev))}
            onChanged={() => void res.reload({ quiet: true })}
          />
        ) : (
          <AgentsTab project={d.project} members={d.members} onMember={onMember} onChanged={() => void res.reload({ quiet: true })} />
        )}
      </div>
    </div>
  );
}

function Header({ d }: { d: ProjectDetail }) {
  const { t } = usePrefs();
  const p = t.app.project;
  const f = useFormat();
  // The two weekly figures drop out on phones to keep the header short.
  const stats: [number, string, boolean][] = [
    [d.stats.messages, p.messages, true],
    [d.stats.messages_7d, p.week, false],
    [d.stats.open_handoffs, p.open, true],
    [d.stats.done_handoffs_7d, p.done, false],
    [d.stats.agents, p.agents, true],
  ];
  return (
    <div className="mt-5 flex flex-col gap-5 lg:flex-row lg:items-end lg:justify-between">
      <div className="min-w-0">
        <h1 className="display text-[clamp(2rem,4.5vw,3rem)]">{d.project.name}</h1>
        <p className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-sm text-fg-muted">
          <span translate="no" className="font-mono text-[0.75rem]">
            {d.project.org}/{d.project.slug}
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span translate="no" className="font-mono text-[0.75rem] text-fg">
              @{d.project.handle}
            </span>
            <RoleBadge role={d.project.role} />
          </span>
          <span className={cn("inline-flex items-center gap-1.5", d.project.telegram ? "text-accent" : "")}>
            <Send className="size-3.5" aria-hidden="true" />
            {d.project.telegram ? p.telegramOn : p.telegramOff}
          </span>
        </p>
      </div>
      <ul className="flex flex-wrap gap-x-6 gap-y-2">
        {stats.map(([n, label, always]) => (
          <li key={label} className={cn("items-baseline gap-1.5", always ? "flex" : "hidden sm:flex")}>
            <span className="tnum text-lg font-semibold">{f.num(n)}</span>
            <span className="text-sm text-fg-muted">{label}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
