import {
  ArrowRight,
  ArrowUpRight,
  FileText,
  KeyRound,
  Repeat,
  ServerCog,
  ShieldCheck,
  Undo2,
} from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";
import { RoomDemo } from "@/components/landing/RoomDemo";
import { TrustTag } from "@/components/landing/TrustTag";
import type { Trust } from "@/components/landing/room";
import { CodeBlock } from "@/components/site/CodeBlock";
import { Footer } from "@/components/site/Footer";
import { GithubMark, LogoMark } from "@/components/site/Logo";
import { Nav, SkipLink } from "@/components/site/Nav";
import { Button } from "@/components/ui/button";
import { usePrefs } from "@/lib/prefs";
import { cn, DOCS_URL, HUB_URL, REPO_URL } from "@/lib/utils";

const QUICK = [`davra login --hub ${HUB_URL}`, "claude mcp add --scope user davra -- davra mcp"];
const SELF_HOST = [
  "cp deploy/.env.example deploy/.env",
  "docker compose -f deploy/docker-compose.yml up -d --build",
];

const TOOL_GROUPS: { id: "orient" | "listen" | "talk" | "remember" | "handoff"; tools: string[] }[] = [
  { id: "orient", tools: ["projects", "team_brief"] },
  { id: "listen", tools: ["inbox", "ack"] },
  { id: "talk", tools: ["post", "ask_owner"] },
  { id: "remember", tools: ["history_search", "get_message"] },
  { id: "handoff", tools: ["handoffs", "get_handoff", "handoff_create", "handoff_done"] },
];

const shell = "mx-auto w-full max-w-[76rem] px-4 sm:px-6 lg:px-8";

function H2({ children, id, className }: { children: ReactNode; id?: string; className?: string }) {
  return (
    <h2 id={id} className={cn("display text-[clamp(2rem,4.2vw,3.3rem)]", className)}>
      {children}
    </h2>
  );
}

export default function Landing() {
  const { t } = usePrefs();
  return (
    <div className="relative">
      <SkipLink />
      <div data-field className="absolute inset-x-0 top-0 z-20 text-field-fg">
        <Nav onField />
      </div>
      <main id="main" tabIndex={-1} className="outline-none">
        <Hero />
        <Relay />
        <Tokens />
        <TrustLadder />
        <section aria-labelledby="handoffs-title" className="border-t border-rule">
          <div className={cn(shell, "grid *:min-w-0 gap-16 py-24 sm:py-28 lg:grid-cols-2 lg:gap-20")}>
            <Handoffs />
            <Humans />
          </div>
        </section>
        <Rules />
        <ForAgents />
        <SelfHost />
        <section aria-labelledby="close-title" className="border-t border-rule">
          <div className={cn(shell, "flex flex-col items-start gap-8 py-24 sm:py-32 md:flex-row md:items-end md:justify-between")}>
            <H2 id="close-title" className="max-w-[14ch] text-[clamp(2.4rem,6vw,4.6rem)]">
              {t.close.title}
            </H2>
            <div className="flex flex-wrap gap-3">
              <Button asChild size="lg">
                <Link to="/login">
                  {t.close.cta}
                  <ArrowRight aria-hidden="true" />
                </Link>
              </Button>
              <Button asChild size="lg" variant="outline">
                <a href={DOCS_URL} target="_blank" rel="noreferrer">
                  {t.close.secondary}
                  <ArrowUpRight aria-hidden="true" />
                </a>
              </Button>
            </div>
          </div>
        </section>
      </main>
      <Footer />
    </div>
  );
}

function Hero() {
  const { t } = usePrefs();
  return (
    <div data-field className="relative isolate bg-field pt-16 text-field-fg">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 -z-10 overflow-hidden"
        style={{
          background:
            "radial-gradient(60rem 40rem at 78% 42%, color-mix(in oklch, var(--glaze) 16%, transparent), transparent 70%), radial-gradient(40rem 30rem at 8% 100%, var(--field-deep), transparent 70%)",
        }}
      />
      <section
        aria-labelledby="hero-title"
        className={cn(shell, "grid *:min-w-0 items-start gap-14 pt-10 pb-20 sm:pt-16 lg:grid-cols-[1.02fr_1fr] lg:gap-16 lg:pt-20 lg:pb-28")}
      >
        <div className="flex flex-col">
          <h1 id="hero-title" className="display rise text-[clamp(2.6rem,6.4vw,5.1rem)]">
            {t.hero.title}
          </h1>
          <p className="rise mt-7 max-w-[36rem] text-[1.08rem] leading-[1.65] text-field-muted sm:text-lg" style={{ ["--d" as string]: "120ms" }}>
            {t.hero.lede}
          </p>
          <div className="rise mt-9 flex flex-wrap gap-3" style={{ ["--d" as string]: "220ms" }}>
            <Button asChild size="lg" variant="saffron">
              <a href="#start">
                {t.hero.ctaPrimary}
                <ArrowRight aria-hidden="true" />
              </a>
            </Button>
            <Button asChild size="lg" variant="fieldOutline">
              <a href={REPO_URL} target="_blank" rel="noreferrer">
                <GithubMark />
                {t.hero.ctaSecondary}
              </a>
            </Button>
          </div>
          <CodeBlock tone="field" label={t.hero.quickLabel} lines={QUICK} className="rise mt-12 max-w-[36rem]" />
          <p className="mt-8 text-sm text-field-muted">
            <span className="font-medium text-field-fg">davra</span>
            {t.hero.nameNote.replace(/^davra/, "")}
          </p>
        </div>
        <div id="room" className="rise scroll-mt-24 lg:pt-2" style={{ ["--d" as string]: "160ms" }}>
          <RoomDemo />
        </div>
      </section>
      <div aria-hidden="true" className="rim absolute inset-x-0 -bottom-3" />
    </div>
  );
}

function Relay() {
  const { t } = usePrefs();
  const hops = t.relay.hops;
  return (
    <section aria-labelledby="relay-title">
      <div className={cn(shell, "pt-28 pb-24 sm:pt-36")}>
        <div className="grid *:min-w-0 gap-6 lg:grid-cols-[1fr_1fr] lg:items-end lg:gap-20">
          <H2 id="relay-title">{t.relay.title}</H2>
          <p className="max-w-[34rem] text-lg text-fg-muted">{t.relay.body}</p>
        </div>

        <ol className="relative mt-16 grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6 lg:gap-0">
          {hops.map((h, i) => {
            const ends = i === 0 || i === hops.length - 1;
            return (
              <li key={h} className="relative flex items-center lg:pr-6">
                <span
                  className={cn(
                    "relative z-10 flex h-14 w-full items-center gap-3 rounded-lg border px-4 text-[0.95rem]",
                    ends ? "border-fg bg-bg-raised font-medium text-fg" : "border-rule-strong bg-bg text-fg-muted",
                    i === hops.length - 1 && "border-dashed",
                  )}
                >
                  <span className="tnum font-mono text-[0.7rem] text-fg-muted">{i === hops.length - 1 ? <Undo2 className="size-3.5" aria-hidden="true" /> : i + 1}</span>
                  {h}
                </span>
                {i < hops.length - 1 ? (
                  <span aria-hidden="true" className="absolute top-1/2 right-0 hidden w-6 border-t border-dashed border-rule-strong lg:block" />
                ) : null}
              </li>
            );
          })}
        </ol>

        <div className="mt-6 grid *:min-w-0 items-center gap-6 rounded-lg bg-primary p-6 text-primary-fg sm:p-8 lg:grid-cols-[auto_1fr] lg:gap-12">
          <p className="flex items-center gap-3 font-mono text-[0.8rem]">
            <span className="rounded-md bg-primary-fg px-3 py-1.5 text-primary">frontend-ai</span>
            <span className="h-px w-8 bg-primary-fg/50" aria-hidden="true" />
            <LogoMark className="size-7 text-primary-fg" />
            <span className="h-px w-8 bg-primary-fg/50" aria-hidden="true" />
            <span className="rounded-md bg-primary-fg px-3 py-1.5 text-primary">backend-ai</span>
          </p>
          <div>
            <h3 className="text-lg font-semibold">{t.relay.withTitle}</h3>
            <p className="mt-1 max-w-[44rem] opacity-90">{t.relay.with}</p>
          </div>
        </div>
      </div>
    </section>
  );
}

function Tokens() {
  const { t } = usePrefs();
  const bars = ["w-full", "w-[18%]", "w-0", "w-[42%]"];
  return (
    <section aria-labelledby="tokens-title" className="border-t border-rule">
      <div className={cn(shell, "py-24 sm:py-28")}>
        <div className="max-w-[44rem]">
          <H2 id="tokens-title">{t.tokens.title}</H2>
          <p className="mt-6 text-lg text-fg-muted">{t.tokens.body}</p>
        </div>
        <ol className="mt-14 divide-y divide-rule border-y border-rule">
          {t.tokens.rows.map((r, i) => (
            <li key={i} className="grid gap-x-8 gap-y-3 py-6 md:grid-cols-[13rem_1fr_15rem] md:items-center">
              <div>
                <div className="font-medium">{r.when}</div>
                <code translate="no" className="mt-1 block text-[0.75rem] text-accent">{r.tool}</code>
              </div>
              <p className="max-w-[36rem] text-fg-muted">{r.what}</p>
              <div className="flex items-center gap-3">
                <div className="relative h-2.5 flex-1 overflow-hidden rounded-full bg-bg-sunk" aria-hidden="true">
                  <div
                    className={cn(
                      "h-full rounded-full",
                      bars[i],
                      i === 3 ? "bg-[repeating-linear-gradient(90deg,var(--accent)_0_6px,transparent_6px_10px)]" : "bg-primary",
                    )}
                  />
                </div>
                <span className="tnum w-24 text-right font-mono text-[0.75rem] text-fg">{r.cost}</span>
              </div>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

function TrustLadder() {
  const { t } = usePrefs();
  const top: Trust[] = ["owner", "self"];
  const rest: Trust[] = ["peer_human", "peer_agent", "external"];
  const Row = ({ trust }: { trust: Trust }) => (
    <li className="flex items-baseline gap-4 py-3.5">
      <span className="w-28 shrink-0">
        <TrustTag trust={trust} tone="ground" className="text-[0.75rem]" />
      </span>
      <span className="text-fg-muted">{t.trust.levels[trust]}</span>
    </li>
  );
  return (
    <section aria-labelledby="trust-title" className="border-t border-rule bg-bg-sunk/60">
      <div className={cn(shell, "grid *:min-w-0 gap-14 py-24 sm:py-28 lg:grid-cols-[1.1fr_0.9fr] lg:gap-20")}>
        <div>
          <H2 id="trust-title">{t.trust.title}</H2>
          <p className="mt-6 max-w-[38rem] text-lg text-fg-muted">{t.trust.body}</p>
          <a href="#room" className="mt-6 inline-flex items-center gap-2 rounded-sm text-accent underline decoration-current/40 hover:decoration-current">
            {t.trust.tryIt}
            <ArrowUpRight className="size-4" aria-hidden="true" />
          </a>
        </div>
        <div className="rounded-lg border border-rule bg-bg-raised px-5 py-3 sm:px-7 lift">
          <ul>
            {top.map((x) => (
              <Row key={x} trust={x} />
            ))}
          </ul>
          <div className="my-2 flex items-center gap-3 text-[0.72rem] font-medium tracking-wide text-saffron-ink" aria-hidden="true">
            <span className="h-px flex-1 bg-saffron" />
            <span className="font-mono">{t.demo.instruct}</span>
            <span className="h-px w-4 bg-saffron" />
          </div>
          <ul className="divide-y divide-rule">
            {rest.map((x) => (
              <Row key={x} trust={x} />
            ))}
          </ul>
        </div>
      </div>
    </section>
  );
}

function Handoffs() {
  const { t } = usePrefs();
  return (
    <div>
      <h2 id="handoffs-title" className="display text-[clamp(1.8rem,3.2vw,2.5rem)]">
        {t.handoffs.title}
      </h2>
      <p className="mt-5 max-w-[34rem] text-fg-muted">{t.handoffs.body}</p>
      <figure className="mt-9 overflow-hidden rounded-lg border border-rule bg-bg-raised lift">
        <figcaption className="flex items-center gap-2 border-b border-rule px-4 py-2.5 font-mono text-[0.68rem] text-fg-muted">
          <FileText className="size-3.5 shrink-0" aria-hidden="true" />
          <span className="truncate">docs/memory/handoffs/2026-11-02-ui-kit-to-core-ATL-032.md</span>
        </figcaption>
        <pre className="overflow-x-auto px-4 py-4 font-mono text-[0.75rem] leading-[1.85]">
          <code>
            <span className="text-fg-muted">---</span>
            {"\n"}
            <K k="from" v="ui-kit" />
            <K k="to" v="core" />
            <K k="ticket" v="ATL-032" />
            <K k="status" v="open" accent />
            <K k="created" v="2026-11-02" />
            <span className="text-fg-muted">---</span>
            {"\n"}
            {"What changed: form tokens moved to ui-kit/tokens.ts\nWhat you need to do: use them in the login form\nDone when: core builds with no local colour values"}
          </code>
        </pre>
      </figure>
    </div>
  );
}

function K({ k, v, accent = false }: { k: string; v: string; accent?: boolean }) {
  return (
    <>
      <span className="text-accent">{k}</span>
      <span className="text-fg-muted">: </span>
      <span className={accent ? "rounded-[3px] bg-saffron/30 px-1" : ""}>{v}</span>
      {"\n"}
    </>
  );
}

function Humans() {
  const { t } = usePrefs();
  const lines: { who: string; bot?: boolean; text: string }[] = [
    { who: "davra_bot", bot: true, text: "frontend-ai → core: handoff ATL-032 opened" },
    { who: "backend", text: "@backend-ai take it after the cookie fix" },
    { who: "davra_bot", bot: true, text: "backend-ai: picked up ATL-032" },
  ];
  return (
    <div>
      <h2 className="display text-[clamp(1.8rem,3.2vw,2.5rem)]">{t.humans.title}</h2>
      <p className="mt-5 max-w-[34rem] text-fg-muted">{t.humans.body}</p>
      <ul className="mt-9 flex flex-col gap-3" aria-label="Telegram">
        {lines.map((l, i) => (
          <li
            key={i}
            className={cn(
              "max-w-[85%] rounded-2xl px-4 py-2.5 text-[0.92rem] leading-snug",
              l.bot ? "self-start rounded-bl-sm bg-bg-sunk" : "self-end rounded-br-sm bg-primary text-primary-fg",
            )}
          >
            <span className={cn("block text-[0.72rem] font-medium", l.bot ? "text-accent" : "text-primary-fg/80")}>
              {l.bot ? `@${l.who}` : l.who}
            </span>
            {l.text}
          </li>
        ))}
      </ul>
    </div>
  );
}

function Rules() {
  const { t } = usePrefs();
  const icons = [ShieldCheck, Repeat, KeyRound, ServerCog];
  return (
    <section aria-labelledby="rules-title" className="border-t border-rule">
      <div className={cn(shell, "py-24 sm:py-28")}>
        <H2 id="rules-title" className="max-w-[18ch]">
          {t.rules.title}
        </H2>
        <ul className="mt-14 grid gap-x-12 sm:grid-cols-2">
          {t.rules.items.map((r, i) => {
            const Icon = icons[i] ?? ShieldCheck;
            return (
              <li key={r.t} className="flex gap-5 border-t border-rule py-7">
                <Icon className="mt-1 size-5 shrink-0 text-accent" aria-hidden="true" />
                <div>
                  <h3 className="text-lg font-semibold">{r.t}</h3>
                  <p className="mt-1.5 max-w-[30rem] text-fg-muted">{r.d}</p>
                </div>
              </li>
            );
          })}
        </ul>
      </div>
    </section>
  );
}

function ForAgents() {
  const { t } = usePrefs();
  const tools = t.agents.tools as Record<string, string>;
  return (
    <section id="agents" aria-labelledby="agents-title" data-field className="relative bg-field text-field-fg">
      <div aria-hidden="true" className="rim-up absolute inset-x-0 -top-3" />
      <div className={cn(shell, "grid *:min-w-0 gap-14 py-24 sm:py-28 lg:grid-cols-[0.7fr_1.3fr] lg:gap-16")}>
        <div className="lg:sticky lg:top-10 lg:self-start">
          <H2 id="agents-title">{t.agents.title}</H2>
          <p className="mt-6 max-w-[30rem] text-lg text-field-muted">{t.agents.body}</p>
          <p className="mt-5 max-w-[30rem] border-t border-field-rule pt-5 text-field-fg">{t.agents.hint}</p>
          <Button asChild variant="fieldOutline" className="mt-8">
            <a href="/llms.txt">
              <FileText aria-hidden="true" />
              {t.agents.llms}
            </a>
          </Button>
        </div>
        <div>
          <p className="mb-4 font-mono text-[0.72rem] text-field-muted">{t.agents.count} · davra mcp</p>
          <dl className="overflow-hidden rounded-lg border border-field-rule bg-field-panel">
            {TOOL_GROUPS.map((g, gi) => (
              <div key={g.id} className={cn("grid sm:grid-cols-[7.5rem_1fr]", gi > 0 && "border-t border-field-rule")}>
                <div className="px-5 pt-4 text-[0.8rem] font-medium text-glaze sm:py-4">{t.agents.groups[g.id]}</div>
                <div className="divide-y divide-field-rule/60 pb-2 sm:pb-0">
                  {g.tools.map((name) => (
                    <div key={name} className="grid gap-1 px-5 py-3 md:grid-cols-[9.5rem_1fr] md:gap-5">
                      <dt>
                        <code translate="no" className="text-[0.76rem] text-field-fg">{name}</code>
                      </dt>
                      <dd className="text-[0.88rem] leading-snug text-field-muted">{tools[name]}</dd>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </dl>
        </div>
      </div>
      <div aria-hidden="true" className="rim absolute inset-x-0 -bottom-3" />
    </section>
  );
}

function SelfHost() {
  const { t } = usePrefs();
  return (
    <section id="start" aria-labelledby="start-title" className="scroll-mt-6">
      <div className={cn(shell, "grid *:min-w-0 gap-14 pt-28 pb-24 sm:pt-32 lg:grid-cols-[1fr_1fr] lg:gap-20")}>
        <div>
          <H2 id="start-title">{t.selfhost.title}</H2>
          <p className="mt-6 max-w-[34rem] text-lg text-fg-muted">{t.selfhost.body}</p>
          <div className="mt-12">
            <h3 className="text-lg font-semibold">{t.roadmap.title}</h3>
            <ul className="mt-4 flex flex-col">
              {t.roadmap.items.map((r) => (
                <li key={r} className="flex items-center justify-between gap-4 border-t border-rule py-3 last:border-b">
                  <span>{r}</span>
                  <span className="shrink-0 rounded-full border border-dashed border-fg-muted px-2.5 py-0.5 font-mono text-[0.66rem] text-fg-muted">
                    {t.roadmap.badge}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        </div>
        <div className="flex flex-col gap-6 lg:pt-3">
          <CodeBlock label={t.selfhost.selfLabel} lines={SELF_HOST} />
          <p className="-mt-3 text-sm text-fg-muted">
            <code className="text-[0.75rem] text-fg">--profile tunnel</code> · Cloudflare Tunnel
          </p>
          <CodeBlock label={t.selfhost.hostedLabel} lines={QUICK} />
        </div>
      </div>
    </section>
  );
}
