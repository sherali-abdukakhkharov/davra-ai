import { Check } from "lucide-react";
import { useRef, useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router";
import { AuthLayout } from "@/components/site/AuthLayout";
import { EMAIL_RE, Field, FormAlert, SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { ApiError, errorText, register } from "@/lib/api";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";

const MIN = 10;
type Errors = { name?: string; email?: string; password?: string };

export default function Register() {
  const { t } = usePrefs();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [errors, setErrors] = useState<Errors>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const refs = {
    name: useRef<HTMLInputElement>(null),
    email: useRef<HTMLInputElement>(null),
    password: useRef<HTMLInputElement>(null),
  };
  const alertRef = useRef<HTMLDivElement>(null);
  const e = t.auth.errors;
  const length = [...password].length;
  const longEnough = length >= MIN;

  const validate = (): Errors => {
    const next: Errors = {};
    if (!name.trim()) next.name = e.nameRequired;
    const em = email.trim();
    if (!em) next.email = e.emailRequired;
    else if (!EMAIL_RE.test(em)) next.email = e.emailInvalid;
    if (!password) next.password = e.passwordRequired;
    else if (!longEnough) next.password = e.passwordShort;
    return next;
  };

  const onSubmit = async (ev: FormEvent) => {
    ev.preventDefault();
    if (pending) return;
    const v = validate();
    setErrors(v);
    setFormError(null);
    const first = (["name", "email", "password"] as const).find((k) => v[k]);
    if (first) return refs[first].current?.focus();
    setPending(true);
    try {
      await register({ name: name.trim(), email: email.trim(), password });
      navigate("/app", { replace: true });
    } catch (err) {
      setPending(false);
      const text = errorText(err, e);
      const field = err instanceof ApiError ? err.field : null;
      if (field === "name" || field === "email" || field === "password") {
        setErrors({ [field]: text });
        requestAnimationFrame(() => refs[field].current?.focus());
        return;
      }
      setFormError(text);
      requestAnimationFrame(() => alertRef.current?.focus());
    }
  };

  const clear = (k: keyof Errors) => errors[k] && setErrors((s) => ({ ...s, [k]: undefined }));

  return (
    <AuthLayout>
      <h1 className="display text-[clamp(2rem,5vw,2.6rem)]">{t.auth.register.title}</h1>
      <p className="mt-3 text-fg-muted">{t.auth.register.sub}</p>
      <form noValidate onSubmit={onSubmit} className="mt-10" aria-busy={pending}>
        <FormAlert ref={alertRef} title={e.summary} message={formError} />
        <div className="flex flex-col gap-6">
          <Field
            ref={refs.name}
            label={t.auth.name}
            name="name"
            autoComplete="name"
            value={name}
            onChange={(x) => {
              setName(x.target.value);
              clear("name");
            }}
            error={errors.name}
            disabled={pending}
            required
          />
          <Field
            ref={refs.email}
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
              clear("email");
            }}
            error={errors.email}
            disabled={pending}
            required
          />
          <Field
            ref={refs.password}
            label={t.auth.password}
            type="password"
            name="password"
            autoComplete="new-password"
            minLength={MIN}
            value={password}
            onChange={(x) => {
              setPassword(x.target.value);
              clear("password");
            }}
            error={errors.password}
            hint={errors.password ? null : t.auth.register.passwordHint}
            aside={
              <span
                className={cn(
                  "tnum inline-flex items-center gap-1 font-mono text-[0.7rem] transition-colors",
                  longEnough ? "text-ok" : "text-fg-muted",
                )}
                aria-hidden="true"
              >
                {longEnough ? <Check className="size-3.5" /> : null}
                {fill(t.auth.register.passwordCount, { n: Math.min(length, MIN) })}
              </span>
            }
            disabled={pending}
            required
          />
        </div>
        <Button type="submit" size="lg" className="mt-9 w-full" disabled={pending}>
          <SubmitLabel pending={pending} idle={t.auth.register.submit} busy={t.auth.register.submitting} />
        </Button>
      </form>
      <p className="mt-8 border-t border-rule pt-6 text-sm text-fg-muted">
        {t.auth.register.switchText}{" "}
        <Link to="/login" className="rounded-sm font-medium text-accent underline decoration-current/40 hover:decoration-current">
          {t.auth.register.switchLink}
        </Link>
      </p>
    </AuthLayout>
  );
}
