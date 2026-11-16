# Building Davra for macOS

The hub's Docker image builds the `davra` CLI for Linux and Windows only. The
macOS builds are made on a Mac with `scripts/build-macos.sh`. They are attached
to the GitHub Release `v<version>`, and the hub machine pulls them from there.
This guide is written so an agent (Claude Code on the Mac) can follow it from
start to finish.

The result:

- `davra-macos-aarch64` (Apple Silicon) and `davra-macos-x86_64` (Intel). Once
  the hub serves them, `curl -fsSL https://davra.qitmir.uz/install.sh | sh`
  installs `davra` on a Mac without Rust.
- Optionally, Davra Desktop: a universal `Davra_<version>_universal.dmg`.

## 1. Prerequisites (check each, install what's missing)

| Tool | Check | Install |
|---|---|---|
| Xcode Command Line Tools | `xcode-select -p` | `xcode-select --install` |
| rustup (not Homebrew's `rust`) | `rustup --version` | `brew install rustup && rustup-init -y`, then open a new shell |
| GitHub CLI, signed in with write access to the repository | `gh auth status` | `brew install gh && gh auth login` |
| Node.js and npm (Desktop only) | `npm --version` | `brew install node` |

Homebrew's `rust` builds only for the Mac's own CPU, so it cannot build the
Intel binary on Apple Silicon. If it is installed, `which cargo` must point at
`~/.cargo/bin/cargo` (rustup), not `/opt/homebrew/bin/cargo`. If it doesn't,
put `~/.cargo/bin` first on PATH, or run `brew uninstall rust` after asking the
user. A `davra` that was installed earlier with `cargo install` keeps working
either way.

## 2. Get the code

```sh
git clone https://github.com/sherali-abdukakhkharov/davra-ai.git   # or: cd davra-ai && git pull
cd davra-ai
git checkout main && git pull
grep -m1 '^version' Cargo.toml        # the version that will be built, e.g. 0.2.4
```

Build from `main`, and only from a commit that is pushed. The release is tagged
at that commit.

## 3. Build the CLI

```sh
scripts/build-macos.sh cli
```

This takes a few minutes the first time. The script stops with a clear message
if a check fails. It:

- builds for both targets;
- ensures each binary is signed ad hoc;
- checks the architectures with `lipo`;
- checks that `--version` matches `Cargo.toml`;
- writes `dist-macos/` and `SHA256SUMS-macos`.

Check by hand:

```sh
ls -l dist-macos/
codesign -dv dist-macos/davra-macos-aarch64 2>&1 | grep -i signature   # "Signature=adhoc"
dist-macos/davra-macos-aarch64 --version
```

If the build fails because the code doesn't compile on macOS:

1. Make the smallest fix.
2. Run `cargo test --workspace --locked` and `cargo clippy --workspace --all-targets --locked -- -D warnings`.
3. Commit the fix on a branch named `macos-build-fixes`, using `git commit -s`.
4. Push the branch and tell the user what you changed. Ask the user before opening a pull request.

Never commit build output (`dist-macos/`, `target/`, `desktop/src-tauri/binaries/`).

## 4. Desktop (optional)

```sh
scripts/build-macos.sh cli desktop
```

This builds the web UI (`web/dist-desktop`) and a universal sidecar (`lipo`),
installs `tauri-cli` 2.12.4 if it is missing, and bundles
`Davra_<version>_universal.dmg`.

- **No Developer ID:** the app is signed ad hoc, so macOS asks once before the
  first start. Right-click the app and choose Open, or go to System Settings,
  then Privacy & Security, then Open Anyway.
- **Auto-update:** `Davra_<version>_universal.app.tar.gz` and its `.sig` are
  made only with the updater key. The user copies it by hand from the hub
  maintainer's machine to `~/.davra-release/updater.key` on the Mac
  (`chmod 600`), and the script picks it up from there. Don't search for it
  anywhere else, and never generate a new one: a new key would not match the
  public key built into the app. Never commit, upload or print it. If the file
  isn't there, the script prints "no updater key" and builds the `.dmg` alone,
  which is fine.

Open the `.app` from `desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/`
and check that the window shows the hub connection and the CLI panel.

## 5. Upload

```sh
scripts/build-macos.sh cli --upload            # or: cli desktop --upload
```

This rebuilds, then creates the GitHub Release `v<version>` at the current
commit if it doesn't exist yet. It attaches everything in `dist-macos/` and
prints the release URL. Give the user that URL.

## 6. On the hub machine (not on the Mac)

The hub maintainer runs:

```sh
scripts/fetch-macos-clients.sh v<version>                                    # into deploy/clients/, checksums verified
docker compose -f deploy/docker-compose.yml --profile tunnel-config up -d --build hub
curl -fsS https://davra.qitmir.uz/downloads/SHA256SUMS                       # lists davra-macos-*
```

After that, on the Mac, check the one-line install:

```sh
curl -fsSL https://davra.qitmir.uz/install.sh | sh
~/.davra/bin/davra --version
```

`curl` doesn't mark downloads as quarantined, so Gatekeeper doesn't block
this. A binary downloaded through a browser would need
`xattr -d com.apple.quarantine <file>`.

When the release also has the Desktop files, the fetch script puts them in
`desktop/dist/publish/` and rewrites `latest.json` with `scripts/desktop-manifest.sh`.
Each file there keeps its own version, so a Mac build can come later than the
Windows and Linux ones. The web app then offers the `.dmg`. Apps signed with
the updater key update themselves from `Davra_<version>_universal.app.tar.gz`:
the hub answers `target=darwin` on `/api/v1/desktop/update`.
