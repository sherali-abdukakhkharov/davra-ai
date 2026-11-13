// In-memory stand-in for the Tauri commands, for `npm run dev:desktop` in a plain browser.
// Scenarios: ?mock=fresh (first run, default), ?mock=error (hub set, token revoked),
// ?mock=offline (hub unreachable), ?mock=ready (signed in, CLI outdated, one client connected).
// State survives reloads in sessionStorage; ?mock=<scenario> starts over.

import type { Applied, Backend, Change, ClientId, Commands, Plan, RepoArgs, Status } from "./bridge";

const BUNDLED = "0.2.0";
const APP_VERSION = "0.2.0";
const KEY = "davra-desktop-mock";

const windows = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);
const HOME = windows ? "C:\\Users\\frontend" : "/home/frontend";
const SEP = windows ? "\\" : "/";
const join = (...parts: string[]) => parts.join(SEP);
const fwd = (p: string) => p.replace(/\\/g, "/");
const CLI_PATH = join(HOME, ".davra", "bin", windows ? "davra.exe" : "davra");
const FOLDERS = windows
  ? ["D:\\Dev\\ai-team", "D:\\Dev\\digital-edo-api", "D:\\Dev\\point-admin", "D:\\Dev\\atlas"]
  : ["/home/frontend/dev/ai-team", "/home/frontend/dev/digital-edo-api", "/home/frontend/dev/point-admin", "/home/frontend/dev/atlas"];

type Parts = { mcp: boolean; hooks: boolean };
type State = {
  hub: string | null;
  token: string | null;
  revoked: boolean;
  offline: boolean;
  cli: { installed: string | null; on_path: boolean };
  clients: Record<ClientId, { detected: boolean } & Parts>;
  repos: { path: string; project: string; usage: boolean; has_file: boolean }[];
  picks: number;
};

function scenario(name: string): State {
  const base: State = {
    hub: null,
    token: null,
    revoked: false,
    offline: false,
    cli: { installed: null, on_path: false },
    clients: {
      claude: { detected: true, mcp: false, hooks: false },
      cursor: { detected: true, mcp: false, hooks: false },
      codex: { detected: false, mcp: false, hooks: false },
    },
    repos: [],
    picks: 0,
  };
  if (name === "error" || name === "offline") {
    return { ...base, hub: "https://davra.qitmir.uz", token: "dvr_live", revoked: name === "error", offline: name === "offline" };
  }
  if (name === "ready") {
    return {
      ...base,
      hub: "https://davra.qitmir.uz",
      token: "dvr_live",
      cli: { installed: "0.1.4", on_path: true },
      clients: {
        claude: { detected: true, mcp: true, hooks: false },
        cursor: { detected: true, mcp: true, hooks: false },
        codex: { detected: false, mcp: false, hooks: false },
      },
      repos: [
        { path: FOLDERS[0]!, project: "ai-team", usage: true, has_file: true },
        { path: FOLDERS[3]!, project: "atlas", usage: false, has_file: false },
      ],
      picks: 1,
    };
  }
  return base;
}

function load(): State {
  const flag = new URLSearchParams(location.search).get("mock");
  try {
    if (flag) {
      const s = scenario(flag);
      sessionStorage.setItem(KEY, JSON.stringify(s));
      // Drop the flag so a reload keeps the clicked-through state.
      const url = new URL(location.href);
      url.searchParams.delete("mock");
      history.replaceState(null, "", url);
      return s;
    }
    const raw = sessionStorage.getItem(KEY);
    if (raw) return JSON.parse(raw) as State;
  } catch {
    // Storage blocked: fall through to a fresh state.
  }
  return scenario("fresh");
}

const NAMES: Record<ClientId, string> = { claude: "Claude Code", cursor: "Cursor", codex: "Codex" };

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const stamp = () => {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
};

export function createMock(): Backend {
  let s = load();
  const save = () => {
    try {
      sessionStorage.setItem(KEY, JSON.stringify(s));
    } catch {
      // Not persisted; fine for a mock.
    }
  };

  const me = (): Status["me"] =>
    s.token && !s.revoked && !s.offline
      ? {
          handle: "frontend-ai",
          kind: "agent",
          org: "qitmir",
          owner: "frontend",
          projects: [
            { slug: "ai-team", role: "member" },
            { slug: "atlas", role: "member" },
            { slug: "point", role: "viewer" },
          ],
        }
      : null;

  const status = (): Status => {
    const m = me();
    return {
      version: APP_VERSION,
      hub: s.hub,
      me: m,
      me_error:
        s.hub && s.token && !m
          ? s.offline
            ? `Can’t reach ${s.hub}: connection timed out after 10 s.`
            : "The hub rejected the stored token (401 Unauthorized). It may have been revoked."
          : null,
      cli: {
        path: s.cli.installed ? CLI_PATH : null,
        version: s.cli.installed,
        bundled_version: BUNDLED,
        on_path: s.cli.installed ? s.cli.on_path : false,
      },
      clients: (Object.keys(NAMES) as ClientId[]).map((id) => {
        const c = s.clients[id];
        const full = id === "claude" ? c.mcp && c.hooks : c.mcp;
        const partial = id === "claude" && c.mcp !== c.hooks;
        return {
          id,
          name: NAMES[id],
          detected: c.detected,
          connected: full,
          partial,
          detail: detail(id, c),
        };
      }),
      repos: s.repos.map((r) => ({ ...r, hub: r.has_file ? s.hub : null })),
    };
  };

  const detail = (id: ClientId, c: State["clients"][ClientId]): string => {
    if (id === "claude") {
      if (c.mcp && c.hooks) return "MCP server “davra” (user scope) · 3 hooks in ~/.claude/settings.json";
      if (c.mcp) return "MCP server registered · hooks missing from ~/.claude/settings.json";
      if (c.hooks) return "Hooks installed · MCP server not registered";
      return c.detected ? "claude 2.4.1 found on PATH" : "claude not found on PATH";
    }
    if (id === "cursor") {
      if (c.mcp) return "MCP server “davra” in ~/.cursor/mcp.json";
      return c.detected ? "~/.cursor found" : "~/.cursor not found";
    }
    if (c.mcp) return "MCP server “davra” in ~/.codex/config.toml";
    return c.detected ? "~/.codex found" : "codex not found on PATH and no ~/.codex folder";
  };

  const requireMe = () => {
    if (!me()) throw "Not signed in to a hub.";
  };

  const exe = () => fwd(s.cli.installed ? CLI_PATH : join(HOME, "AppData", "Local", "Davra Desktop", "davra.exe"));

  const hooksDiff = (add: boolean) => {
    const sign = add ? "+" : "-";
    const e = exe();
    return [
      `--- ${fwd(join(HOME, ".claude", "settings.json"))}`,
      `+++ ${fwd(join(HOME, ".claude", "settings.json"))}`,
      "@@ -1,6 +1,23 @@",
      " {",
      '   "model": "opus",',
      `   "permissions": { "defaultMode": "acceptEdits" },`,
      add ? '-  "hooks": {}' : '+  "hooks": {}',
      `${sign}  "hooks": {`,
      `${sign}    "SessionStart": [`,
      `${sign}      { "hooks": [{ "type": "command", "command": "${e} brief --hook", "timeout": 20 }] }`,
      `${sign}    ],`,
      `${sign}    "UserPromptSubmit": [`,
      `${sign}      { "hooks": [{ "type": "command", "command": "${e} inbox --hook", "timeout": 10 }] }`,
      `${sign}    ],`,
      `${sign}    "Stop": [`,
      `${sign}      { "hooks": [{ "type": "command", "command": "${e} inbox --hook --stop", "timeout": 10 }] }`,
      `${sign}    ]`,
      `${sign}  }`,
      " }",
    ].join("\n");
  };

  const planConnect = (id: ClientId): Plan => {
    requireMe();
    const c = s.clients[id];
    const changes: Change[] = [];
    const notes: string[] = [];
    if (id === "claude") {
      if (!c.mcp)
        changes.push({
          kind: "command",
          title: "Register the davra MCP server for all projects",
          path: null,
          diff: `claude mcp add --scope user davra -- ${exe()} mcp`,
          new_file: false,
        });
      if (!c.hooks)
        changes.push({
          kind: "file",
          title: "Add the session brief, inbox and live-message hooks",
          path: join(HOME, ".claude", "settings.json"),
          diff: hooksDiff(true),
          new_file: false,
        });
      if (changes.length) notes.push("Hooks only speak inside repositories that have a .davra.toml.", "Start a new Claude Code session to load them.");
    } else if (id === "cursor" && !c.mcp) {
      const p = join(HOME, ".cursor", "mcp.json");
      changes.push({
        kind: "file",
        title: "Add the davra MCP server",
        path: p,
        diff: [
          `--- /dev/null`,
          `+++ ${fwd(p)}`,
          "@@ -0,0 +1,8 @@",
          "+{",
          '+  "mcpServers": {',
          '+    "davra": {',
          `+      "command": "${exe()}",`,
          '+      "args": ["mcp"]',
          "+    }",
          "+  }",
          "+}",
        ].join("\n"),
        new_file: true,
      });
      notes.push("Reload Cursor’s MCP servers (Settings → MCP) to pick it up.");
    } else if (id === "codex" && !c.mcp) {
      const p = join(HOME, ".codex", "config.toml");
      changes.push({
        kind: "file",
        title: "Add the davra MCP server",
        path: p,
        diff: [
          `--- ${fwd(p)}`,
          `+++ ${fwd(p)}`,
          "@@ -3,3 +3,7 @@",
          ' model = "gpt-5-codex"',
          ' approval_policy = "on-request"',
          " ",
          "+[mcp_servers.davra]",
          `+command = "${exe()}"`,
          '+args = ["mcp"]',
          "+",
        ].join("\n"),
        new_file: false,
      });
      if (!c.detected) notes.push("Codex wasn’t found on this machine. The config is written anyway and takes effect once Codex is installed.");
    }
    if (changes.length && !s.cli.installed) notes.push("The davra CLI isn’t installed yet; the bundled copy is used until you install it.");
    return { changes, notes };
  };

  const planDisconnect = (id: ClientId): Plan => {
    const c = s.clients[id];
    const changes: Change[] = [];
    if (id === "claude") {
      if (c.mcp) changes.push({ kind: "command", title: "Remove the davra MCP server", path: null, diff: "claude mcp remove --scope user davra", new_file: false });
      if (c.hooks)
        changes.push({ kind: "file", title: "Remove the Davra hooks", path: join(HOME, ".claude", "settings.json"), diff: hooksDiff(false), new_file: false });
    } else if (c.mcp) {
      const p = id === "cursor" ? join(HOME, ".cursor", "mcp.json") : join(HOME, ".codex", "config.toml");
      const diff =
        id === "cursor"
          ? [`--- ${fwd(p)}`, `+++ ${fwd(p)}`, "@@ -1,8 +1,3 @@", " {", '-  "mcpServers": {', '-    "davra": {', `-      "command": "${exe()}",`, '-      "args": ["mcp"]', "-    }", "-  }", '+  "mcpServers": {}', " }"]
          : [`--- ${fwd(p)}`, `+++ ${fwd(p)}`, "@@ -3,7 +3,3 @@", ' model = "gpt-5-codex"', ' approval_policy = "on-request"', " ", "-[mcp_servers.davra]", `-command = "${exe()}"`, '-args = ["mcp"]', "-"];
      changes.push({ kind: "file", title: "Remove the davra MCP server", path: p, diff: diff.join("\n"), new_file: false });
    }
    return { changes, notes: changes.length ? ["Your token stays in the keychain; log out to remove it."] : [] };
  };

  const planRepo = ({ path, project, usage }: RepoArgs): Plan => {
    requireMe();
    if (!me()!.projects.some((p) => p.slug === project)) throw `You aren’t a member of “${project}” on this hub.`;
    const existing = s.repos.find((r) => r.path === path);
    const changes: Change[] = [];
    const notes: string[] = [];
    const toml = join(path, ".davra.toml");
    if (!existing?.has_file) {
      changes.push({
        kind: "file",
        title: "Link the repository to the project",
        path: toml,
        diff: [`--- /dev/null`, `+++ ${fwd(toml)}`, "@@ -0,0 +1,3 @@", `+hub     = "${s.hub}"`, `+org     = "qitmir"`, `+project = "${project}"`].join("\n"),
        new_file: true,
      });
      notes.push("Commit .davra.toml so teammates’ agents land in the same room. It holds no secrets.");
    } else if (existing.project !== project) {
      changes.push({
        kind: "file",
        title: "Point the repository at another project",
        path: toml,
        diff: [`--- ${fwd(toml)}`, `+++ ${fwd(toml)}`, "@@ -1,3 +1,3 @@", ` hub     = "${s.hub}"`, ` org     = "qitmir"`, `-project = "${existing.project}"`, `+project = "${project}"`].join("\n"),
        new_file: false,
      });
    }
    const settings = join(path, ".claude", "settings.local.json");
    const had = existing?.usage ?? false;
    const helper = `${exe()} otel-headers --hub ${s.hub} --project ${project}`;
    if (usage && (!had || existing?.project !== project)) {
      changes.push({
        kind: "file",
        title: "Report Claude Code usage to the hub",
        path: settings,
        diff: [
          `--- /dev/null`,
          `+++ ${fwd(settings)}`,
          "@@ -0,0 +1,11 @@",
          "+{",
          '+  "env": {',
          '+    "CLAUDE_CODE_ENABLE_TELEMETRY": "1",',
          '+    "OTEL_METRICS_EXPORTER": "otlp",',
          '+    "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL": "http/json",',
          '+    "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE": "delta",',
          '+    "OTEL_METRICS_INCLUDE_ACCOUNT_UUID": "false",',
          `+    "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT": "${s.hub}/otlp/v1/metrics"`,
          "+  },",
          `+  "otelHeadersHelper": "${helper}"`,
          "+}",
        ].join("\n"),
        new_file: true,
      });
      notes.push("settings.local.json is personal and git-ignored by Claude Code. Start a new Claude Code session for usage to show up.");
    } else if (!usage && had) {
      changes.push({
        kind: "file",
        title: "Stop reporting Claude Code usage",
        path: settings,
        diff: [`--- ${fwd(settings)}`, `+++ /dev/null`, "@@ -1,11 +0,0 @@", "-{", '-  "env": {', '-    "CLAUDE_CODE_ENABLE_TELEMETRY": "1",', "-    …", "-  },", `-  "otelHeadersHelper": "${helper}"`, "-}"].join("\n"),
        new_file: false,
      });
    }
    return { changes, notes: changes.length ? notes : [] };
  };

  const backupsFor = (plan: Plan) =>
    plan.changes
      .filter((c) => c.kind === "file" && !c.new_file && c.path)
      .map((c) => join(HOME, ".davra", "backups", `${stamp()}-${c.path!.split(/[\\/]/).pop()}`));

  const handlers: { [K in keyof Commands]: (args: Commands[K]["args"]) => Commands[K]["out"] } = {
    status: () => status(),
    login: ({ hub, token }) => {
      const h = hub.trim().replace(/\/+$/, "");
      if (!/^https?:\/\/[^\s/]+/.test(h)) throw "Enter the hub’s full address, like https://davra.example.com.";
      if (h.includes("offline")) throw `Can’t reach ${h}: no such host.`;
      if (!token.startsWith("dvr_") || token.length < 12) throw "The hub rejected this token (401 Unauthorized). Create a new one in the web app.";
      s.hub = h;
      s.token = token;
      s.revoked = false;
      s.offline = false;
      return status();
    },
    logout: () => {
      s.token = null;
      s.revoked = false;
      s.offline = false;
      return status();
    },
    install_cli: () => {
      s.cli = { installed: BUNDLED, on_path: true };
      return status();
    },
    plan_connect: ({ client }) => planConnect(client),
    apply_connect: ({ client }): Applied => {
      const plan = planConnect(client);
      const backups = backupsFor(plan);
      s.clients[client] = { ...s.clients[client], mcp: true, hooks: client === "claude" };
      return { backups, status: status() };
    },
    plan_disconnect: ({ client }) => planDisconnect(client),
    apply_disconnect: ({ client }): Applied => {
      const plan = planDisconnect(client);
      const backups = backupsFor(plan);
      s.clients[client] = { ...s.clients[client], mcp: false, hooks: false };
      return { backups, status: status() };
    },
    plan_repo: (args) => planRepo(args),
    apply_repo: (args): Applied => {
      const plan = planRepo(args);
      const backups = backupsFor(plan);
      const next = { path: args.path, project: args.project, usage: args.usage, has_file: true };
      const i = s.repos.findIndex((r) => r.path === args.path);
      if (i >= 0) s.repos[i] = next;
      else s.repos.push(next);
      return { backups, status: status() };
    },
    forget_repo: ({ path }) => {
      s.repos = s.repos.filter((r) => r.path !== path);
      return status();
    },
  };

  const DELAY: Partial<Record<keyof Commands, number>> = { login: 700, install_cli: 900, status: 250 };

  return {
    async call(cmd, args) {
      await sleep(DELAY[cmd] ?? 350);
      // Handlers throw strings, like the Rust side rejects with a string.
      const out = (handlers[cmd] as (a: typeof args) => unknown)(args);
      save();
      return structuredClone(out) as Commands[typeof cmd]["out"];
    },
    async pickFolder() {
      await sleep(200);
      const linked = new Set(s.repos.map((r) => r.path));
      const free = FOLDERS.filter((f) => !linked.has(f));
      if (!free.length) return null;
      const pick = free[s.picks % free.length]!;
      s.picks++;
      save();
      return pick;
    },
    async openUrl(url) {
      window.open(url, "_blank", "noopener,noreferrer");
    },
  };
}
