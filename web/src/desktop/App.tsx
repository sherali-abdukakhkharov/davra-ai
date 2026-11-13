import { CircleAlert, RotateCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { usePrefs } from "@/lib/prefs";
import { bridge, errorText, type Status } from "./bridge";
import { ConnectScreen } from "./screens/Connect";
import { MainScreen } from "./screens/Main";
import { DesktopContext, needsConnect, type DesktopState } from "./state";

type Boot = { phase: "loading" } | { phase: "error"; message: string } | { phase: "ready" };

export function App() {
  const { t } = usePrefs();
  const [boot, setBoot] = useState<Boot>({ phase: "loading" });
  const [status, setStatus] = useState<Status | null>(null);
  // Holds the connect screen on its success view until the person continues.
  const [welcome, setWelcome] = useState(false);

  const load = useCallback(async () => {
    setBoot({ phase: "loading" });
    try {
      setStatus(await bridge.status());
      setBoot({ phase: "ready" });
    } catch (err) {
      setBoot({ phase: "error", message: errorText(err) });
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const ctx = useMemo<DesktopState | null>(() => (status ? { status, setStatus } : null), [status]);

  if (!ctx) {
    return (
      <div className="flex min-h-dvh items-center justify-center p-6">
        {boot.phase === "error" ? (
          <div role="alert" className="flex max-w-[28rem] flex-col items-start gap-4 rounded-lg border border-danger/40 bg-danger-soft px-5 py-4">
            <div className="flex gap-3">
              <CircleAlert className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden="true" />
              <div>
                <p className="font-medium">{t.desktop.loadError}</p>
                <p className="mt-0.5 text-sm break-words">{boot.message}</p>
              </div>
            </div>
            <Button variant="outline" size="sm" onClick={() => void load()}>
              <RotateCw aria-hidden="true" />
              {t.desktop.retry}
            </Button>
          </div>
        ) : (
          <span role="status" className="inline-flex items-center gap-3 text-fg-muted">
            <span className="size-4 animate-spin rounded-full border-2 border-rule-strong border-t-accent motion-reduce:animate-none" aria-hidden="true" />
            {t.desktop.loading}
          </span>
        )}
      </div>
    );
  }

  return (
    <DesktopContext.Provider value={ctx}>
      {needsConnect(ctx.status) || welcome ? (
        <ConnectScreen welcome={welcome} onConnected={() => setWelcome(true)} onContinue={() => setWelcome(false)} />
      ) : (
        <MainScreen />
      )}
    </DesktopContext.Provider>
  );
}
