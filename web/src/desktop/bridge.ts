// Typed bridge to the Tauri commands in desktop/src-tauri.
// Inside Tauri it calls the real commands; in a plain browser (npm run dev:desktop)
// it serves an in-memory mock so the whole UI can be clicked through.

import { invoke } from "@tauri-apps/api/core";

export type ClientId = "claude" | "cursor" | "codex";

export type Project = { slug: string; role: string };

export type Me = {
  handle: string;
  kind: "agent" | "human";
  org: string;
  owner: string | null;
  projects: Project[];
};

export type Client = {
  id: ClientId;
  name: string;
  detected: boolean;
  connected: boolean;
  partial: boolean;
  detail: string;
};

export type Repo = {
  path: string;
  project: string;
  hub: string | null;
  usage: boolean;
  has_file: boolean;
};

export type Status = {
  version: string;
  hub: string | null;
  me: Me | null;
  me_error: string | null;
  cli: { path: string | null; version: string | null; bundled_version: string; on_path: boolean };
  clients: Client[];
  repos: Repo[];
};

export type Change = {
  kind: "file" | "command";
  title: string;
  path: string | null;
  /** Unified diff for files, the command line for commands. */
  diff: string;
  new_file: boolean;
};

/** An empty `changes` list means there is nothing to do. */
export type Plan = { changes: Change[]; notes: string[] };

export type Applied = { backups: string[]; status: Status };

export type RepoArgs = { path: string; project: string; usage: boolean };

/** Every command the Rust side implements. Errors reject with a string message. */
export type Commands = {
  status: { args: undefined; out: Status };
  login: { args: { hub: string; token: string }; out: Status };
  logout: { args: undefined; out: Status };
  install_cli: { args: undefined; out: Status };
  plan_connect: { args: { client: ClientId }; out: Plan };
  apply_connect: { args: { client: ClientId }; out: Applied };
  plan_disconnect: { args: { client: ClientId }; out: Plan };
  apply_disconnect: { args: { client: ClientId }; out: Applied };
  plan_repo: { args: RepoArgs; out: Plan };
  apply_repo: { args: RepoArgs; out: Applied };
  forget_repo: { args: { path: string }; out: Status };
};

export type CommandName = keyof Commands;

/** What the mock module implements, so it stays in step with the contract. */
export type Backend = {
  call<K extends CommandName>(cmd: K, args: Commands[K]["args"]): Promise<Commands[K]["out"]>;
  pickFolder(): Promise<string | null>;
  openUrl(url: string): Promise<void>;
};

export const inTauri: boolean = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const tauri: Backend = {
  call: (cmd, args) => invoke(cmd, args ?? {}),
  async pickFolder() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false });
    return typeof picked === "string" ? picked : null;
  },
  async openUrl(url) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  },
};

let mock: Promise<Backend> | null = null;
function backend(): Promise<Backend> {
  if (inTauri) return Promise.resolve(tauri);
  mock ??= import("./mock").then((m) => m.createMock());
  return mock;
}

async function call<K extends CommandName>(cmd: K, args: Commands[K]["args"]): Promise<Commands[K]["out"]> {
  const b = await backend();
  return b.call(cmd, args);
}

export const bridge = {
  status: () => call("status", undefined),
  login: (hub: string, token: string) => call("login", { hub, token }),
  logout: () => call("logout", undefined),
  installCli: () => call("install_cli", undefined),
  planConnect: (client: ClientId) => call("plan_connect", { client }),
  applyConnect: (client: ClientId) => call("apply_connect", { client }),
  planDisconnect: (client: ClientId) => call("plan_disconnect", { client }),
  applyDisconnect: (client: ClientId) => call("apply_disconnect", { client }),
  planRepo: (args: RepoArgs) => call("plan_repo", args),
  applyRepo: (args: RepoArgs) => call("apply_repo", args),
  forgetRepo: (path: string) => call("forget_repo", { path }),
  pickFolder: async () => (await backend()).pickFolder(),
  openUrl: async (url: string) => (await backend()).openUrl(url),
};

/** Commands reject with a plain string; anything else is turned into one. */
export function errorText(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (err && typeof err === "object" && "message" in err && typeof err.message === "string") return err.message;
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

/** Compares dotted versions (1.10.0 > 1.9.2). Pre-release tags are ignored. */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) => v.replace(/^v/, "").split(/[.+-]/).slice(0, 3).map((x) => Number.parseInt(x, 10) || 0);
  const pa = parse(a);
  const pb = parse(b);
  for (let i = 0; i < 3; i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d;
  }
  return 0;
}
