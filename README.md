# Patinote

A native, lightweight second brain for Ubuntu, written in Rust. Patinote opens
the same Markdown vault you already use with Obsidian, without conversion, and
adds a Vim mode, a command-line interface and an MCP server for AI agents.

> **Status:** usable for daily notes, still in early development. The project
> is working towards its first milestone, a daily driver that can replace
> Obsidian for everyday notes. Keep your vault under Git or Syncthing while
> you try it.

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

## Getting started

Start Patinote from the app launcher, or from a terminal:

```bash
patinote-app                      # the last vault, or a welcome screen to pick one
patinote-app --vault ~/Vault      # a given vault
patinote open "Meeting notes"     # a note by name, alias or path, starting the app if needed
```

Without `--vault`, Patinote uses `$PATINOTE_VAULT`, then the last vault you
opened, then `[vault] default` from the configuration. Opening a vault does not
change any file in it; the first start builds the index in the background.

Patinote starts in the `minimal` layout: just the editor. Press `Space` in
normal mode and wait to see the next keys: `Space f` finds a note, `Space e`
opens the explorer, `Space l` switches to the `classic` layout with panels.
Every key and `:` command is listed in [`docs/keys.md`](docs/keys.md).

## Configuration

Settings live in `~/.config/patinote/config.toml`. Every key is optional:

```toml
layout = "minimal"        # "minimal", "classic" or "focus" on first start; the last one used is remembered
renderer = "tiny-skia"    # "tiny-skia" (less memory) or "wgpu" (GPU)

[vault]
default = "~/Vault"       # used when no other vault is given

[editor]
insert_escape = "jk"      # two keys that leave insert mode; off by default
relative_numbers = true   # relative line numbers; also `:set rnu` / `:set nornu`
```

To keep files out of lists, search and the explorer, add gitignore-style
patterns to `<vault>/.patinote/ignore`. Ignored notes still work as link
targets.

## CLI and MCP

The `patinote` command reads the same vault and index as the app:

```bash
patinote search "weekly review"   # full-text search
patinote backlinks "Project X"    # links to a note, with context
patinote unresolved               # links to notes that do not exist
patinote read "Project X" --json  # one JSON document, for scripts and agents
patinote --help                   # every command
```

With `--json`, stdout holds exactly one JSON document; progress and warnings go
to stderr. Exit codes are stable, so scripts can rely on them.

For AI agents, `patinote mcp` runs a read-only MCP server over stdio. Add it to
your MCP client:

```json
{"mcpServers": {"patinote": {"command": "patinote", "args": ["mcp", "--vault", "/path/to/Vault"]}}}
```

Its tools return the same JSON as the matching CLI commands. `patinote agents
init` writes an `AGENTS.md` into the vault with the vault's conventions and the
commands an agent may use.

## Where Patinote keeps data

Your `.md` files are the only source of truth. Patinote keeps its own data
outside the vault; the index is only a cache and is safe to delete while
Patinote is closed.

| What | Where |
| --- | --- |
| Settings | `~/.config/patinote/config.toml`, and `<vault>/.patinote/` for per-vault settings |
| Index (a cache) | `~/.cache/patinote/<vault-id>/` |
| Open notes, layout, write journal | `~/.local/state/patinote/<vault-id>/` |

Patinote never writes to `.obsidian/`. Deleted notes go to the vault's
`.trash/` folder, never away for good. Saves are atomic and keep line endings,
BOM and file permissions. `patinote cache clean` removes indexes of vaults that
no longer exist.

## Measuring

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
