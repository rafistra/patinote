# Patinote

A native, lightweight second brain for Ubuntu, written in Rust. Patinote opens
the same Markdown vault you already use with Obsidian, without conversion, and
adds a Vim mode, a command-line interface and an MCP server for AI agents.

> **Status:** early development. Nothing is usable yet. The project is working
> towards its first milestone, a daily driver that can replace Obsidian for
> everyday notes.

## Goals

- Open and edit an existing Obsidian vault as is, and keep it usable in
  Obsidian.
- Feel instant: start in under 300 ms and type without lag on a vault of
  10,000 notes.
- Work fully from the keyboard through a Vim mode.
- Expose every capability through the `patinote` CLI and MCP, so AI agents use
  the same code path as the app.
- Keep your data yours: `.md` files are the source of truth, and the SQLite
  index is only a cache that is safe to delete.

Patinote does not run Obsidian plugins and does not offer its own sync service.
Use Syncthing or Git for that.

## Workspace

| Crate | Purpose |
| --- | --- |
| `patinote-core` | Note model, parser, link resolver, vault scanner, watcher, write path, command registry, `Store` trait |
| `patinote-index` | SQLite index that implements `Store` |
| `patinote-cli` | The `patinote` binary: CLI and `patinote mcp` |
| `patinote-app` | The `patinote-app` binary: the Iced desktop app |

## Building

The Rust toolchain is pinned in `rust-toolchain.toml`; `rustup` picks it up
automatically.

```bash
cargo build --workspace
cargo test --workspace
```

## Installing

To install a release build for your user (Ubuntu, GNOME or any desktop that
reads XDG desktop entries):

```bash
scripts/install.sh              # installs to ~/.local
scripts/install.sh --uninstall  # removes it again
```

This puts `patinote` and `patinote-app` in `~/.local/bin`, adds Patinote to
the app launcher, and installs shell completions for bash, zsh and fish. Set
`PREFIX` to install somewhere else. Your vaults, settings and indexes are not
touched by either command.

To measure the app against the performance targets on your machine (needs a
desktop session; uses a generated vault only):

```bash
scripts/measure.sh
```

## Contributing

CI runs `cargo fmt`, `cargo clippy`, `cargo test`, `cargo deny` and a check
that keeps private vault data out of this public repository. Enable the same
checks before each commit:

```bash
git config core.hooksPath .githooks
```

The vault data check (`scripts/check-vault-data.sh`) only allows Markdown and
other vault files under `docs/`, `fixtures/` and `tests/` folders, keeps
`fixtures/vault-mockup/` identical to `fixtures/vault-mockup.sha256`, and
rejects absolute home paths. If you change the fixture on purpose, stage it and
run `scripts/check-vault-data.sh --update-manifest`.

If you develop against your own vault, set `PATINOTE_REAL_VAULT` to its path.
The pre-commit hook then also refuses commits that copy a file from that vault
or mention one of its note names. It prints only repository paths and counts,
never the names it matched. Never set this variable in CI.

## Documentation

- [`docs/keys.md`](docs/keys.md): every key and `:` command in the app.

Planning documents in `docs/` are written in Indonesian.

- `docs/PRD.md`: product requirements and acceptance criteria.
- `docs/design/`: technical design decisions.
- `docs/mockups/`: static UI mockups. Open them in a browser from inside the
  repository.

## License

Patinote is released under the [MIT License](LICENSE). The bundled Inter and
JetBrains Mono fonts are licensed under the SIL Open Font License 1.1; see
`crates/patinote-app/assets/fonts/`.
