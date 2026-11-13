import { FolderGit2, FolderOpen, FolderPlus, ListChecks, Settings2, Trash2, TriangleAlert } from "lucide-react";
import { useId, useRef, useState, type FormEvent } from "react";
import { SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { bridge, errorText, type Repo } from "../bridge";
import { InlineError, Mono, PanelHeader, selectClass, Switch } from "../components/bits";
import { hubHost, useDesktop, useReview } from "../state";

type Draft = { mode: "add" | "edit"; path: string; project: string; usage: boolean };

const basename = (p: string) => p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || p;

export function ReposPanel() {
  const { t } = usePrefs();
  const r = t.desktop.repos;
  const { status } = useDesktop();
  const projects = status.me?.projects ?? [];
  const [draft, setDraft] = useState<Draft | null>(null);
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const addRef = useRef<HTMLButtonElement>(null);

  const defaultProject = (path: string) => {
    const name = basename(path).toLowerCase();
    return projects.find((p) => p.slug === name)?.slug ?? projects[0]?.slug ?? "";
  };

  const pick = async () => {
    setPicking(true);
    setError(null);
    try {
      const path = await bridge.pickFolder();
      if (!path) return;
      const linked = status.repos.find((x) => x.path === path);
      setDraft(
        linked
          ? { mode: "edit", path, project: linked.project, usage: linked.usage }
          : { mode: "add", path, project: defaultProject(path), usage: true },
      );
    } catch (err) {
      setError(errorText(err));
    } finally {
      setPicking(false);
    }
  };

  const closeDraft = () => {
    setDraft(null);
    requestAnimationFrame(() => addRef.current?.focus());
  };

  return (
    <section aria-labelledby="repos-title" className="flex flex-col gap-6">
      <PanelHeader
        id="repos-title"
        title={r.title}
        sub={r.sub}
        action={
          projects.length ? (
            <Button ref={addRef} size="sm" onClick={() => void pick()} disabled={picking}>
              {picking ? null : <FolderPlus aria-hidden="true" />}
              <SubmitLabel pending={picking} idle={r.add} busy={r.picking} />
            </Button>
          ) : null
        }
      />

      {!projects.length ? <p className="text-sm text-fg-muted">{r.noProjects}</p> : null}
      {error ? <InlineError title={r.add} message={error} /> : null}

      {draft ? (
        <RepoForm
          key={draft.path}
          draft={draft}
          onChange={setDraft}
          onCancel={closeDraft}
          onPickAnother={() => void pick()}
          picking={picking}
          onDone={() => setDraft(null)}
        />
      ) : null}

      <div className="flex flex-col gap-2">
        <h2 className="text-sm font-medium text-fg-muted">{r.linked}</h2>
        {status.repos.length ? (
          <ul className="divide-y divide-rule rounded-lg border border-rule bg-bg-raised">
            {status.repos.map((repo) => (
              <li key={repo.path}>
                <RepoRow
                  repo={repo}
                  hub={status.hub}
                  editing={draft?.mode === "edit" && draft.path === repo.path}
                  onEdit={() => setDraft({ mode: "edit", path: repo.path, project: repo.project, usage: repo.usage })}
                />
              </li>
            ))}
          </ul>
        ) : (
          <p className="rounded-lg border border-dashed border-rule-strong px-4 py-6 text-center text-sm text-fg-muted">{r.empty}</p>
        )}
      </div>
    </section>
  );
}

function RepoForm({
  draft,
  onChange,
  onCancel,
  onPickAnother,
  picking,
  onDone,
}: {
  draft: Draft;
  onChange: (d: Draft) => void;
  onCancel: () => void;
  onPickAnother: () => void;
  picking: boolean;
  onDone: () => void;
}) {
  const { t } = usePrefs();
  const r = t.desktop.repos;
  const { status } = useDesktop();
  const openReview = useReview();
  const projects = status.me?.projects ?? [];
  const id = useId();

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const args = { path: draft.path, project: draft.project, usage: draft.usage };
    openReview({
      title: fill(r.reviewTitle, { name: basename(draft.path) }),
      empty: r.alreadyDone,
      plan: () => bridge.planRepo(args),
      apply: () => bridge.applyRepo(args),
      onApplied: onDone,
    });
  };

  return (
    <form
      onSubmit={submit}
      aria-labelledby={`${id}-title`}
      className="rounded-lg border border-rule-strong bg-bg-raised"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onCancel();
        }
      }}
    >
      <div className="flex flex-col gap-5 px-4 py-4">
        <h2 id={`${id}-title`} className="font-semibold">
          {draft.mode === "add" ? r.addTitle : r.editTitle}
        </h2>

        <div className="flex flex-col gap-1.5">
          <span className="text-sm font-medium">{r.folder}</span>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <span className="inline-flex min-w-0 items-center gap-2 rounded-md bg-bg-sunk px-2.5 py-1.5">
              <FolderOpen className="size-4 shrink-0 text-accent" aria-hidden="true" />
              <Mono className="text-[0.8rem]">{draft.path}</Mono>
            </span>
            {draft.mode === "add" ? (
              <Button type="button" variant="ghost" size="sm" onClick={onPickAnother} disabled={picking}>
                {r.another}
              </Button>
            ) : null}
          </div>
        </div>

        <div className="flex max-w-[22rem] flex-col gap-1.5">
          <label htmlFor={`${id}-project`} className="text-sm font-medium">
            {r.project}
          </label>
          <select
            id={`${id}-project`}
            className={cn(selectClass, "font-mono text-[0.85rem]")}
            value={draft.project}
            onChange={(e) => onChange({ ...draft, project: e.target.value })}
            autoFocus
          >
            {projects.map((p) => (
              <option key={p.slug} value={p.slug}>
                {status.me?.org}/{p.slug} · {p.role}
              </option>
            ))}
          </select>
        </div>

        <div className="flex items-start gap-3">
          <Switch
            id={`${id}-usage`}
            checked={draft.usage}
            onChange={(usage) => onChange({ ...draft, usage })}
            labelledBy={`${id}-usage-label`}
            describedBy={`${id}-usage-hint`}
          />
          <div className="min-w-0">
            <label id={`${id}-usage-label`} htmlFor={`${id}-usage`} className="cursor-pointer text-sm font-medium">
              {r.usage}
            </label>
            <p id={`${id}-usage-hint`} className="mt-0.5 text-sm text-fg-muted">
              {r.usagePrivacy}
            </p>
          </div>
        </div>
      </div>
      <div className="flex flex-wrap gap-2 border-t border-rule px-4 py-3">
        <Button type="submit" size="sm" disabled={!draft.project}>
          <ListChecks aria-hidden="true" />
          {r.review}
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          {r.cancel}
        </Button>
      </div>
    </form>
  );
}

function RepoRow({ repo, hub, editing, onEdit }: { repo: Repo; hub: string | null; editing: boolean; onEdit: () => void }) {
  const { t } = usePrefs();
  const r = t.desktop.repos;
  const { setStatus } = useDesktop();
  const [confirming, setConfirming] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const forgetRef = useRef<HTMLButtonElement>(null);
  const otherHub = repo.hub && hub && repo.hub.replace(/\/+$/, "") !== hub.replace(/\/+$/, "");

  const forget = async () => {
    setPending(true);
    setError(null);
    try {
      setStatus(await bridge.forgetRepo(repo.path));
    } catch (err) {
      setError(errorText(err));
      setPending(false);
    }
  };

  return (
    <div className={cn("flex flex-col gap-2 px-4 py-3", editing && "bg-bg-sunk/50")}>
      <div className="flex flex-wrap items-start gap-x-4 gap-y-2">
        <FolderGit2 className="mt-0.5 size-4 shrink-0 text-fg-muted" aria-hidden="true" />
        <div className="min-w-0 flex-1 basis-[16rem]">
          <p className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span translate="no" className="font-medium">
              {basename(repo.path)}
            </span>
            <span translate="no" className="rounded-[4px] bg-bg-sunk px-1.5 py-px font-mono text-[0.7rem]">
              {repo.project}
            </span>
            <span
              className={cn(
                "rounded-[4px] border px-1.5 py-px font-mono text-[0.65rem]",
                repo.usage ? "border-accent/50 text-accent" : "border-rule-strong text-fg-muted",
              )}
            >
              {repo.usage ? r.usageOn : r.usageOff}
            </span>
            {!repo.has_file ? (
              <span className="inline-flex items-center gap-1 rounded-[4px] border border-saffron bg-saffron/15 px-1.5 py-px text-[0.7rem] text-fg">
                <TriangleAlert className="size-3 text-saffron-ink" aria-hidden="true" />
                {r.missing}
              </span>
            ) : null}
            {otherHub ? <span className="text-[0.72rem] text-fg-muted">{fill(r.otherHub, { hub: hubHost(repo.hub!) })}</span> : null}
          </p>
          <Mono className="mt-0.5 block text-fg-muted">{repo.path}</Mono>
          {!repo.has_file ? <p className="mt-1 text-[0.8rem] text-fg-muted">{r.missingHint}</p> : null}
        </div>
        {confirming ? null : (
          <div className="flex gap-1.5">
            <Button size="sm" variant={repo.has_file ? "ghost" : "outline"} onClick={onEdit} aria-label={`${r.edit} ${basename(repo.path)}`}>
              <Settings2 aria-hidden="true" />
              <span className="hidden sm:inline">{r.edit}</span>
            </Button>
            <Button ref={forgetRef} size="sm" variant="ghost" onClick={() => setConfirming(true)} aria-label={`${r.forget} ${basename(repo.path)}`}>
              <Trash2 aria-hidden="true" />
              <span className="hidden sm:inline">{r.forget}</span>
            </Button>
          </div>
        )}
      </div>
      {confirming ? (
        <div role="group" aria-label={r.forgetConfirm} className="flex flex-wrap items-center gap-x-3 gap-y-2 pl-8">
          <p className="text-sm">{r.forgetConfirm}</p>
          <div className="flex gap-1.5">
            <Button size="sm" variant="outline" className="border-danger/60 text-danger hover:bg-danger-soft" onClick={() => void forget()} disabled={pending} autoFocus>
              <SubmitLabel pending={pending} idle={r.forgetYes} busy={r.forgetYes} />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={pending}
              onClick={() => {
                setConfirming(false);
                requestAnimationFrame(() => forgetRef.current?.focus());
              }}
            >
              {r.keep}
            </Button>
          </div>
        </div>
      ) : null}
      {error ? <InlineError title={r.forget} message={error} className="ml-8" /> : null}
    </div>
  );
}
