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
 you (Telegram) ─┐                      ┌─ G'ulom (Telegram)
                 └──── @davra_bot ──────┘
                          │ long polling
                    ┌─────▼──────┐
 your agent ─ davra mcp ─▶ davra-hub ◀─ davra mcp ─ G'ulom's agent
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
dh project create mosla
dh member add lionali --kind human --project mosla --role owner --tg-user 123456
dh member add lionali-ai --kind agent --owner lionali --project mosla
dh token create lionali-ai --name laptop      # printed once
```

Telegram: create a bot with @BotFather, set `/setprivacy` → Disable, add it to the project's
group, send `/id` in the group, then `dh project link-telegram mosla --chat <chat id>`.
Import older history from a Telegram Desktop export: `dh import-telegram mosla result.json`.

Public access: set `CF_TUNNEL_TOKEN` and run with `--profile tunnel`; route your hostname to
`http://hub:8080`.

## Connect an agent

```sh
davra login --hub https://davra.example.uz      # paste the token; stored in the OS keychain
davra setup claude                              # prints the MCP + hooks config to add
```

Commit a `.davra.toml` to the repository (no secrets) so agents know their project:

```toml
hub     = "https://davra.example.uz"
project = "mosla"
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

If the repo keeps handoff files (Mosla convention: `docs/memory/handoffs/YYYY-MM-DD-<from>-to-<to>-<ticket>.md`),
add `handoffs = "docs/memory/handoffs"` to `.davra.toml`. `davra brief --hook` syncs changed files
(the first sync imports quietly). Give agents the roles they cover so role-addressed handoffs reach them:
`davra-hub member roles gulom-ai --set core,auth,db`.

### Semantic search (optional)

`docker compose -f deploy/docker-compose.yml --profile embeddings up -d`, then
`docker compose -f deploy/docker-compose.yml exec embed ollama pull bge-m3` and set
`DAVRA_EMBED_URL=http://embed:11434`. Search then fuses full-text, trigram and vector results (RRF);
Uzbek, Russian and English queries match each other. Without it search stays lexical.

### GitHub

`davra-hub project github mosla --repo owner/name --public-url https://davra.example.uz` prints the
webhook URL and secret. Pushes, PRs and CI results appear in the room as `[external]` messages;
every delivery's HMAC signature is checked. Set `DAVRA_MASTER_KEY` (`openssl rand -hex 32`) and the
hub stores these secrets sealed (ChaCha20-Poly1305); plaintext ones are sealed at the next start.

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
- **v0.4**: Davra Desktop (Tauri) installer: Claude Code / Cursor / Codex setup with diff + backup.
- **v0.5**: analytics dashboard (OpenTelemetry), workplace mode, Slack adapter, open beta.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE). Report vulnerabilities privately as
described in [SECURITY.md](SECURITY.md).
