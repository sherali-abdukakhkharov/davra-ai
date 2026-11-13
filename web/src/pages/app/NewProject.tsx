import { ArrowLeft } from "lucide-react";
import { useRef, useState, type ChangeEvent, type FormEvent, type RefObject } from "react";
import { Link, useNavigate, useOutletContext } from "react-router";
import { Field, FormAlert, SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { ApiError } from "@/lib/api";
import { usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { web, webErrorText } from "@/lib/web";
import type { AppContext } from "./AppLayout";
import { shell } from "./Overview";

const SLUG = /^[a-z0-9._-]{1,64}$/;
type Key = "org" | "org_name" | "slug" | "name" | "handle";

/** Lowercases and strips a display name down to a usable handle or slug. */
function toSlug(s: string) {
  return s
    .normalize("NFKD")
    .replace(/[̀-ͯʻʼ’']/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

export default function NewProject() {
  const { t } = usePrefs();
  const c = t.app.create;
  const { user } = useOutletContext<AppContext>();
  const navigate = useNavigate();
  const [v, setV] = useState<Record<Key, string>>({ org: "", org_name: "", slug: "", name: "", handle: toSlug(user.name) });
  const [slugTouched, setSlugTouched] = useState(false);
  const [errors, setErrors] = useState<Partial<Record<Key, string>>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const refs: Record<Key, RefObject<HTMLInputElement | null>> = {
    org: useRef<HTMLInputElement>(null),
    org_name: useRef<HTMLInputElement>(null),
    slug: useRef<HTMLInputElement>(null),
    name: useRef<HTMLInputElement>(null),
    handle: useRef<HTMLInputElement>(null),
  };
  const alertRef = useRef<HTMLDivElement>(null);

  const set = (k: Key) => (e: ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value;
    setV((s) => {
      const next = { ...s, [k]: value };
      if (k === "name" && !slugTouched) next.slug = toSlug(value);
      return next;
    });
    if (k === "slug") setSlugTouched(true);
    if (errors[k]) setErrors((s) => ({ ...s, [k]: undefined }));
  };

  const onSubmit = async (ev: FormEvent) => {
    ev.preventDefault();
    if (pending) return;
    const next: Partial<Record<Key, string>> = {};
    for (const k of ["org", "slug", "handle"] as const) {
      if (!v[k].trim()) next[k] = c.required;
      else if (!SLUG.test(v[k].trim())) next[k] = c.invalid;
    }
    if (!v.name.trim()) next.name = c.required;
    setErrors(next);
    setFormError(null);
    const first = (["org", "name", "slug", "handle"] as const).find((k) => next[k]);
    if (first) return refs[first].current?.focus();
    setPending(true);
    try {
      const { project } = await web.createProject({
        org: v.org.trim(),
        org_name: v.org_name.trim() || undefined,
        slug: v.slug.trim(),
        name: v.name.trim(),
        handle: v.handle.trim(),
      });
      navigate(`/app/${encodeURIComponent(project.org)}/${encodeURIComponent(project.slug)}`, { replace: true });
    } catch (err) {
      setPending(false);
      const text = webErrorText(err, t.app.errors);
      const field = err instanceof ApiError ? err.field : null;
      if (field && field in refs) {
        setErrors({ [field]: text });
        requestAnimationFrame(() => refs[field as Key].current?.focus());
        return;
      }
      setFormError(text);
      requestAnimationFrame(() => alertRef.current?.focus());
    }
  };

  return (
    <div className={cn(shell, "py-10 sm:py-14")}>
      <Link to="/app" className="inline-flex items-center gap-2 rounded-sm text-sm text-fg-muted hover:text-fg">
        <ArrowLeft className="size-4" aria-hidden="true" />
        {t.app.project.back}
      </Link>
      <div className="mt-6 grid gap-10 *:min-w-0 lg:grid-cols-[minmax(0,26rem)_1fr] lg:gap-20">
        <div>
          <h1 className="display text-[clamp(1.9rem,4vw,2.6rem)]">{c.title}</h1>
          <p className="mt-3 text-fg-muted">{c.sub}</p>
          <pre
            aria-hidden="true"
            className="mt-8 hidden overflow-x-auto rounded-lg border border-rule bg-bg-raised px-4 py-3.5 font-mono text-[0.75rem] leading-[1.9] text-fg-muted lg:block"
          >
            <span className="text-fg"># .davra.toml</span>
            {"\n"}
            <span className="text-accent">org</span>     = "{v.org || "…"}"{"\n"}
            <span className="text-accent">project</span> = "{v.slug || "…"}"
          </pre>
        </div>
        <form noValidate onSubmit={onSubmit} aria-busy={pending} className="max-w-[32rem]">
          <FormAlert ref={alertRef} title={t.auth.errors.summary} message={formError} />
          <div className="grid gap-6 *:min-w-0 sm:grid-cols-2">
            <Field ref={refs.org} label={c.org} name="org" autoComplete="off" autoCapitalize="none" placeholder="qitmir…" value={v.org} onChange={set("org")} error={errors.org} hint={errors.org ? null : c.orgHint} disabled={pending} />
            <Field ref={refs.org_name} label={c.orgName} name="org_name" autoComplete="organization" placeholder="Qitmir…" value={v.org_name} onChange={set("org_name")} error={errors.org_name} hint={errors.org_name ? null : c.orgNameHint} disabled={pending} />
            <Field ref={refs.name} className="sm:col-span-2" label={c.name} name="name" autoComplete="off" placeholder="Atlas…" value={v.name} onChange={set("name")} error={errors.name} disabled={pending} />
            <Field ref={refs.slug} label={c.slug} name="slug" autoComplete="off" autoCapitalize="none" placeholder="atlas…" value={v.slug} onChange={set("slug")} error={errors.slug} hint={errors.slug ? null : c.slugHint} disabled={pending} />
            <Field ref={refs.handle} label={c.handle} name="handle" autoComplete="username" autoCapitalize="none" value={v.handle} onChange={set("handle")} error={errors.handle} hint={errors.handle ? null : c.handleHint} disabled={pending} />
          </div>
          <div className="mt-9 flex flex-wrap gap-3">
            <Button type="submit" size="lg" disabled={pending}>
              <SubmitLabel pending={pending} idle={c.submit} busy={c.submitting} />
            </Button>
            <Button asChild variant="ghost" size="lg">
              <Link to="/app">{c.cancel}</Link>
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}
