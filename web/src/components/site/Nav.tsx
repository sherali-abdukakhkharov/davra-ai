import { Link } from "react-router";
import { Button } from "@/components/ui/button";
import { usePrefs } from "@/lib/prefs";
import { cn, DOCS_URL, REPO_URL } from "@/lib/utils";
import { GithubMark, Logo } from "./Logo";
import { LangToggle, ThemeToggle } from "./Prefs";

export function SkipLink() {
  const { t } = usePrefs();
  return (
    <a
      href="#main"
      className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 focus:rounded-md focus:bg-saffron focus:px-4 focus:py-2 focus:text-[oklch(0.22_0.08_268)]"
    >
      {t.nav.skip}
    </a>
  );
}

/** Top navigation. `onField` paints it for the cobalt hero field. */
export function Nav({ onField = false, showLogin = true }: { onField?: boolean; showLogin?: boolean }) {
  const { t } = usePrefs();
  const link = cn(
    "hidden h-9 items-center gap-2 rounded-md px-3 text-sm transition-colors sm:inline-flex",
    onField ? "text-field-muted hover:text-field-fg hover:bg-field-deep" : "text-fg-muted hover:text-fg hover:bg-bg-sunk",
  );
  return (
    <header className={cn("relative z-20", onField ? "text-field-fg" : "border-b border-rule text-fg")}>
      <nav className="mx-auto flex h-16 max-w-[76rem] items-center gap-2 px-4 sm:px-6 lg:px-8" aria-label="Main">
        <Link to="/" aria-label={t.nav.home} className="mr-auto -ml-1 rounded-md px-1 py-1">
          <Logo />
        </Link>
        <a className={link} href={DOCS_URL} target="_blank" rel="noreferrer">
          {t.nav.docs}
        </a>
        <a className={link} href={REPO_URL} target="_blank" rel="noreferrer">
          <GithubMark />
          {t.nav.github}
        </a>
        <span className={cn("mx-1 hidden h-5 w-px sm:block", onField ? "bg-field-rule" : "bg-rule")} aria-hidden="true" />
        <LangToggle onField={onField} />
        <ThemeToggle onField={onField} />
        {showLogin ? (
          <Button asChild size="sm" variant={onField ? "saffron" : "primary"} className="ml-1 px-4">
            <Link to="/login">{t.nav.login}</Link>
          </Button>
        ) : null}
      </nav>
    </header>
  );
}
