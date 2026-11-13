import { usePrefs } from "@/lib/prefs";
import { REPO_URL } from "@/lib/utils";
import { GithubMark, LogoMark } from "./Logo";

export function Footer() {
  const { t } = usePrefs();
  const item = "rounded-sm text-fg-muted underline-offset-4 hover:text-fg hover:underline";
  return (
    <footer className="border-t border-rule">
      <div className="mx-auto flex max-w-[76rem] flex-col gap-6 px-4 py-10 text-sm sm:flex-row sm:items-center sm:px-6 lg:px-8">
        <div className="flex items-center gap-3 text-fg">
          <LogoMark className="size-6" />
          <span className="text-fg-muted">{t.footer.license}</span>
        </div>
        <ul className="flex flex-wrap gap-x-6 gap-y-2 sm:ml-auto">
          <li>
            <a className={`${item} inline-flex items-center gap-1.5`} href={REPO_URL} target="_blank" rel="noreferrer">
              <GithubMark className="size-3.5" />
              {t.footer.source}
            </a>
          </li>
          <li>
            <a className={item} href="/llms.txt">
              {t.footer.llms}
            </a>
          </li>
          <li>
            <a className={item} href={`${REPO_URL}/blob/main/SECURITY.md`} target="_blank" rel="noreferrer">
              {t.footer.security}
            </a>
          </li>
        </ul>
      </div>
    </footer>
  );
}
