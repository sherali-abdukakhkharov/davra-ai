import { Check, Copy, Link2, LogOut, Plus, UserMinus } from "lucide-react";
import { useEffect, useId, useRef, useState, type ComponentProps, type FormEvent, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { useFormat } from "@/lib/format";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { grantable, isAdmin, web, webErrorText, type Invite, type Member, type ProjectRef, type Role } from "@/lib/web";
import { Notice, RoleBadge } from "./States";
import { useResource } from "./useResource";

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

const selectClass =
  "h-11 w-full min-w-0 cursor-pointer rounded-md border border-rule-strong bg-bg-raised px-3 text-[0.95rem] text-fg outline-none transition-[border-color,box-shadow] duration-150 hover:border-fg-muted/60 focus-visible:border-accent focus-visible:ring-3 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60";

type Props = {
  project: ProjectRef;
  members: Member[];
  /** The hub sent a fresh member list. */
  onMembers: (members: Member[]) => void;
  /** Something changed that needs a reload of the project. */
  onChanged: () => void;
};

export function MembersTab({ project, members, onMembers, onChanged }: Props) {
  const { t } = usePrefs();
  const m = t.app.members;
  const admin = isAdmin(project.role);
  const me = members.find((x) => x.is_me);
  const [status, setStatus] = useState<string | null>(null);
  const groups: [string, Member[]][] = [
    [m.humans, members.filter((x) => x.kind === "human")],
    [m.agents, members.filter((x) => x.kind === "agent")],
  ];
  return (
    <div className="flex flex-col gap-10">
      {admin ? <InvitePanel project={project} members={members} /> : null}
      {status ? <Notice tone="ok">{status}</Notice> : null}
      <div className="grid gap-10 *:min-w-0 lg:grid-cols-2">
        {groups.map(([title, list]) => (
          <section key={title} aria-label={title}>
            <h2 className="flex items-baseline gap-2 text-lg font-semibold">
              {title} <span className="tnum text-sm font-normal text-fg-muted">{list.length}</span>
            </h2>
            <ul className="mt-3 divide-y divide-rule border-y border-rule">
              {list.map((x) => (
                <MemberRow
                  key={x.handle}
                  project={project}
                  m={x}
                  onMembers={(next) => {
                    onMembers(next);
                    const changed = next.find((y) => y.handle === x.handle);
                    if (changed) setStatus(fill(m.roleSaved, { handle: x.handle, role: changed.role }));
                  }}
                  onRemoved={onChanged}
                />
              ))}
            </ul>
          </section>
        ))}
      </div>
      {me && me.role !== "owner" ? <Leave project={project} /> : null}
    </div>
  );
}

/** Inline "are you sure" strip, the same shape as revoking a token. */
function Confirm({
  text,
  yes,
  busy,
  pending,
  error,
  onYes,
  onNo,
}: {
  text: string;
  yes: string;
  busy: string;
  pending: boolean;
  error: string | null;
  onYes: () => void;
  onNo: () => void;
}) {
  const { t } = usePrefs();
  const yesRef = useRef<HTMLButtonElement>(null);
  useEffect(() => yesRef.current?.focus(), []);
  return (
    <div role="alertdialog" aria-label={text} className="mt-3 flex flex-col gap-2 rounded-md border border-danger/40 bg-danger-soft px-3.5 py-2.5">
      <div className="flex flex-wrap items-center gap-3">
        <p className="min-w-[12rem] flex-1 text-sm">{text}</p>
        <div className="flex gap-2">
          <Button ref={yesRef} size="sm" className="bg-danger text-bg hover:bg-danger/90" onClick={onYes} disabled={pending}>
            <SubmitLabel pending={pending} idle={yes} busy={busy} />
          </Button>
          <Button size="sm" variant="ghost" onClick={onNo} disabled={pending}>
            {t.app.members.keep}
          </Button>
        </div>
      </div>
      {error ? (
        <p role="alert" className="text-sm text-danger">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function MemberRow({ project, m: x, onMembers, onRemoved }: { project: ProjectRef; m: Member; onMembers: (m: Member[]) => void; onRemoved: () => void }) {
  const { t } = usePrefs();
  const m = t.app.members;
  const f = useFormat();
  const roles = grantable(project.role);
  const canRole = x.kind === "human" && !x.is_me && roles.includes(x.role);
  const canRemove = !x.is_me && (x.kind === "agent" ? x.mine || isAdmin(project.role) : roles.includes(x.role));
  const [saving, setSaving] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selectId = useId();

  const changeRole = async (role: Role) => {
    setSaving(true);
    setError(null);
    try {
      const r = await web.setMemberRole(project.org, project.slug, x.handle, role);
      onMembers(r.members);
    } catch (err) {
      setError(webErrorText(err, t.app.errors));
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    setPending(true);
    setError(null);
    try {
      await web.removeMember(project.org, project.slug, x.handle);
      onRemoved();
    } catch (err) {
      setPending(false);
      setError(webErrorText(err, t.app.errors));
    }
  };

  return (
    <li className="py-3.5">
      <div className="flex items-start gap-3">
        <Seat kind={x.kind} className="mt-1.5" />
        <div className="min-w-0 flex-1">
          <p className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span translate="no" className="font-mono text-[0.85rem] font-medium">
              @{x.handle}
            </span>
            {x.display_name ? <span className="text-sm text-fg-muted">{x.display_name}</span> : null}
            {x.is_me ? <span className="rounded-full bg-fg px-2 py-px text-[0.68rem] text-bg">{m.you}</span> : null}
            {x.mine ? <span className="rounded-full bg-accent px-2 py-px text-[0.68rem] text-bg">{m.yours}</span> : null}
            {x.kind === "human" && !x.linked ? <span className="rounded-full border border-rule-strong px-2 py-px text-[0.68rem] text-fg-muted">{m.noAccount}</span> : null}
          </p>
          <p className="mt-1 flex flex-wrap items-center gap-1.5 text-sm text-fg-muted">
            {!canRole ? <RoleBadge role={x.role} /> : null}
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
          {canRole || canRemove ? (
            <div className="mt-2.5 flex flex-wrap items-center gap-2">
              {canRole ? (
                <>
                  <label htmlFor={selectId} className="sr-only">
                    {fill(m.roleFor, { handle: x.handle })}
                  </label>
                  <select
                    id={selectId}
                    value={x.role}
                    disabled={saving}
                    aria-busy={saving}
                    onChange={(e) => void changeRole(e.target.value)}
                    className={cn(selectClass, "h-9 w-auto font-mono text-[0.8rem]")}
                  >
                    {roles.map((r) => (
                      <option key={r} value={r}>
                        {r}
                      </option>
                    ))}
                  </select>
                </>
              ) : null}
              {canRemove && !confirm ? (
                <Button variant="ghost" size="sm" className="text-danger hover:bg-danger-soft" onClick={() => setConfirm(true)}>
                  <UserMinus aria-hidden="true" />
                  {m.remove}
                </Button>
              ) : null}
            </div>
          ) : null}
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
      </div>
      {confirm ? (
        <Confirm
          text={fill(x.kind === "agent" ? m.removeAgentConfirm : m.removeConfirm, { handle: x.handle })}
          yes={m.removeYes}
          busy={m.removing}
          pending={pending}
          error={error}
          onYes={() => void remove()}
          onNo={() => {
            setConfirm(false);
            setError(null);
          }}
        />
      ) : error ? (
        <p role="alert" className="mt-2 text-sm text-danger">
          {error}
        </p>
      ) : null}
    </li>
  );
}

function Leave({ project }: { project: ProjectRef }) {
  const { t } = usePrefs();
  const m = t.app.members;
  const navigate = useNavigate();
  const [confirm, setConfirm] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const leave = async () => {
    setPending(true);
    setError(null);
    try {
      await web.removeMember(project.org, project.slug, project.handle);
      navigate("/app", { replace: true });
    } catch (err) {
      setPending(false);
      setError(webErrorText(err, t.app.errors));
    }
  };
  return (
    <section className="border-t border-rule pt-6" aria-label={m.leave}>
      {confirm ? (
        <Confirm text={m.leaveConfirm} yes={m.leaveYes} busy={m.leaving} pending={pending} error={error} onYes={() => void leave()} onNo={() => setConfirm(false)} />
      ) : (
        <Button variant="ghost" className="text-danger hover:bg-danger-soft" onClick={() => setConfirm(true)}>
          <LogOut aria-hidden="true" />
          {m.leave}
        </Button>
      )}
    </section>
  );
}

function SelectField({ label, hint, children, ...props }: { label: string; hint?: ReactNode } & ComponentProps<"select">) {
  const id = useId();
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <Label htmlFor={id}>{label}</Label>
      <select id={id} aria-describedby={hint ? `${id}-hint` : undefined} className={selectClass} {...props}>
        {children}
      </select>
      {hint ? (
        <p id={`${id}-hint`} className="text-sm text-fg-muted">
          {hint}
        </p>
      ) : null}
    </div>
  );
}

function InvitePanel({ project, members }: { project: ProjectRef; members: Member[] }) {
  const { t } = usePrefs();
  const m = t.app.members;
  const roles = grantable(project.role);
  // People added from the CLI that have no account yet can be handed their seat.
  const seats = members.filter((x) => x.kind === "human" && !x.linked && !x.is_me && roles.includes(x.role));
  const [role, setRole] = useState<Role>(roles.includes("member") ? "member" : roles[0]!);
  const [who, setWho] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<{ url: string; who: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const list = useResource((s) => web.invites(project.org, project.slug, s), [project.org, project.slug]);
  const linkRef = useRef<HTMLDivElement>(null);
  const codeId = useId();

  useEffect(() => {
    if (created) linkRef.current?.focus();
  }, [created]);

  const create = async (e: FormEvent) => {
    e.preventDefault();
    if (pending) return;
    setPending(true);
    setError(null);
    try {
      const r = await web.createInvite(project.org, project.slug, role, who || undefined);
      setCreated({ url: r.url, who: who ? `@${who}` : m.someoneNew });
      setCopied(false);
      void list.reload({ quiet: true });
    } catch (err) {
      setError(webErrorText(err, t.app.errors));
    } finally {
      setPending(false);
    }
  };

  const copy = async () => {
    if (!created) return;
    try {
      await navigator.clipboard.writeText(created.url);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      const el = document.getElementById(codeId);
      if (el) window.getSelection()?.selectAllChildren(el);
    }
  };

  return (
    <section aria-labelledby="invite-title" className="rounded-lg border border-rule-strong bg-bg-raised">
      <div className="px-5 pt-5 sm:px-6 sm:pt-6">
        <h2 id="invite-title" className="flex items-center gap-2 text-lg font-semibold">
          <Link2 className="size-4 text-accent" aria-hidden="true" />
          {m.inviteTitle}
        </h2>
        <p className="mt-1 max-w-[44rem] text-sm text-fg-muted">{m.inviteSub}</p>
      </div>

      <div className="px-5 py-5 sm:px-6">
        {created ? (
          <div ref={linkRef} tabIndex={-1} className="flex flex-col gap-3 outline-none">
            <p className="text-sm font-medium">{fill(m.linkTitle, { who: created.who })}</p>
            <div className="flex items-stretch gap-2">
              <code id={codeId} translate="no" className="min-w-0 flex-1 rounded-md border border-rule-strong bg-bg px-3.5 py-2.5 font-mono text-[0.78rem] break-all select-all">
                {created.url}
              </code>
              <Button variant="outline" onClick={copy} className="h-auto shrink-0" aria-label={m.copyLink}>
                {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
                <span className="hidden sm:inline" aria-live="polite">
                  {copied ? m.copied : m.copyLink}
                </span>
              </Button>
            </div>
            <p className="text-sm text-fg-muted">{m.linkOnce}</p>
            <div>
              <Button variant="ghost" size="sm" onClick={() => setCreated(null)}>
                <Plus aria-hidden="true" />
                {m.another}
              </Button>
            </div>
          </div>
        ) : (
          <form noValidate onSubmit={create} aria-busy={pending} className="flex flex-col gap-4">
            <div className="grid gap-5 *:min-w-0 md:grid-cols-2">
              <SelectField label={m.inviteRole} value={role} onChange={(e) => setRole(e.target.value)} hint={m.roleHelp[role]} disabled={pending}>
                {roles.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </SelectField>
              {seats.length ? (
                <SelectField label={m.inviteFor} value={who} onChange={(e) => setWho(e.target.value)} hint={m.inviteForHint} disabled={pending}>
                  <option value="">{m.inviteForNew}</option>
                  {seats.map((s) => (
                    <option key={s.handle} value={s.handle}>
                      {fill(m.inviteForMember, { handle: s.handle })}
                    </option>
                  ))}
                </SelectField>
              ) : null}
            </div>
            {error ? (
              <p role="alert" className="text-sm text-danger">
                {error}
              </p>
            ) : null}
            <div>
              <Button type="submit" disabled={pending}>
                <SubmitLabel pending={pending} idle={m.createInvite} busy={m.creatingInvite} />
              </Button>
            </div>
          </form>
        )}
      </div>

      <div className="border-t border-rule px-5 py-4 sm:px-6">
        <h3 className="text-sm font-semibold">{m.pending}</h3>
        {list.data && list.data.invites.length === 0 ? <p className="mt-1.5 text-sm text-fg-muted">{m.pendingNone}</p> : null}
        {list.data && list.data.invites.length ? (
          <ul className="mt-2 divide-y divide-rule">
            {list.data.invites.map((i) => (
              <InviteRow key={i.id} project={project} invite={i} onGone={() => void list.reload({ quiet: true })} />
            ))}
          </ul>
        ) : null}
      </div>
    </section>
  );
}

function InviteRow({ project, invite: i, onGone }: { project: ProjectRef; invite: Invite; onGone: () => void }) {
  const { t } = usePrefs();
  const m = t.app.members;
  const f = useFormat();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const withdraw = async () => {
    setPending(true);
    setError(null);
    try {
      await web.revokeInvite(project.org, project.slug, i.id);
      onGone();
    } catch (err) {
      setPending(false);
      setError(webErrorText(err, t.app.errors));
    }
  };
  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2.5 text-sm">
      <RoleBadge role={i.role} />
      {i.member ? (
        <span translate="no" className="font-mono text-[0.8rem]">
          {fill(m.forHandle, { handle: i.member })}
        </span>
      ) : null}
      <span className="text-fg-muted">
        {i.created_by ? `${fill(m.byHandle, { handle: i.created_by })} · ` : null}
        <time dateTime={i.expires_at} title={f.full(i.expires_at)}>
          {fill(m.expires, { time: f.relative(i.expires_at) })}
        </time>
      </span>
      <Button variant="ghost" size="sm" className="ml-auto text-danger hover:bg-danger-soft" onClick={() => void withdraw()} disabled={pending}>
        <SubmitLabel pending={pending} idle={m.withdraw} busy={m.withdrawing} />
      </Button>
      {error ? (
        <p role="alert" className="w-full text-danger">
          {error}
        </p>
      ) : null}
    </li>
  );
}
