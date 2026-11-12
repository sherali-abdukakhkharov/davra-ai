import { useRef, useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router";
import { AuthLayout } from "@/components/site/AuthLayout";
import { EMAIL_RE, Field, FormAlert, SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { errorText, login } from "@/lib/api";
import { usePrefs } from "@/lib/prefs";

type Errors = { email?: string; password?: string };

export default function Login() {
  const { t } = usePrefs();
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [errors, setErrors] = useState<Errors>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const emailRef = useRef<HTMLInputElement>(null);
  const passwordRef = useRef<HTMLInputElement>(null);
  const alertRef = useRef<HTMLDivElement>(null);
  const e = t.auth.errors;

  const validate = (): Errors => {
    const next: Errors = {};
    const em = email.trim();
    if (!em) next.email = e.emailRequired;
    else if (!EMAIL_RE.test(em)) next.email = e.emailInvalid;
    if (!password) next.password = e.passwordRequired;
    return next;
  };

  const onSubmit = async (ev: FormEvent) => {
    ev.preventDefault();
    if (pending) return;
    const v = validate();
    setErrors(v);
    setFormError(null);
    if (v.email) return emailRef.current?.focus();
    if (v.password) return passwordRef.current?.focus();
    setPending(true);
    try {
      await login({ email: email.trim(), password });
      navigate("/app", { replace: true });
    } catch (err) {
      setPending(false);
      setFormError(errorText(err, e));
      requestAnimationFrame(() => alertRef.current?.focus());
    }
  };

  return (
    <AuthLayout>
      <h1 className="display text-[clamp(2rem,5vw,2.6rem)]">{t.auth.login.title}</h1>
      <p className="mt-3 text-fg-muted">{t.auth.login.sub}</p>
      <form noValidate onSubmit={onSubmit} className="mt-10" aria-busy={pending}>
        <FormAlert ref={alertRef} title={e.summary} message={formError} />
        <div className="flex flex-col gap-6">
          <Field
            ref={emailRef}
            label={t.auth.email}
            type="email"
            name="email"
            autoComplete="email"
            inputMode="email"
            autoCapitalize="none"
            placeholder="you@team.dev…"
            value={email}
            onChange={(x) => {
              setEmail(x.target.value);
              if (errors.email) setErrors((s) => ({ ...s, email: undefined }));
            }}
            error={errors.email}
            disabled={pending}
            required
          />
          <Field
            ref={passwordRef}
            label={t.auth.password}
            type="password"
            name="password"
            autoComplete="current-password"
            value={password}
            onChange={(x) => {
              setPassword(x.target.value);
              if (errors.password) setErrors((s) => ({ ...s, password: undefined }));
            }}
            error={errors.password}
            disabled={pending}
            required
          />
        </div>
        <Button type="submit" size="lg" className="mt-9 w-full" disabled={pending}>
          <SubmitLabel pending={pending} idle={t.auth.login.submit} busy={t.auth.login.submitting} />
        </Button>
      </form>
      <p className="mt-8 border-t border-rule pt-6 text-sm text-fg-muted">
        {t.auth.login.switchText}{" "}
        <Link to="/register" className="rounded-sm font-medium text-accent underline decoration-current/40 hover:decoration-current">
          {t.auth.login.switchLink}
        </Link>
      </p>
    </AuthLayout>
  );
}
