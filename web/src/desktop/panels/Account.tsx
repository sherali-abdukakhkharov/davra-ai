import { ExternalLink, KeyRound, LogOut } from "lucide-react";
import { useRef, useState } from "react";
import { RoleBadge } from "@/components/app/States";
import { SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { fill, usePrefs } from "@/lib/prefs";
import { bridge, errorText } from "../bridge";
import { DefRow, InlineError, PanelHeader } from "../components/bits";
import { useDesktop } from "../state";

export function AccountPanel() {
  const { t } = usePrefs();
  const a = t.desktop.account;
  const { status, setStatus } = useDesktop();
  const me = status.me!;
  const hub = status.hub ?? "";
  const [confirming, setConfirming] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const logoutRef = useRef<HTMLButtonElement>(null);

  const logout = async () => {
    setPending(true);
    setError(null);
    try {
      // The app switches to the connect screen once `me` is gone.
      setStatus(await bridge.logout());
    } catch (err) {
      setError(errorText(err));
      setPending(false);
    }
  };

  return (
    <section aria-labelledby="account-title" className="flex flex-col gap-6">
      <PanelHeader
        id="account-title"
        title={a.title}
        sub={a.sub}
        action={
          <Button variant="outline" size="sm" onClick={() => void bridge.openUrl(hub)}>
            <ExternalLink aria-hidden="true" />
            {a.openWeb}
          </Button>
        }
      />

      <dl className="divide-y divide-rule rounded-lg border border-rule bg-bg-raised">
        <DefRow label={a.hub}>
          <span translate="no" className="font-mono text-[0.82rem] break-all">
            {hub}
          </span>
        </DefRow>
        <DefRow label={a.handle}>
          <span className="flex flex-wrap items-center gap-2">
            <span translate="no" className="font-mono text-[0.88rem] font-semibold">
              @{me.handle}
            </span>
            <span className="rounded-[4px] border border-rule-strong px-1.5 py-px font-mono text-[0.68rem] text-fg-muted">{a.kind[me.kind]}</span>
          </span>
        </DefRow>
        <DefRow label={a.org}>
          <span translate="no" className="font-mono text-[0.82rem]">
            {me.org}
          </span>
        </DefRow>
        {me.owner ? (
          <DefRow label={a.owner}>
            <span translate="no" className="font-mono text-[0.82rem]">
              @{me.owner}
            </span>
          </DefRow>
        ) : null}
        <DefRow label={a.projects}>
          {me.projects.length ? (
            <ul className="flex flex-wrap gap-x-4 gap-y-1.5">
              {me.projects.map((p) => (
                <li key={p.slug} className="inline-flex items-center gap-1.5">
                  <span translate="no" className="font-mono text-[0.82rem]">
                    {p.slug}
                  </span>
                  <RoleBadge role={p.role} />
                </li>
              ))}
            </ul>
          ) : (
            <span className="text-sm text-fg-muted">{a.noProjects}</span>
          )}
        </DefRow>
        <DefRow label={a.token}>
          <span className="inline-flex items-center gap-1.5 text-sm text-fg-muted">
            <KeyRound className="size-3.5" aria-hidden="true" />
            {a.tokenStored}
          </span>
        </DefRow>
      </dl>

      <div className="flex flex-col gap-3">
        {confirming ? (
          <div role="group" aria-labelledby="logout-q" className="rounded-lg border border-rule-strong bg-bg-raised p-4">
            <p id="logout-q" className="max-w-[40rem] text-sm">
              {a.logoutConfirm}
            </p>
            <div className="mt-3 flex flex-wrap gap-2">
              <Button size="sm" onClick={() => void logout()} disabled={pending} className="bg-danger text-bg hover:bg-danger/90" autoFocus>
                {pending ? null : <LogOut aria-hidden="true" />}
                <SubmitLabel pending={pending} idle={a.logoutYes} busy={a.loggingOut} />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                disabled={pending}
                onClick={() => {
                  setConfirming(false);
                  requestAnimationFrame(() => logoutRef.current?.focus());
                }}
              >
                {a.keep}
              </Button>
            </div>
          </div>
        ) : (
          <div>
            <Button ref={logoutRef} variant="outline" size="sm" onClick={() => setConfirming(true)}>
              <LogOut aria-hidden="true" />
              {a.logout}
            </Button>
          </div>
        )}
        {error ? <InlineError title={a.logout} message={error} /> : null}
        <p className="tnum text-[0.75rem] text-fg-muted">{fill(t.desktop.version, { v: status.version })}</p>
      </div>
    </section>
  );
}
