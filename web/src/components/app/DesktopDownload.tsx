import { Download, MonitorDown } from "lucide-react";
import { useEffect, useState } from "react";
import { usePrefs } from "@/lib/prefs";

type DesktopFile = {
  os: "windows" | "linux" | "macos";
  kind: "nsis" | "deb" | "appimage" | "dmg" | "app";
  /** Its own version: macOS builds are published apart from the others. */
  version?: string;
  name: string;
  size: number;
  sha256: string;
};
type Latest = { version: string; files: DesktopFile[] };

/** The hub serves Davra Desktop under /desktop when its operator built it (scripts/build-desktop.sh). */
async function loadLatest(signal: AbortSignal): Promise<Latest | null> {
  try {
    const r = await fetch("/desktop/latest.json", { signal, headers: { accept: "application/json" } });
    if (!r.ok || !(r.headers.get("content-type") ?? "").includes("json")) return null;
    const j = (await r.json()) as Latest;
    return Array.isArray(j.files) && j.files.length ? j : null;
  } catch {
    return null;
  }
}

const viewerOs = (): DesktopFile["os"] | null =>
  /Windows/i.test(navigator.userAgent)
    ? "windows"
    : /Macintosh|Mac OS X/i.test(navigator.userAgent)
      ? "macos"
      : /Linux|X11/i.test(navigator.userAgent)
        ? "linux"
        : null;

const mb = (n: number) => `${(n / 1_048_576).toFixed(n < 10_485_760 ? 1 : 0)} MB`;

/** "Or use Davra Desktop": the installers for this computer first, the rest after. */
export function DesktopDownload() {
  const { t } = usePrefs();
  const d = t.app.agents.desktop;
  const [latest, setLatest] = useState<Latest | null>(null);
  useEffect(() => {
    const ac = new AbortController();
    void loadLatest(ac.signal).then(setLatest);
    return () => ac.abort();
  }, []);
  if (!latest) return null;
  const os = viewerOs();
  // The .app bundle is only for the updater; people install the .dmg.
  const files = latest.files.filter((f) => f.kind !== "app").sort((a, b) => Number(b.os === os) - Number(a.os === os));
  const label = (f: DesktopFile) =>
    f.kind === "nsis" ? d.windows : f.kind === "deb" ? d.deb : f.kind === "dmg" ? d.mac : d.appimage;
  return (
    <section className="rounded-md border border-rule-strong bg-bg-raised px-4 py-3">
      <p className="flex items-center gap-2 text-sm font-semibold">
        <MonitorDown className="size-4 text-accent" aria-hidden="true" />
        {d.title}
        <span className="font-mono text-xs font-normal text-fg-muted">v{latest.version}</span>
      </p>
      <p className="mt-1 text-sm text-fg-muted">{d.body}</p>
      <ul className="mt-3 flex flex-wrap gap-2">
        {files.map((f) => (
          <li key={f.name}>
            <a
              href={`/desktop/${encodeURIComponent(f.name)}?sha=${f.sha256}`}
              download
              title={`SHA-256 ${f.sha256}`}
              className={
                "inline-flex items-center gap-2 rounded-md border px-3 py-1.5 text-sm font-medium focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent " +
                (f.os === os ? "border-accent bg-accent/10" : "border-rule-strong hover:bg-bg")
              }
            >
              <Download className="size-4" aria-hidden="true" />
              {label(f)}
              <span className="font-mono text-xs text-fg-muted">
                {f.version && f.version !== latest.version ? `v${f.version} · ` : ""}
                {mb(f.size)}
              </span>
            </a>
          </li>
        ))}
      </ul>
      <p className="mt-2 text-xs text-fg-muted">
        {d.checksums}{" "}
        <a className="underline underline-offset-2" href="/desktop/SHA256SUMS">
          SHA256SUMS
        </a>
      </p>
    </section>
  );
}
