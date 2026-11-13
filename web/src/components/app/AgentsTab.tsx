import { Check, Copy, KeyRound, Pencil, Plus, TerminalSquare, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { CodeBlock } from "@/components/site/CodeBlock";
import { Field, SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { ApiError } from "@/lib/api";
import { useFormat } from "@/lib/format";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { web, webErrorText, type Member, type NewToken, type ProjectRef, type Token } from "@/lib/web";
import { Seat } from "./MembersTab";
import { Notice } from "./States";

const HANDLE = /^[a-z0-9._-]{1,64}$/;
const parseRoles = (s: string) => [...new Set(s.split(/[,\s]+/).map((x) => x.trim().toLowerCase()).filter(Boolean))];

type Props = { project: ProjectRef; members: Member[]; onChanged: () => void; onMember: (m: Member) => void };

export function AgentsTab({ project, members, onChanged, onMember }: Props) {
  const { t } = usePrefs();
  const a = t.app.agents;
  const mine = members.filter((m) => m.kind === "agent" && m.mine);
  const others = members.filter((m) => m.kind === "agent" && !m.mine);
  const [adding, setAdding] = useState(mine.length === 0);
  const [justAdded, setJustAdded] = useState<string | null>(null);

  return (
    <div className="flex flex-col gap-10">
      <section aria-labelledby="mine-title" className="flex flex-col gap-4">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <h2 id="mine-title" className="text-lg font-semibold">
              {a.yours}
            </h2>
            <p className="mt-1 max-w-[44rem] text-sm text-fg-muted">{a.sub}</p>
          </div>
          {!adding ? (
            <Button variant="outline" onClick={() => setAdding(true)}>
              <Plus aria-hidden="true" />
              {a.add}
            </Button>
          ) : null}
        </div>
        {adding ? (
          <AddAgent
            project={project}
            onCancel={mine.length ? () => setAdding(false) : undefined}
            onAdded={(m) => {
              onMember(m);
              setAdding(false);
              setJustAdded(m.handle);
            }}
          />
        ) : null}
        {mine.length === 0 && !adding ? <p className="text-fg-muted">{a.noneYours}</p> : null}
        <ul className="flex flex-col gap-4">
          {mine.map((m) => (
            <li key={m.handle}>
              <AgentCard project={project} agent={m} openConnect={justAdded === m.handle} onChanged={onChanged} onMember={onMember} />
            </li>
          ))}
        </ul>
      </section>

      <section aria-labelledby="others-title">
        <h2 id="others-title" className="text-lg font-semibold">
          {a.others}
        </h2>
        {others.length === 0 ? (
          <p className="mt-2 text-fg-muted">{a.noneOthers}</p>
        ) : (
          <ul className="mt-3 divide-y divide-rule border-y border-rule">
            {others.map((m) => (
              <li key={m.handle} className="flex flex-wrap items-center gap-x-3 gap-y-1 py-3">
                <Seat kind="agent" />
                <span translate="no" className="font-mono text-[0.85rem] font-medium">
                  @{m.handle}
                </span>
                <RoleChips roles={m.roles} />
                {m.owner ? <span className="ml-auto text-sm text-fg-muted">{fill(a.owner, { owner: m.owner })}</span> : null}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function RoleChips({ roles }: { roles: string[] }) {
  const { t } = usePrefs();
  if (!roles.length) return <span className="text-[0.75rem] text-fg-muted">{t.app.members.noRoles}</span>;
  return (
    <span className="flex flex-wrap gap-1.5">
      {roles.map((r) => (
        <span key={r} translate="no" className="rounded-[4px] bg-bg-sunk px-1.5 py-px font-mono text-[0.7rem]">
          {r}
        </span>
      ))}
    </span>
  );
}

function AddAgent({ project, onAdded, onCancel }: { project: ProjectRef; onAdded: (m: Member) => void; onCancel?: () => void }) {
  const { t } = usePrefs();
  const a = t.app.agents;
  const [handle, setHandle] = useState(`${project.handle}-ai`.slice(0, 64));
  const [roles, setRoles] = useState("");
  const [errors, setErrors] = useState<{ handle?: string; roles?: string; form?: string }>({});
  const [pending, setPending] = useState(false);
  const handleRef = useRef<HTMLInputElement>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (pending) return;
    const h = handle.trim();
    if (!HANDLE.test(h)) {
      setErrors({ handle: t.app.create.invalid });
      handleRef.current?.focus();
      return;
    }
    setPending(true);
    setErrors({});
    try {
      const { agent } = await web.addAgent(project.org, project.slug, h, parseRoles(roles));
      onAdded(agent);
    } catch (err) {
      setPending(false);
      const text = webErrorText(err, t.app.errors);
      const field = err instanceof ApiError ? err.field : null;
      if (field === "handle" || field === "roles") setErrors({ [field]: text });
      else setErrors({ form: text });
      if (field === "handle") handleRef.current?.focus();
    }
  };

  return (
    <form noValidate onSubmit={submit} aria-busy={pending} className="rounded-lg border border-rule-strong bg-bg-raised p-5 sm:p-6">
      <h3 className="font-semibold">{a.add}</h3>
      <div className="mt-4 grid gap-5 *:min-w-0 md:grid-cols-2">
        <Field ref={handleRef} label={a.handle} name="agent-handle" autoComplete="off" autoCapitalize="none" value={handle} onChange={(e) => setHandle(e.target.value)} error={errors.handle} hint={errors.handle ? null : a.handleHint} disabled={pending} />
        <Field label={a.roles} name="agent-roles" autoComplete="off" autoCapitalize="none" placeholder="ui-kit, core…" value={roles} onChange={(e) => setRoles(e.target.value)} error={errors.roles} hint={errors.roles ? null : a.rolesHint} disabled={pending} />
      </div>
      {errors.form ? (
        <p role="alert" className="mt-4 text-sm text-danger">
          {errors.form}
        </p>
      ) : null}
      <div className="mt-6 flex flex-wrap gap-3">
        <Button type="submit" disabled={pending}>
          <SubmitLabel pending={pending} idle={a.create} busy={a.creating} />
        </Button>
        {onCancel ? (
          <Button type="button" variant="ghost" onClick={onCancel} disabled={pending}>
            {a.cancel}
          </Button>
        ) : null}
      </div>
    </form>
  );
}

function AgentCard({ project, agent, openConnect, onChanged, onMember }: { project: ProjectRef; agent: Member; openConnect: boolean; onChanged: () => void; onMember: (m: Member) => void }) {
  const { t } = usePrefs();
  const a = t.app.agents;
  const [editing, setEditing] = useState(false);
  const [rolesDraft, setRolesDraft] = useState(agent.roles.join(", "));
  const [savingRoles, setSavingRoles] = useState(false);
  const [rolesError, setRolesError] = useState<string | null>(null);
  const [connect, setConnect] = useState(openConnect);

  const saveRoles = async (e: FormEvent) => {
    e.preventDefault();
    setSavingRoles(true);
    setRolesError(null);
    try {
      const { agent: next } = await web.setRoles(project.org, project.slug, agent.handle, parseRoles(rolesDraft));
      onMember(next);
      setEditing(false);
    } catch (err) {
      setRolesError(webErrorText(err, t.app.errors));
    } finally {
      setSavingRoles(false);
    }
  };

  return (
    <article className="overflow-hidden rounded-lg border border-rule bg-bg-raised" aria-labelledby={`agent-${agent.handle}`}>
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2 px-5 py-4">
        <Seat kind="agent" />
        <h3 id={`agent-${agent.handle}`} translate="no" className="font-mono text-[0.92rem] font-semibold">
          @{agent.handle}
        </h3>
        {!editing ? <RoleChips roles={agent.roles} /> : null}
        {!editing ? (
          <Button variant="ghost" size="sm" className="ml-auto" onClick={() => setEditing(true)}>
            <Pencil aria-hidden="true" />
            {a.editRoles}
          </Button>
        ) : null}
      </header>
      {editing ? (
        <form onSubmit={saveRoles} className="flex flex-col gap-3 border-t border-rule px-5 py-4">
          <Field
            label={a.roles}
            name={`roles-${agent.handle}`}
            autoComplete="off"
            autoCapitalize="none"
            value={rolesDraft}
            onChange={(e) => setRolesDraft(e.target.value)}
            hint={a.rolesHint}
            error={rolesError}
            disabled={savingRoles}
          />
          <div className="flex gap-2">
            <Button type="submit" size="sm" disabled={savingRoles}>
              <SubmitLabel pending={savingRoles} idle={a.saveRoles} busy={a.saving} />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => {
                setEditing(false);
                setRolesDraft(agent.roles.join(", "));
                setRolesError(null);
              }}
            >
              {a.cancel}
            </Button>
          </div>
        </form>
      ) : null}

      <div className="border-t border-rule px-5 py-4">
        <h4 className="flex items-center gap-2 text-sm font-semibold">
          <KeyRound className="size-4 text-accent" aria-hidden="true" />
          {a.tokens}
        </h4>
        {agent.tokens.length === 0 ? (
          <p className="mt-2 text-sm text-fg-muted">{a.noTokens}</p>
        ) : (
          <ul className="mt-2 divide-y divide-rule">
            {agent.tokens.map((tok) => (
              <TokenRow key={String(tok.id)} tok={tok} onRevoked={onChanged} />
            ))}
          </ul>
        )}
      </div>

      <div className="border-t border-rule bg-bg-sunk/50 px-5 py-4">
        {connect ? (
          <ConnectFlow project={project} agent={agent} onClose={() => setConnect(false)} onCreated={onChanged} />
        ) : (
          <Button variant="primary" onClick={() => setConnect(true)}>
            <TerminalSquare aria-hidden="true" />
            {a.connect}
          </Button>
        )}
      </div>
    </article>
  );
}

function TokenRow({ tok, onRevoked }: { tok: Token; onRevoked: () => void }) {
  const { t } = usePrefs();
  const a = t.app.agents;
  const f = useFormat();
  const [confirm, setConfirm] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const yesRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (confirm) yesRef.current?.focus();
  }, [confirm]);

  const revoke = async () => {
    setPending(true);
    setError(null);
    try {
      await web.revokeToken(tok.id);
      onRevoked();
    } catch (err) {
      setPending(false);
      setError(webErrorText(err, t.app.errors));
    }
  };

  return (
    <li className="flex flex-col gap-2 py-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span translate="no" className="font-mono text-[0.82rem] font-medium">
          {tok.name}
        </span>
        <span className="text-[0.78rem] text-fg-muted">
          <time dateTime={tok.created_at} title={f.full(tok.created_at)}>
            {fill(a.created, { time: f.relative(tok.created_at) })}
          </time>
          {" · "}
          {tok.last_used_at ? (
            <time dateTime={tok.last_used_at} title={f.full(tok.last_used_at)}>
              {fill(a.used, { time: f.relative(tok.last_used_at) })}
            </time>
          ) : (
            a.unused
          )}
        </span>
        {!confirm ? (
          <Button variant="ghost" size="sm" className="ml-auto text-danger hover:bg-danger-soft" onClick={() => setConfirm(true)}>
            {a.revoke}
          </Button>
        ) : null}
      </div>
      {confirm ? (
        <div role="alertdialog" aria-label={fill(a.revokeConfirm, { name: tok.name })} className="flex flex-wrap items-center gap-3 rounded-md border border-danger/40 bg-danger-soft px-3.5 py-2.5">
          <p className="flex-1 text-sm">{fill(a.revokeConfirm, { name: tok.name })}</p>
          <div className="flex gap-2">
            <Button ref={yesRef} size="sm" className="bg-danger text-bg hover:bg-danger/90" onClick={revoke} disabled={pending}>
              <SubmitLabel pending={pending} idle={a.revokeYes} busy={a.revoking} />
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setConfirm(false)} disabled={pending}>
              {a.keep}
            </Button>
          </div>
        </div>
      ) : null}
      {error ? (
        <p role="alert" className="text-sm text-danger">
          {error}
        </p>
      ) : null}
    </li>
  );
}

function ConnectFlow({ project, agent, onClose, onCreated }: { project: ProjectRef; agent: Member; onClose: () => void; onCreated: () => void }) {
  const { t } = usePrefs();
  const a = t.app.agents;
  const [name, setName] = useState(agent.tokens.length ? "" : "laptop");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<NewToken | null>(null);
  const [copied, setCopied] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);
  const revealRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!created) nameRef.current?.focus();
    else revealRef.current?.focus();
  }, [created]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (pending) return;
    if (!name.trim()) {
      setError(t.app.create.required);
      nameRef.current?.focus();
      return;
    }
    setPending(true);
    setError(null);
    try {
      const tok = await web.createToken(project.org, project.slug, agent.handle, name.trim());
      setCreated(tok);
      onCreated();
    } catch (err) {
      setError(webErrorText(err, t.app.errors));
    } finally {
      setPending(false);
    }
  };

  const copy = async () => {
    if (!created) return;
    try {
      await navigator.clipboard.writeText(created.token);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      // Clipboard refused: select the text so the person can copy it by hand.
      const el = document.getElementById(`tok-${String(created.id)}`);
      if (el) window.getSelection()?.selectAllChildren(el);
    }
  };

  if (created) {
    return (
      <div ref={revealRef} tabIndex={-1} className="flex flex-col gap-4 outline-none">
        <div role="alert" className="flex gap-3 rounded-md border border-saffron bg-saffron/15 px-4 py-3">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-saffron-ink" aria-hidden="true" />
          <div>
            <p className="font-semibold">{a.onceTitle}</p>
            <p className="mt-0.5 text-sm">{a.onceBody}</p>
          </div>
        </div>
        <div>
          <p className="mb-2 text-sm font-medium">{fill(a.step1, { handle: agent.handle })}</p>
          <div className="flex items-stretch gap-2">
            <code
              id={`tok-${String(created.id)}`}
              translate="no"
              className="min-w-0 flex-1 rounded-md border border-rule-strong bg-bg-raised px-3.5 py-2.5 font-mono text-[0.78rem] break-all select-all"
            >
              {created.token}
            </code>
            <Button variant="outline" onClick={copy} className="h-auto shrink-0" aria-label={a.copyToken}>
              {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
              <span className="hidden sm:inline" aria-live="polite">
                {copied ? a.copied : a.copyToken}
              </span>
            </Button>
          </div>
        </div>
        <div>
          <p className="mb-2 text-sm font-medium">{a.step2}</p>
          <CodeBlock label={a.connect} lines={created.commands} />
        </div>
        <div>
          <Button onClick={onClose}>{a.done}</Button>
        </div>
      </div>
    );
  }

  return (
    <form noValidate onSubmit={submit} className="flex flex-col gap-4">
      <h4 className="flex items-center gap-2 font-semibold">
        <TerminalSquare className="size-4 text-accent" aria-hidden="true" />
        {a.connect}
      </h4>
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start">
        <Field
          ref={nameRef}
          className="sm:max-w-[18rem] sm:flex-1"
          label={a.tokenName}
          name={`token-${agent.handle}`}
          autoComplete="off"
          autoCapitalize="none"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            setError(null);
          }}
          error={error}
          hint={error ? null : a.tokenNameHint}
          disabled={pending}
        />
        <div className={cn("flex gap-2 sm:pt-[1.6rem]")}>
          <Button type="submit" size="lg" disabled={pending}>
            <SubmitLabel pending={pending} idle={a.createToken} busy={a.creatingToken} />
          </Button>
          <Button type="button" variant="ghost" size="lg" onClick={onClose} disabled={pending}>
            {a.cancel}
          </Button>
        </div>
      </div>
      <Notice>{a.onceBody}</Notice>
    </form>
  );
}
