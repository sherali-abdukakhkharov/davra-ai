import { ArrowDownToLine, CircleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { fill, usePrefs } from "@/lib/prefs";
import { bridge, errorText, type UpdateInfo } from "../bridge";

const EVERY = 6 * 60 * 60 * 1000;

/**
 * Asks the connected hub for a newer Davra Desktop at start and every six hours.
 * The update is signed; the app verifies it before installing, then restarts.
 * A failed check stays quiet (offline is normal); a failed install is shown.
 */
export function UpdateBanner() {
  const { t } = usePrefs();
  const u = t.desktop.update;
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    const check = () =>
      bridge
        .checkUpdate()
        .then((i) => alive && setInfo(i))
        .catch(() => {});
    void check();
    const id = window.setInterval(check, EVERY);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  if (!info) return null;

  const install = async () => {
    setBusy(true);
    setError(null);
    try {
      await bridge.installUpdate();
    } catch (err) {
      setError(errorText(err));
      setBusy(false);
    }
  };

  return (
    <div role="status" className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-accent/40 bg-accent/10 px-5 py-2.5 text-sm md:px-8">
      <ArrowDownToLine className="size-4 shrink-0 text-accent" aria-hidden="true" />
      <p className="min-w-0 flex-1">
        {fill(u.available, { v: info.version, current: info.current })}
        {error ? (
          <span className="mt-1 flex items-start gap-1.5 text-danger">
            <CircleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
            <span className="break-words">{fill(u.failed, { e: error })}</span>
          </span>
        ) : null}
      </p>
      <Button size="sm" onClick={() => void install()} disabled={busy}>
        {busy ? u.installing : u.install}
      </Button>
    </div>
  );
}
