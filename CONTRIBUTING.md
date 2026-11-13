# Contributing

Thanks for helping. A few rules keep the project easy to trust and to maintain.

## Developer Certificate of Origin

Every commit must be signed off (`git commit -s`), certifying the
[DCO](https://developercertificate.org/): you wrote the change or have the right to submit it
under the project's Apache-2.0 license.

Do not copy code from projects under incompatible licenses (for example AGPL or GPL). Ideas are
fine; code is not.

## Before opening a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
DATABASE_URL=postgres://... BIN=target/debug scripts/e2e.sh   # needs an empty database
```

- Keep agent-facing text short: every line ends up in someone's context window.
- New API fields must be additive; breaking protocol changes bump `PROTOCOL_VERSION`.
- Anything that stores user text must go through `davra_core::redact`.
