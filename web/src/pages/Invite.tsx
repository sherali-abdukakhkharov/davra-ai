import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { RoleBadge } from "@/components/app/States";
import { AuthLayout } from "@/components/site/AuthLayout";
import { Field, FormAlert, SubmitLabel } from "@/components/site/Form";
import { PageLoading } from "@/components/site/Loading";
import { Button } from "@/components/ui/button";
import { ApiError, session, type User } from "@/lib/api";
import { fill, usePrefs } from "@/lib/prefs";
import { web, webErrorText, type InviteInfo } from "@/lib/web";

const HANDLE = /^[a-z0-9._-]{1,64}$/;

/** A handle suggestion from a display name: "Gulom Bakirov" -> "gulom". */
function suggest(name: string): string {
  const first = name.trim().split(/\s+/)[0] ?? "";
  return first
    .normalize("NFKD")
    .toLowerCase()
    .replace(/[^a-z0-9._-]/g, "")
    .slice(0, 64);
}

type Loaded = { info: InviteInfo | null; user: User | null; error: unknown };

export default function Invite() {
  const { t } = usePrefs();
  const v = t.app.invite;
  const { code = "" } = useParams();
  const navigate = useNavigate();
  const [state, setState] = useState<Loaded | null>(null);
  const [handle, setHandle] = useState("");
  const [fieldError, setFieldError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const handleRef = useRef<HTMLInputElement>(null);
  const alertRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const ac = new AbortController();
    void (async () => {
      const [info, user] = await Promise.allSettled([web.inviteInfo(code, ac.signal), session(ac.signal)]);
      if (ac.signal.aborted) return;
      const u = user.status === "fulfilled" ? user.value : null;
      setState({
        info: info.status === "fulfilled" ? info.value : null,
        user: u,
        error: info.status === "rejected" ? info.reason : null,
      });
      if (u) setHandle(suggest(u.name));
    })();
    return () => ac.abort();
  }, [code]);

  if (!state) return <PageLoading label={v.loading} />;
  const { info, user, error } = state;
  const here = `/invite/${encodeURIComponent(code)}`;
  const project = info?.name || (info ? `${info.org}/${info.slug}` : "");

  const accept = async (e: FormEvent) => {
    e.preventDefault();
    if (pending || !info) return;
    const h = handle.trim().toLowerCase();
    if (!info.member && !HANDLE.test(h)) {
      setFieldError(t.app.create.invalid);
      handleRef.current?.focus();
      return;
    }
    setPending(true);
    setFieldError(null);
    setFormError(null);
    try {
      const r = await web.acceptInvite(code, info.member ? undefined : h);
      navigate(`/app/${encodeURIComponent(r.org)}/${encodeURIComponent(r.slug)}`, { replace: true });
    } catch (err) {
      setPending(false);
      const text = webErrorText(err, t.app.errors);
      if (err instanceof ApiError && err.field === "handle") {
        setFieldError(text);
        handleRef.current?.focus();
      } else {
        setFormError(text);
        requestAnimationFrame(() => alertRef.current?.focus());
      }
    }
  };

  // The link itself is the problem: say which way, and what to do.
  if (!info || info.status !== "open") {
    const why = !info
      ? error instanceof ApiError && error.status === 404
        ? v.invalid
        : webErrorText(error, t.app.errors)
      : info.status === "used"
        ? v.used
        : info.status === "expired"
          ? v.expired
          : v.revoked;
    return (
      <AuthLayout>
        <h1 className="display text-[clamp(2rem,5vw,2.6rem)]">{why}</h1>
        <p className="mt-3 text-fg-muted">{v.askNew}</p>
        <Button asChild className="mt-8">
          <Link to={user ? "/app" : "/"}>{user ? v.toApp : t.notFound.back}</Link>
        </Button>
      </AuthLayout>
    );
  }

  const intro = (
    <>
      <h1 className="display text-[clamp(2rem,5vw,2.6rem)]">{fill(v.title, { project })}</h1>
      <p className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-fg-muted">
        <span translate="no" className="font-mono text-[0.78rem]">
          {info.org}/{info.slug}
        </span>
        <RoleBadge role={info.role} />
      </p>
      <p className="mt-4">{info.invited_by ? fill(v.by, { by: info.invited_by, role: info.role }) : fill(v.byNobody, { role: info.role })}</p>
      <p className="mt-1 text-sm text-fg-muted">{t.app.members.roleHelp[info.role] ?? info.role}</p>
    </>
  );

  if (!user) {
    const next = `?next=${encodeURIComponent(here)}`;
    return (
      <AuthLayout>
        {intro}
        <p className="mt-8 border-t border-rule pt-6 text-fg-muted">{v.signInFirst}</p>
        <div className="mt-5 flex flex-wrap gap-3">
          <Button asChild>
            <Link to={`/register${next}`}>{v.register}</Link>
          </Button>
          <Button asChild variant="outline">
            <Link to={`/login${next}`}>{v.login}</Link>
          </Button>
        </div>
      </AuthLayout>
    );
  }

  return (
    <AuthLayout>
      {intro}
      <form noValidate onSubmit={accept} aria-busy={pending} className="mt-8 border-t border-rule pt-6">
        <FormAlert ref={alertRef} title={t.auth.errors.summary} message={formError} />
        <p className="text-sm text-fg-muted">{fill(v.signedInAs, { name: user.name })}</p>
        {info.member ? (
          <p className="mt-4">{fill(v.asMember, { handle: info.member })}</p>
        ) : (
          <Field
            ref={handleRef}
            className="mt-5"
            label={v.handle}
            name="handle"
            autoComplete="username"
            autoCapitalize="none"
            spellCheck={false}
            value={handle}
            onChange={(e) => {
              setHandle(e.target.value);
              setFieldError(null);
            }}
            error={fieldError}
            hint={fieldError ? null : v.handleHint}
            disabled={pending}
          />
        )}
        <Button type="submit" size="lg" className="mt-7 w-full" disabled={pending}>
          <SubmitLabel pending={pending} idle={v.accept} busy={v.accepting} />
        </Button>
      </form>
    </AuthLayout>
  );
}
