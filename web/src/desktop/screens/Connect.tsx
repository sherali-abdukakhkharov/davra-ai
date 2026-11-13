import { ArrowRight, CircleAlert, CircleCheck, ExternalLink, KeyRound, RotateCw } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Field, FormAlert, SubmitLabel } from "@/components/site/Form";
import { Logo } from "@/components/site/Logo";
import { LangToggle, ThemeToggle } from "@/components/site/Prefs";
import { Button } from "@/components/ui/button";
import { fill, usePrefs } from "@/lib/prefs";
import { HUB_URL } from "@/lib/utils";
import { bridge, errorText } from "../bridge";
import { MockBadge } from "../components/bits";
import { hubHost, useDesktop } from "../state";

type Props = {
  /** Show the success view (just signed in) instead of the form. */
  welcome: boolean;
  onConnected: () => void;
  onContinue: () => void;
};

export function ConnectScreen({ welcome, onConnected, onContinue }: Props) {
  const { t } = usePrefs();
  const c = t.desktop.connect;
  const { status } = useDesktop();
  const webUrl = status.hub ?? HUB_URL;

  return (
    <div className="flex min-h-dvh flex-col min-[700px]:grid min-[700px]:grid-cols-[minmax(17rem,0.8fr)_minmax(0,1.2fr)]">
      <aside data-field className="relative flex flex-col bg-field text-field-fg">
        <div className="flex h-14 items-center gap-3 px-5 min-[700px]:h-16 min-[700px]:px-7">
          <Logo />
          <MockBadge className="border-saffron text-saffron" />
          <div className="ml-auto flex items-center gap-2 min-[700px]:hidden">
            <LangToggle onField />
            <ThemeToggle onField />
          </div>
        </div>
        <div className="flex flex-1 flex-col px-5 pt-2 pb-6 min-[700px]:justify-center min-[700px]:px-7 min-[700px]:pb-16">
          <h1 className="max-w-[18ch] text-[1.6rem] leading-tight font-semibold tracking-[-0.02em] [font-variation-settings:'SHRP'_60] min-[700px]:text-[1.9rem]">
            {c.title}
          </h1>
          <p className="mt-3 max-w-[34rem] text-sm text-field-muted">{c.sub}</p>
          <section aria-labelledby="where-title" className="mt-6 max-w-[34rem] border-t border-field-rule pt-5 min-[700px]:mt-8">
            <h2 id="where-title" className="flex items-center gap-2 text-sm font-medium">
              <KeyRound className="size-4 text-glaze" aria-hidden="true" />
              {c.whereTitle}
            </h2>
            <ol className="mt-3 flex flex-col gap-2.5 text-sm text-field-muted">
              {c.steps.map((s, i) => (
                <li key={i} className="flex gap-3">
                  <span aria-hidden="true" className="tnum mt-px inline-flex size-5 shrink-0 items-center justify-center rounded-full border border-field-rule font-mono text-[0.65rem] text-field-fg">
                    {i + 1}
                  </span>
                  <span>{fill(s, { connect: t.app.agents.connect, create: t.app.agents.createToken })}</span>
                </li>
              ))}
            </ol>
            <Button variant="fieldOutline" size="sm" className="mt-5" onClick={() => void bridge.openUrl(webUrl)}>
              <ExternalLink aria-hidden="true" />
              {c.openWeb}
              <span className="font-mono text-[0.72rem] text-field-muted">{hubHost(webUrl)}</span>
            </Button>
          </section>
        </div>
      </aside>

      <main className="flex flex-1 flex-col">
        <div className="hidden h-16 items-center justify-end gap-2 px-7 min-[700px]:flex">
          <LangToggle />
          <ThemeToggle />
        </div>
        <div className="flex flex-1 items-start justify-center px-5 py-8 min-[700px]:items-center min-[700px]:px-10 min-[700px]:pb-20">
          <div className="w-full max-w-[25rem]">
            {welcome ? <Welcome onContinue={onContinue} /> : <Body onConnected={onConnected} />}
          </div>
        </div>
      </main>
    </div>
  );
}

/** Either the error view (hub set, /me failed) or the token form. */
function Body({ onConnected }: { onConnected: () => void }) {
  const { status } = useDesktop();
  const [editing, setEditing] = useState(false);
  if (status.hub && status.me_error && !editing) return <HubError onChangeToken={() => setEditing(true)} />;
  return <TokenForm onConnected={onConnected} onCancel={status.hub && status.me_error ? () => setEditing(false) : undefined} />;
}

function HubError({ onChangeToken }: { onChangeToken: () => void }) {
  const { t } = usePrefs();
  const c = t.desktop.connect;
  const { status, setStatus } = useDesktop();
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const ref = useRef<HTMLHeadingElement>(null);

  const retry = async () => {
    setPending(true);
    setFailed(null);
    try {
      const s = await bridge.status();
      setStatus(s);
      // Still failing: say so again, so a retry that changes nothing is visible.
      if (!s.me) ref.current?.focus();
    } catch (err) {
      setFailed(errorText(err));
    } finally {
      setPending(false);
    }
  };

  return (
    <div role="alert" aria-busy={pending} className="flex flex-col gap-4">
      <div className="flex gap-3">
        <CircleAlert className="mt-1 size-5 shrink-0 text-danger" aria-hidden="true" />
        <div className="min-w-0">
          <h2 ref={ref} tabIndex={-1} className="text-lg font-semibold outline-none">
            {fill(c.errorTitle, { hub: hubHost(status.hub ?? "") })}
          </h2>
          <p className="mt-2 rounded-md border border-danger/40 bg-danger-soft px-3 py-2 font-mono text-[0.78rem] break-words">
            {failed ?? status.me_error}
          </p>
          <p className="mt-3 text-sm text-fg-muted">{c.errorHint}</p>
        </div>
      </div>
      <div className="flex flex-wrap gap-2 pl-8">
        <Button onClick={() => void retry()} disabled={pending} size="sm">
          {pending ? null : <RotateCw aria-hidden="true" />}
          <SubmitLabel pending={pending} idle={c.retry} busy={c.retrying} />
        </Button>
        <Button variant="outline" size="sm" onClick={onChangeToken} disabled={pending}>
          <KeyRound aria-hidden="true" />
          {c.changeToken}
        </Button>
      </div>
    </div>
  );
}

function validHub(v: string): boolean {
  try {
    const u = new URL(v);
    return (u.protocol === "https:" || u.protocol === "http:") && Boolean(u.host);
  } catch {
    return false;
  }
}

function TokenForm({ onConnected, onCancel }: { onConnected: () => void; onCancel?: () => void }) {
  const { t } = usePrefs();
  const c = t.desktop.connect;
  const { status, setStatus } = useDesktop();
  const [hub, setHub] = useState(status.hub ?? HUB_URL);
  const [token, setToken] = useState("");
  const [errors, setErrors] = useState<{ hub?: string; token?: string; form?: string }>({});
  const [pending, setPending] = useState(false);
  const hubRef = useRef<HTMLInputElement>(null);
  const tokenRef = useRef<HTMLInputElement>(null);
  const alertRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // The hub is usually prefilled, so start where the typing is.
    // Only on mount, so it reads the initial hub.
    (hub ? tokenRef : hubRef).current?.focus();
  }, []);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (pending) return;
    const h = hub.trim().replace(/\/+$/, "");
    const tk = token.trim();
    const next: typeof errors = {};
    if (!h) next.hub = c.hubRequired;
    else if (!validHub(h)) next.hub = c.hubInvalid;
    if (!tk) next.token = c.tokenRequired;
    else if (!tk.startsWith("dvr_")) next.token = c.tokenShape;
    if (next.hub || next.token) {
      setErrors(next);
      (next.hub ? hubRef : tokenRef).current?.focus();
      return;
    }
    setErrors({});
    setPending(true);
    try {
      const s = await bridge.login(h, tk);
      setStatus(s);
      if (s.me) onConnected();
      else {
        setErrors({ form: s.me_error ?? c.failed });
        alertRef.current?.focus();
      }
    } catch (err) {
      setErrors({ form: errorText(err) });
      requestAnimationFrame(() => alertRef.current?.focus());
    } finally {
      setPending(false);
    }
  };

  return (
    <form noValidate onSubmit={submit} aria-busy={pending} className="flex flex-col">
      <FormAlert ref={alertRef} title={c.failed} message={errors.form ?? null} />
      <div className="flex flex-col gap-5">
        <Field
          ref={hubRef}
          label={c.hub}
          name="hub"
          type="url"
          inputMode="url"
          autoComplete="url"
          autoCapitalize="none"
          placeholder="https://davra.example.com"
          value={hub}
          onChange={(e) => setHub(e.target.value)}
          error={errors.hub}
          hint={errors.hub ? null : c.hubHint}
          disabled={pending}
          className="[&_input]:font-mono [&_input]:text-[0.88rem]"
        />
        <Field
          ref={tokenRef}
          label={c.token}
          name="token"
          type="password"
          autoComplete="off"
          autoCapitalize="none"
          placeholder="dvr_…"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          error={errors.token}
          hint={errors.token ? null : c.tokenHint}
          disabled={pending}
          className="[&_input]:font-mono [&_input]:text-[0.88rem]"
        />
      </div>
      <div className="mt-7 flex flex-wrap gap-2">
        <Button type="submit" disabled={pending} className="min-w-32">
          <SubmitLabel pending={pending} idle={c.submit} busy={c.submitting} />
        </Button>
        {onCancel ? (
          <Button type="button" variant="ghost" onClick={onCancel} disabled={pending}>
            {c.cancel}
          </Button>
        ) : null}
      </div>
    </form>
  );
}

function Welcome({ onContinue }: { onContinue: () => void }) {
  const { t } = usePrefs();
  const c = t.desktop.connect;
  const { status } = useDesktop();
  const me = status.me;
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => ref.current?.focus(), []);
  if (!me) return null;
  return (
    <div role="status" className="flex flex-col gap-5">
      <div className="flex gap-3">
        <CircleCheck className="mt-1 size-5 shrink-0 text-ok" aria-hidden="true" />
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">
            <span translate="no">{fill(c.successTitle, { handle: me.handle, org: me.org })}</span>
          </h2>
          <p className="mt-1 font-mono text-[0.75rem] text-fg-muted">{status.hub}</p>
          <p className="mt-3 text-sm">
            {me.projects.length ? fill(c.successProjects, { list: me.projects.map((p) => p.slug).join(", ") }) : c.successNoProjects}
          </p>
        </div>
      </div>
      <div className="pl-8">
        <Button ref={ref} onClick={onContinue}>
          {c.continue}
          <ArrowRight aria-hidden="true" />
        </Button>
      </div>
    </div>
  );
}
