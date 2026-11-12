import { FolderOpen, LogOut } from "lucide-react";
import { useState } from "react";
import { Link, redirect, useLoaderData, useNavigate } from "react-router";
import { CodeBlock } from "@/components/site/CodeBlock";
import { FormAlert, SubmitLabel } from "@/components/site/Form";
import { Logo } from "@/components/site/Logo";
import { SkipLink } from "@/components/site/Nav";
import { LangToggle, ThemeToggle } from "@/components/site/Prefs";
import { Button } from "@/components/ui/button";
import { errorText, logout, session, type User } from "@/lib/api";
import { fill, usePrefs } from "@/lib/prefs";
import { HUB_URL } from "@/lib/utils";

export async function loader({ request }: { request: Request }) {
  let user: User | null = null;
  try {
    user = await session(request.signal);
  } catch {
    user = null;
  }
  if (!user) throw redirect("/login");
  return { user };
}

export default function AppHome() {
  const { t } = usePrefs();
  const { user } = useLoaderData() as { user: User };
  const navigate = useNavigate();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onLogout = async () => {
    setPending(true);
    setError(null);
    try {
      await logout();
      navigate("/login", { replace: true });
    } catch (err) {
      setPending(false);
      setError(errorText(err, t.auth.errors));
    }
  };

  return (
    <div className="flex min-h-dvh flex-col">
      <SkipLink />
      <header className="border-b border-rule">
        <div className="mx-auto flex h-16 max-w-[76rem] items-center gap-2 px-4 sm:px-6 lg:px-8">
          <Link to="/" aria-label={t.nav.home} className="mr-auto -ml-1 rounded-md px-1 py-1">
            <Logo />
          </Link>
          <LangToggle />
          <ThemeToggle />
          <Button variant="outline" size="sm" className="ml-1" onClick={onLogout} disabled={pending}>
            {pending ? null : <LogOut aria-hidden="true" />}
            <SubmitLabel pending={pending} idle={t.app.logout} busy={t.app.loggingOut} />
          </Button>
        </div>
      </header>
      <main id="main" tabIndex={-1} className="mx-auto w-full max-w-[76rem] flex-1 px-4 py-14 outline-none sm:px-6 lg:px-8">
        <FormAlert title={t.auth.errors.summary} message={error} />
        <p className="text-fg-muted">
          {fill(t.app.signedInAs, { name: "\u0000" })
            .split("\u0000")
            .flatMap((part, i) => (i === 0 ? [part] : [<strong key={i} className="font-semibold text-fg">{user.name}</strong>, part]))}
        </p>
        <section aria-labelledby="projects-title" className="mt-10 rounded-lg border border-dashed border-rule-strong px-6 py-14 sm:px-12">
          <FolderOpen className="size-6 text-accent" aria-hidden="true" />
          <h1 id="projects-title" className="display mt-5 text-[clamp(1.9rem,4vw,2.8rem)]">
            {t.app.title}
          </h1>
          <p className="mt-4 max-w-[40rem] text-fg-muted">{t.app.body}</p>
          <CodeBlock
            className="mt-8 max-w-[40rem]"
            lines={[`davra login --hub ${HUB_URL}`, "claude mcp add --scope user davra -- davra mcp"]}
          />
        </section>
      </main>
    </div>
  );
}
