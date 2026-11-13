import { Unplug, Zap } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { bridge, type Client } from "../bridge";
import { Mono, PanelHeader, StatusPill, type Tone } from "../components/bits";
import { useDesktop, useReview } from "../state";

export function ClientsPanel() {
  const { t } = usePrefs();
  const c = t.desktop.clients;
  const { status } = useDesktop();

  return (
    <section aria-labelledby="clients-title" className="flex flex-col gap-5">
      <PanelHeader
        id="clients-title"
        title={c.title}
        sub={
          <>
            {c.sub} {c.hooksNote}
          </>
        }
      />
      <ul className="flex flex-col gap-3">
        {status.clients.map((client) => (
          <li key={client.id}>
            <ClientCard client={client} />
          </li>
        ))}
      </ul>
    </section>
  );
}

function ClientCard({ client }: { client: Client }) {
  const { t } = usePrefs();
  const c = t.desktop.clients;
  const openReview = useReview();
  const [backups, setBackups] = useState<string[]>([]);

  const tone: Tone = client.connected ? "ok" : client.partial ? "warn" : "off";
  const stateLabel = client.connected ? c.state.connected : client.partial ? c.state.partial : client.detected ? c.state.off : c.state.missing;
  const hooks = client.id === "claude";

  const connect = () =>
    openReview({
      title: fill(c.reviewConnect, { name: client.name }),
      empty: c.alreadyConnected,
      plan: () => bridge.planConnect(client.id),
      apply: () => bridge.applyConnect(client.id),
      onApplied: (a) => setBackups(a.backups),
    });

  const disconnect = () =>
    openReview({
      title: fill(c.reviewDisconnect, { name: client.name }),
      empty: c.alreadyDisconnected,
      plan: () => bridge.planDisconnect(client.id),
      apply: () => bridge.applyDisconnect(client.id),
      onApplied: (a) => setBackups(a.backups),
    });

  const titleId = `client-${client.id}`;

  return (
    <article
      aria-labelledby={titleId}
      className={cn("rounded-lg border bg-bg-raised", client.connected ? "border-rule" : client.partial ? "border-saffron/70" : "border-rule")}
    >
      <div className="flex flex-wrap items-start gap-x-4 gap-y-3 px-4 py-3.5">
        <div className="min-w-0 flex-1 basis-[18rem]">
          <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
            <h2 id={titleId} className="font-semibold">
              {client.name}
            </h2>
            <StatusPill tone={tone} className={cn(!client.detected && !client.connected && !client.partial && "opacity-80")}>
              {stateLabel}
            </StatusPill>
            <span className="text-[0.75rem] text-fg-muted">{hooks ? c.mcpHooks : c.mcpOnly}</span>
          </div>
          <Mono className="mt-1 block text-fg-muted">{client.detail}</Mono>
          {!client.detected && !client.connected && !client.partial ? <p className="mt-1 text-[0.8rem] text-fg-muted">{c.notFound}</p> : null}
        </div>
        <div className="flex flex-wrap gap-2">
          {client.partial ? (
            <Button size="sm" onClick={connect}>
              <Zap aria-hidden="true" />
              {c.finish}
            </Button>
          ) : null}
          {client.connected || client.partial ? (
            <Button size="sm" variant="outline" onClick={disconnect}>
              <Unplug aria-hidden="true" />
              {c.disconnect}
            </Button>
          ) : (
            <Button size="sm" variant={client.detected ? "primary" : "outline"} onClick={connect}>
              <Zap aria-hidden="true" />
              {c.connect}
            </Button>
          )}
        </div>
      </div>
      {backups.length ? (
        <details className="group border-t border-rule px-4 py-2 text-sm">
          <summary className="cursor-pointer rounded-sm text-fg-muted marker:text-fg-muted hover:text-fg">{c.lastBackups}</summary>
          <ul className="mt-1.5 flex flex-col gap-0.5 pb-1">
            {backups.map((b) => (
              <li key={b}>
                <Mono className="text-fg-muted">{b}</Mono>
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </article>
  );
}
