import { useFormat } from "@/lib/format";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import type { Member } from "@/lib/web";
import { RoleBadge } from "./States";

/** The seat mark used in the landing room: a filled dot for people, a glazed ring for agents. */
export function Seat({ kind, className }: { kind: Member["kind"]; className?: string }) {
  return kind === "human" ? (
    <span aria-hidden="true" className={cn("inline-block size-3 shrink-0 rounded-full bg-primary", className)} />
  ) : (
    <span aria-hidden="true" className={cn("relative inline-block size-3 shrink-0 rounded-full border-2 border-accent", className)}>
      <span className="absolute inset-[2px] rounded-full bg-accent" />
    </span>
  );
}

export function MembersTab({ members }: { members: Member[] }) {
  const { t } = usePrefs();
  const m = t.app.members;
  const groups: [string, Member[]][] = [
    [m.humans, members.filter((x) => x.kind === "human")],
    [m.agents, members.filter((x) => x.kind === "agent")],
  ];
  return (
    <div className="grid gap-10 *:min-w-0 lg:grid-cols-2">
      {groups.map(([title, list]) => (
        <section key={title} aria-label={title}>
          <h2 className="flex items-baseline gap-2 text-lg font-semibold">
            {title} <span className="tnum text-sm font-normal text-fg-muted">{list.length}</span>
          </h2>
          <ul className="mt-3 divide-y divide-rule border-y border-rule">
            {list.map((x) => (
              <MemberRow key={x.handle} m={x} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

function MemberRow({ m: x }: { m: Member }) {
  const { t } = usePrefs();
  const m = t.app.members;
  const f = useFormat();
  return (
    <li className="flex items-start gap-3 py-3.5">
      <Seat kind={x.kind} className="mt-1.5" />
      <div className="min-w-0 flex-1">
        <p className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span translate="no" className="font-mono text-[0.85rem] font-medium">
            @{x.handle}
          </span>
          {x.display_name ? <span className="text-sm text-fg-muted">{x.display_name}</span> : null}
          {x.is_me ? <span className="rounded-full bg-fg px-2 py-px text-[0.68rem] text-bg">{m.you}</span> : null}
          {x.mine ? <span className="rounded-full bg-accent px-2 py-px text-[0.68rem] text-bg">{m.yours}</span> : null}
        </p>
        <p className="mt-1 flex flex-wrap items-center gap-1.5 text-sm text-fg-muted">
          <RoleBadge role={x.role} />
          {x.roles.length ? (
            x.roles.map((r) => (
              <span key={r} translate="no" className="rounded-[4px] bg-bg-sunk px-1.5 py-px font-mono text-[0.68rem] text-fg">
                {r}
              </span>
            ))
          ) : (
            <span className="text-[0.75rem]">{m.noRoles}</span>
          )}
          {x.owner ? <span className="text-[0.8rem]">· {fill(m.ownedBy, { owner: x.owner })}</span> : null}
        </p>
      </div>
      <span className="shrink-0 pt-0.5 text-right text-[0.78rem] text-fg-muted">
        {x.last_seen ? (
          <time dateTime={x.last_seen} title={f.full(x.last_seen)}>
            {fill(m.lastSeen, { time: f.relative(x.last_seen) })}
          </time>
        ) : (
          m.never
        )}
      </span>
    </li>
  );
}
