import { LogOut } from "lucide-react";
import { useState } from "react";
import { Link, Outlet, redirect, useLoaderData, useNavigate } from "react-router";
import { FormAlert, SubmitLabel } from "@/components/site/Form";
import { Logo } from "@/components/site/Logo";
import { SkipLink } from "@/components/site/Nav";
import { LangToggle, ThemeToggle } from "@/components/site/Prefs";
import { Button } from "@/components/ui/button";
import { errorText, logout, session, type User } from "@/lib/api";
import { usePrefs } from "@/lib/prefs";
import { isMock } from "@/lib/utils";

export async function loader({ request }: { request: Request }) {
  let user: User | null = null;
  try {
    user = await session(request.signal);
  } catch {
    user = null;
  }
  if (!user) {
    const u = new URL(request.url);
    throw redirect(`/login?next=${encodeURIComponent(u.pathname + u.search)}`);
  }
  return { user };
}

export type AppContext = { user: User };

export default function AppLayout() {
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
      <header className="sticky top-0 z-30 border-b border-rule bg-bg/90 backdrop-blur-md supports-[backdrop-filter]:bg-bg/75">
        <nav aria-label={t.app.nav.main} className="mx-auto flex h-14 max-w-[76rem] items-center gap-2 px-4 sm:h-16 sm:px-6 lg:px-8">
          <Link to="/app" aria-label={t.app.nav.projects} className="-ml-1 rounded-md px-1 py-1">
            <Logo />
          </Link>
          {isMock() ? (
            <span className="ml-1 hidden rounded-full border border-dashed border-saffron px-2 py-0.5 font-mono text-[0.65rem] text-saffron-ink sm:inline">
              {t.app.mockBadge}
            </span>
          ) : null}
          <span className="mr-auto" />
          <span className="hidden max-w-[14rem] truncate text-sm text-fg-muted md:inline" title={user.email}>
            {user.name}
          </span>
          <LangToggle />
          <ThemeToggle />
          <Button
            variant="outline"
            size="sm"
            className="ml-1 px-2.5 sm:px-3"
            onClick={onLogout}
            disabled={pending}
            aria-label={t.app.logout}
          >
            {pending ? null : <LogOut aria-hidden="true" />}
            <span className="hidden sm:inline">
              <SubmitLabel pending={pending} idle={t.app.logout} busy={t.app.loggingOut} />
            </span>
          </Button>
        </nav>
      </header>
      {error ? (
        <div className="mx-auto w-full max-w-[76rem] px-4 pt-4 sm:px-6 lg:px-8">
          <FormAlert title={t.auth.errors.summary} message={error} />
        </div>
      ) : null}
      <main id="main" tabIndex={-1} className="flex flex-1 flex-col outline-none">
        <Outlet context={{ user } satisfies AppContext} />
      </main>
    </div>
  );
}
