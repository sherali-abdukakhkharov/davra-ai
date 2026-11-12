import { ArrowLeft } from "lucide-react";
import { Link } from "react-router";
import { Footer } from "@/components/site/Footer";
import { Nav, SkipLink } from "@/components/site/Nav";
import { Button } from "@/components/ui/button";
import { usePrefs } from "@/lib/prefs";

export default function NotFound() {
  const { t } = usePrefs();
  return (
    <div className="flex min-h-dvh flex-col">
      <SkipLink />
      <Nav />
      <main id="main" tabIndex={-1} className="mx-auto flex w-full max-w-[76rem] flex-1 flex-col justify-center px-4 py-24 outline-none sm:px-6 lg:px-8">
        <p className="font-mono text-sm text-fg-muted">404</p>
        <h1 className="display mt-4 max-w-[16ch] text-[clamp(2.2rem,5vw,3.8rem)]">{t.notFound.title}</h1>
        <p className="mt-5 text-lg text-fg-muted">{t.notFound.body}</p>
        <Button asChild variant="outline" className="mt-10 self-start">
          <Link to="/">
            <ArrowLeft aria-hidden="true" />
            {t.notFound.back}
          </Link>
        </Button>
      </main>
      <Footer />
    </div>
  );
}
