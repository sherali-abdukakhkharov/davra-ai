import { CircleAlert, FolderGit2, MonitorSmartphone, SquareTerminal, UserRound, type LucideIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { LogoMark } from "@/components/site/Logo";
import { LangToggle, ThemeToggle } from "@/components/site/Prefs";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { compareVersions, type Status } from "../bridge";
import { Dot, MockBadge, type Tone } from "../components/bits";
import { ReviewSheet, type ReviewRequest } from "../components/ReviewSheet";
import { AccountPanel } from "../panels/Account";
import { CliPanel } from "../panels/Cli";
import { ClientsPanel } from "../panels/Clients";
import { ReposPanel } from "../panels/Repos";
import { hubHost, ReviewContext, useDesktop } from "../state";

const SECTIONS = ["account", "cli", "clients", "repos"] as const;
type Section = (typeof SECTIONS)[number];

const ICONS: Record<Section, LucideIcon> = {
  account: UserRound,
  cli: SquareTerminal,
  clients: MonitorSmartphone,
  repos: FolderGit2,
};

type NavItem = { id: Section; label: string; tone: Tone; state: string; badge?: string };

function cliTone(cli: Status["cli"]): { tone: Tone; key: "ok" | "update" | "missing" | "noPath" } {
  if (!cli.path) return { tone: "warn", key: "missing" };
  if (cli.version && compareVersions(cli.version, cli.bundled_version) < 0) return { tone: "warn", key: "update" };
  if (!cli.on_path) return { tone: "warn", key: "noPath" };
  return { tone: "ok", key: "ok" };
}

/** Lands on the first thing that still needs doing. */
function firstSection(s: Status): Section {
  if (cliTone(s.cli).tone !== "ok") return "cli";
  if (!s.clients.some((c) => c.connected)) return "clients";
  if (!s.repos.length) return "repos";
  return "clients";
}

function useNav(s: Status): NavItem[] {
  const { t } = usePrefs();
  const d = t.desktop;
  const cli = cliTone(s.cli);
  const connected = s.clients.filter((c) => c.connected).length;
  const partial = s.clients.some((c) => c.partial);
  const missing = s.repos.some((r) => !r.has_file);
  return [
    { id: "account", label: d.nav.account, tone: s.me_error ? "danger" : "ok", state: s.me_error ? d.navState.error : d.navState.ok },
    { id: "cli", label: d.nav.cli, tone: cli.tone, state: d.navState[cli.key] },
    {
      id: "clients",
      label: d.nav.clients,
      tone: partial ? "warn" : connected ? "ok" : "off",
      state: fill(d.navState.connected, { n: connected, total: s.clients.length }) + (partial ? `, ${d.navState.attention}` : ""),
      badge: `${connected}/${s.clients.length}`,
    },
    {
      id: "repos",
      label: d.nav.repos,
      tone: missing ? "warn" : s.repos.length ? "ok" : "off",
      state: fill(d.navState.repos, { n: s.repos.length }) + (missing ? `, ${d.navState.attention}` : ""),
      badge: String(s.repos.length),
    },
  ];
}

export function MainScreen() {
  const { t } = usePrefs();
  const d = t.desktop;
  const { status } = useDesktop();
  const [section, setSection] = useState<Section>(() => firstSection(status));
  const [review, setReview] = useState<ReviewRequest | null>(null);
  const nav = useNav(status);

  // Ctrl+1…4 jumps between sections.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (review || !(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
      const i = Number(e.key) - 1;
      const id = SECTIONS[i];
      if (id) {
        e.preventDefault();
        setSection(id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [review]);

  const me = status.me!;

  return (
    <ReviewContext.Provider value={setReview}>
      <div className="flex h-dvh">
        <aside className="hidden w-[13.5rem] shrink-0 flex-col border-r border-rule bg-bg-sunk/60 md:flex">
          <div className="flex h-14 items-center gap-2 px-4">
            <LogoMark className="size-6" />
            <span translate="no" className="text-[1.05rem] font-[720] tracking-[-0.03em] [font-variation-settings:'SHRP'_100]">
              Davra
            </span>
            <span className="rounded-[4px] bg-bg-sunk px-1.5 py-px font-mono text-[0.62rem] text-fg-muted">{d.tag}</span>
          </div>
          <NavList items={nav} current={section} onPick={setSection} vertical />
          <div className="mt-auto flex flex-col gap-3 border-t border-rule px-4 py-4">
            <button
              type="button"
              onClick={() => setSection("account")}
              className="-mx-1.5 flex min-w-0 cursor-pointer flex-col rounded-md px-1.5 py-1 text-left hover:bg-bg-sunk"
            >
              <span translate="no" className="truncate font-mono text-[0.78rem] font-medium">
                @{me.handle}
              </span>
              <span className="truncate font-mono text-[0.68rem] text-fg-muted">{status.hub ? hubHost(status.hub) : ""}</span>
            </button>
            <div className="flex items-center gap-2">
              <LangToggle />
              <ThemeToggle />
            </div>
            <div className="flex flex-wrap items-center gap-2 text-[0.7rem] text-fg-muted">
              <span className="tnum">{fill(d.version, { v: status.version })}</span>
              <MockBadge />
            </div>
          </div>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          <header className="border-b border-rule bg-bg-sunk/60 md:hidden">
            <div className="flex h-14 items-center gap-2 px-4">
              <LogoMark className="size-6" />
              <span translate="no" className="text-[1.05rem] font-[720] tracking-[-0.03em]">
                Davra
              </span>
              <MockBadge className="ml-1" />
              <span className="mr-auto" />
              <LangToggle />
              <ThemeToggle />
            </div>
            <NavList items={nav} current={section} onPick={setSection} />
          </header>

          {status.me_error ? (
            <div role="alert" className="flex items-start gap-2.5 border-b border-danger/40 bg-danger-soft px-5 py-2.5 text-sm md:px-8">
              <CircleAlert className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden="true" />
              <p className="min-w-0">
                <span className="font-medium">{d.account.staleTitle}.</span> <span className="break-words">{status.me_error}</span>
              </p>
            </div>
          ) : null}

          <main id="main" className="flex-1 overflow-y-auto [scrollbar-gutter:stable]" aria-labelledby={`${section}-title`}>
            <div className="mx-auto w-full max-w-[50rem] px-5 py-6 md:px-8 md:py-7">
              {section === "account" ? <AccountPanel /> : section === "cli" ? <CliPanel /> : section === "clients" ? <ClientsPanel /> : <ReposPanel />}
            </div>
          </main>
        </div>
      </div>
      {review ? <ReviewSheet key={review.title} request={review} onClose={() => {
            setReview(null);
            // If the button that opened the sheet is gone, land on the panel heading instead of nowhere.
            requestAnimationFrame(() => {
              if (!document.activeElement || document.activeElement === document.body) document.getElementById(`${section}-title`)?.focus();
            });
          }} /> : null}
    </ReviewContext.Provider>
  );
}

function NavList({ items, current, onPick, vertical = false }: { items: NavItem[]; current: Section; onPick: (s: Section) => void; vertical?: boolean }) {
  const { t } = usePrefs();
  return (
    <nav aria-label={t.desktop.nav.label} className={vertical ? "px-2 py-2" : "overflow-x-auto px-2 [scrollbar-width:none]"}>
      <ul className={vertical ? "flex flex-col gap-0.5" : "flex min-w-max gap-1"}>
        {items.map((it, i) => {
          const Icon = ICONS[it.id];
          const active = it.id === current;
          return (
            <li key={it.id}>
              <button
                type="button"
                onClick={() => onPick(it.id)}
                aria-current={active ? "page" : undefined}
                aria-keyshortcuts={`Control+${i + 1}`}
                className={cn(
                  "relative flex w-full cursor-pointer items-center gap-2.5 rounded-md text-[0.9rem] transition-colors duration-150",
                  vertical ? "h-9 px-2.5" : "h-11 px-3",
                  active ? "font-medium text-fg" : "text-fg-muted hover:text-fg",
                  vertical && (active ? "bg-bg-raised shadow-[0_0_0_1px_var(--rule)]" : "hover:bg-bg-sunk"),
                )}
              >
                <Icon className={cn("size-4 shrink-0", active ? "text-primary" : "")} aria-hidden="true" />
                <span className="truncate">{it.label}</span>
                <span className="sr-only">, {it.state}</span>
                <span aria-hidden="true" className={cn("flex items-center gap-1.5", vertical ? "ml-auto" : "ml-1")}>
                  {it.badge ? <span className="tnum font-mono text-[0.68rem] font-normal text-fg-muted">{it.badge}</span> : null}
                  <Dot tone={it.tone} />
                </span>
                {!vertical ? (
                  <span aria-hidden="true" className={cn("absolute inset-x-2 -bottom-px h-[3px] rounded-full", active ? "bg-primary" : "bg-transparent")} />
                ) : null}
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
