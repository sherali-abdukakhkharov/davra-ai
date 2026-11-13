import { CircleCheck, Download, RefreshCw } from "lucide-react";
import { useState } from "react";
import { CodeBlock } from "@/components/site/CodeBlock";
import { SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { fill, usePrefs } from "@/lib/prefs";
import { bridge, compareVersions, errorText } from "../bridge";
import { DefRow, InlineError, Mono, PanelHeader, StatusPill } from "../components/bits";
import { useDesktop } from "../state";

export function CliPanel() {
  const { t } = usePrefs();
  const c = t.desktop.cli;
  const { status, setStatus } = useDesktop();
  const cli = status.cli;
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installed, setInstalled] = useState(false);

  const outdated = Boolean(cli.version && compareVersions(cli.version, cli.bundled_version) < 0);
  const healthy = Boolean(cli.path) && !outdated && cli.on_path;
  const label = !cli.path
    ? fill(c.install, { v: cli.bundled_version })
    : outdated
      ? fill(c.update, { v: cli.bundled_version })
      : !cli.on_path
        ? c.fixPath
        : c.reinstall;

  const install = async () => {
    setPending(true);
    setError(null);
    try {
      setStatus(await bridge.installCli());
      setInstalled(true);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setPending(false);
    }
  };

  return (
    <section aria-labelledby="cli-title" className="flex flex-col gap-6">
      <PanelHeader id="cli-title" title={c.title} sub={c.sub} />

      <div className="rounded-lg border border-rule bg-bg-raised">
        <dl className="divide-y divide-rule">
          <DefRow label={c.installed}>
            {cli.path ? <Mono className="text-[0.8rem]">{cli.path}</Mono> : <StatusPill tone="warn">{c.notInstalled}</StatusPill>}
          </DefRow>
          <DefRow label={c.version}>
            <span className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
              {cli.version ? (
                <span translate="no" className="tnum font-mono text-[0.82rem] font-medium">
                  {cli.version}
                </span>
              ) : (
                <span className="text-sm text-fg-muted">—</span>
              )}
              <span className="tnum text-sm text-fg-muted">{fill(c.bundled, { v: cli.bundled_version })}</span>
              {cli.path ? (
                outdated ? (
                  <StatusPill tone="warn">{fill(c.outdated, { v: cli.bundled_version })}</StatusPill>
                ) : (
                  <StatusPill tone="ok">{c.upToDate}</StatusPill>
                )
              ) : null}
            </span>
          </DefRow>
          <DefRow label={c.onPath}>
            {cli.on_path ? <StatusPill tone="ok">{c.yes}</StatusPill> : <StatusPill tone={cli.path ? "warn" : "off"}>{c.no}</StatusPill>}
          </DefRow>
        </dl>
        <div className="flex flex-wrap items-center gap-3 border-t border-rule px-4 py-3">
          <Button size="sm" variant={healthy ? "outline" : "primary"} onClick={() => void install()} disabled={pending}>
            {pending ? null : healthy ? <RefreshCw aria-hidden="true" /> : <Download aria-hidden="true" />}
            <SubmitLabel pending={pending} idle={label} busy={c.installing} />
          </Button>
          {!healthy && !installed ? <p className="text-sm text-fg-muted">{c.newTerminal}</p> : null}
        </div>
      </div>

      {error ? <InlineError title={c.failed} message={error} /> : null}

      {installed && cli.path ? (
        <div role="status" className="flex flex-col gap-3">
          <p className="flex items-start gap-2.5 text-sm">
            <CircleCheck className="mt-0.5 size-4 shrink-0 text-ok" aria-hidden="true" />
            <span>
              {fill(c.done, { v: cli.version ?? cli.bundled_version, path: cli.path })} <strong className="font-medium">{c.newTerminal}</strong>
            </span>
          </p>
          <CodeBlock label={c.check} lines={["davra --version"]} />
        </div>
      ) : null}
    </section>
  );
}
