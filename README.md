# Davra

**A shared room for human + AI teams.** Davra gives each project one room where the
developers and their AI coding agents (Claude Code, Cursor, Codex) talk to each other,
hand work off, and remember everything, without anyone copy-pasting between chats.

- Agents connect via **MCP** (`davra mcp`) and get a ~1k-token brief at session start,
  only the messages addressed to them, and search over the room's full history.
- Humans watch and steer from a **Telegram group** (one per project). The hub is the source of
  truth; Telegram is a window.
- Everything is stored in **Postgres** and searchable (full-text + trigram today, vectors in v0.2).
- Self-host it with one `docker compose`, behind a **Cloudflare Tunnel** (no open ports).

> Status: **v0.2** (handoffs, semantic search, ask_owner, GitHub webhooks) plus the first piece of
> v0.3 (web accounts). Multi-org isolation (RLS), SSO, the dashboard and the desktop installer are on
> the [roadmap](#roadmap). Expect breaking changes before 1.0.

## How it works

```
 you (Telegram) ─┐                      ┌─ Backend (Telegram)
                 └──── @davra_bot ──────┘
                          │ long polling
                    ┌─────▼──────┐
 your agent ─ davra mcp ─▶ davra-hub ◀─ davra mcp ─ Backend's agent
 (token in keychain)  │  Postgres  │   (token in keychain)
                      └────────────┘
```

- **Trust is explicit.** Every message an agent reads is labelled relative to that agent:
  `[owner]`, `[self]`, `[peer_human]`, `[peer_agent]`, `[external]`. Only `owner`/`self` may give
  instructions; everything else is information.
- **Secrets are redacted** (tokens, keys, `password=...`, URL credentials) before anything is stored.
- **Loop guard**: after 4 consecutive agent messages, agents must wait for a human.
- **Tokens never live in config files**: `davra` keeps them in the OS keychain.

## Quick start (self-host)

```sh
cp deploy/.env.example deploy/.env        # set PG_PASSWORD, optionally DAVRA_TELEGRAM_TOKEN
docker compose -f deploy/docker-compose.yml up -d --build

alias dh='docker compose -f deploy/docker-compose.yml exec hub davra-hub'
dh org create myteam
dh project create atlas
dh member add frontend --kind human --project atlas --role owner --tg-user 123456
dh member add frontend-ai --kind agent --owner frontend --project atlas
dh token create frontend-ai --name laptop      # printed once
```

Telegram: create a bot with @BotFather, set `/setprivacy` → Disable, add it to the project's
group, send `/id` in the group, then `dh project link-telegram atlas --chat <chat id>`.
Import older history from a Telegram Desktop export: `dh import-telegram atlas result.json`.

Public access: set `CF_TUNNEL_TOKEN` and run with `--profile tunnel`; route your hostname to
`http://hub:8080`.

## Connect an agent

```sh
davra login --hub https://davra.example.uz      # paste the token; stored in the OS keychain
davra setup claude                              # prints the MCP + hooks config to add
```

`davra setup cursor` and `davra setup codex` add the MCP server to Cursor and Codex. Every setup
lists its changes first (`--dry-run` shows the diffs), backs up each file it touches to
`~/.davra/backups/`, and `--undo` removes it again.

**Davra Desktop** does the same without a terminal: sign in with the token, install the CLI,
connect Claude Code / Cursor / Codex, and link repositories, each change reviewed as a diff
first. Windows (NSIS, per-user) and Linux (.deb, AppImage) builds come from
`scripts/build-desktop.sh`; see [desktop/README.md](desktop/README.md).

Commit a `.davra.toml` to the repository (no secrets) so agents know their project:

```toml
hub     = "https://davra.example.uz"
project = "atlas"
```

The `SessionStart` hook runs `davra brief --hook`; `UserPromptSubmit` runs `davra inbox --hook`,
which adds nothing to the context when there is nothing new.

### MCP tools

| Tool | What it does |
|---|---|
| `projects` | projects you can access, and which one this repo maps to |
| `team_brief` | members, unread messages for you, recent activity (~1k tokens) |
| `inbox` / `ack` | messages addressed to you; mark handled |
| `post` | write to the room (relayed to Telegram), `@handle` to address |
| `history_search` | search the whole history, returns short snippets with ids |
| `get_message` | one message in full, with surrounding context |
| `handoffs` / `get_handoff` | open handoffs for you (by handle or role) |
| `handoff_create` | hand work to an agent or role; writes the repo's handoff file and announces it |
| `handoff_done` | close a handoff; updates the file and tells the sender |
| `ask_owner` | ask your owner to decide; Telegram buttons; the answer comes back as `[owner]` |

### Handoffs and roles

If the repo keeps handoff files (for example `docs/memory/handoffs/YYYY-MM-DD-<from>-to-<to>-<ticket>.md`),
add `handoffs = "docs/memory/handoffs"` to `.davra.toml`. `davra brief --hook` syncs changed files
(the first sync imports quietly). Give agents the roles they cover so role-addressed handoffs reach them:
`davra-hub member roles backend-ai --set core,auth,db`.

### Semantic search (optional)

`docker compose -f deploy/docker-compose.yml --profile embeddings up -d`, then
`docker compose -f deploy/docker-compose.yml exec embed ollama pull bge-m3` and set
`DAVRA_EMBED_URL=http://embed:11434`. Search then fuses full-text, trigram and vector results (RRF);
Uzbek, Russian and English queries match each other. Without it search stays lexical.

### GitHub

`davra-hub project github atlas --repo owner/name --public-url https://davra.example.uz` prints the
webhook URL and secret. Pushes, PRs and CI results appear in the room as `[external]` messages;
every delivery's HMAC signature is checked. Set `DAVRA_MASTER_KEY` (`openssl rand -hex 32`) and the
hub stores these secrets sealed (ChaCha20-Poly1305); plaintext ones are sealed at the next start.

### Telegram cards

The bot writes rich messages (Bot API 10.1+): one card per message, pushes/PRs/passing CI in one
digest message per 15 minutes, a separate loud card for a failed run. `DAVRA_LANG=uz` gives Uzbek
labels. `davra-hub tg-preview --to <telegram user id>` sends samples to a private chat;
`davra-hub project tg-format <slug> plain` keeps a group on plain text.

### Daily digest

Set `DAVRA_ANTHROPIC_API_KEY` and, after `DAVRA_DIGEST_HOUR` (default 19, the database's local
time), Claude Haiku summarizes each project's day (done, in progress, blockers, next) into the room
and its Telegram group. Quiet days are skipped. Try it first with
`davra-hub digest <slug> --preview-to <telegram user id>` (sends only to you, stores nothing).

### Agent usage

Start Claude Code with `davra claude` in a repository that has a `.davra.toml`, and that session
sends its OpenTelemetry metrics (OTLP/HTTP JSON, delta) to `<hub>/otlp/v1/metrics`. `davra claude`
hands the telemetry settings to that one session (`--settings`): Claude Code ignores them in a
repository's own `.claude/settings*.json`, and nothing goes into your global settings, so other
repositories report nothing. The token is in no file: `otelHeadersHelper` runs `davra otel-headers`,
which reads it from the keychain. Only counts are stored, per member, day and model: cost, tokens,
lines added and removed, commits, PRs, edit decisions and active time. E-mail, account and session
attributes are dropped, and no prompts or code are sent. The web app's **Usage** tab shows them.
Turn it off for a repository with `davra setup claude --no-usage` (kept in `~/.davra/config.toml`).

## Development

```sh
cargo test --workspace
# end-to-end against a real Postgres:
DATABASE_URL=postgres://... BIN=target/debug scripts/e2e.sh
# semantic search against a real embedder:
DATABASE_URL=... DAVRA_EMBED_URL=http://localhost:11434 scripts/e2e-semantic.sh
```

Layout: `crates/davra-core` (protocol, formatting, redaction), `crates/davra-hub` (server),
`crates/davra-cli` (the `davra` binary: MCP proxy, hooks, CLI).

## Roadmap

- **v0.2** (done): embeddings + hybrid RRF search, handoff tools and file sync, roles, `ask_owner`,
  GitHub webhooks. Daily digest moves to v0.3.
- **v0.3**: web app (landing, accounts: started), orgs with Postgres RLS, OAuth 2.1 + OIDC SSO,
  device revocation, audit UI, daily digest.
- **v0.4** (first build): Davra Desktop (Tauri) for Windows and Linux: Claude Code / Cursor / Codex
  setup with diff + backup, CLI install, repository linking. Next: code signing, auto-update, tray.
- **v0.5**: analytics dashboard (OpenTelemetry), workplace mode, Slack adapter, open beta.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE). Report vulnerabilities privately as
described in [SECURITY.md](SECURITY.md).
