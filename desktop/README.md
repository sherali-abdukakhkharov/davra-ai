# Davra Desktop

A small Tauri 2 app that connects this machine to a Davra hub without a terminal:

- **Connect:** hub URL + an agent token (web app → project → agent → *Create token*). The
  token goes to the OS keychain, the same entry the `davra` CLI uses.
- **CLI:** installs the bundled `davra` into `~/.davra/bin` and puts it on PATH.
- **Clients:** Claude Code (MCP server + hooks), Cursor and Codex (MCP server). Every change is
  shown as a diff first, files are backed up to `~/.davra/backups/`, and *Disconnect* undoes it.
- **Repositories:** writes `.davra.toml` and, optionally, Claude Code usage reporting
  (`.claude/settings.local.json`, kept out of git; counts only).

It is a thin shell: the work is done by the `davra_cli` library (`crates/davra-cli`), the same
code behind `davra setup`. The UI lives in `web/` as a second Vite entry (`web/desktop.html`,
built to `web/dist-desktop`); `npm --prefix web run dev:desktop` runs it in a browser against a
mock bridge (`?mock=fresh|ready|error|offline`).

## Build

Everything builds in Docker, Windows included (cross-compiled with `cargo-xwin` and NSIS 3.13):

```sh
scripts/build-desktop.sh            # linux + windows
scripts/build-desktop.sh windows    # or just one
```

Output in `desktop/dist/`: `linux/Davra_<v>_amd64.deb`, `linux/Davra_<v>_amd64.AppImage`,
`windows/Davra_<v>_x64-setup.exe` (per-user install, no admin), and `windows/app/` (the exe and
its `davra.exe` sidecar, to run without installing). The installers are not code-signed yet, so
Windows SmartScreen asks once.
