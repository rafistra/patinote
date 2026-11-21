#!/usr/bin/env bash
# Builds Patinote in release mode and installs it for the current user
# (step 2.10): both binaries, the desktop entry, the icon and shell
# completions. Nothing is written outside PREFIX.
#
# Usage:
#   scripts/install.sh              install to ~/.local
#   scripts/install.sh --uninstall  remove what install put there
#   PREFIX=/some/dir scripts/install.sh
#
# Vaults, settings and indexes are never touched; uninstalling leaves
# ~/.config/patinote, ~/.cache/patinote and ~/.local/state/patinote as
# they are.
set -euo pipefail

cd "$(dirname "$0")/.."
prefix="${PREFIX:-$HOME/.local}"

files=(
    "bin/patinote"
    "bin/patinote-app"
    "share/applications/patinote.desktop"
    "share/icons/hicolor/scalable/apps/patinote.svg"
    "share/bash-completion/completions/patinote"
    "share/zsh/site-functions/_patinote"
    "share/fish/vendor_completions.d/patinote.fish"
)

refresh() {
    command -v update-desktop-database >/dev/null &&
        update-desktop-database -q "$prefix/share/applications" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null &&
        gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
    for f in "${files[@]}"; do
        rm -f "$prefix/$f"
    done
    refresh
    echo "Removed Patinote from $prefix"
    exit 0
fi
[ $# -eq 0 ] || { echo "usage: scripts/install.sh [--uninstall]" >&2; exit 2; }

cargo build --release --locked -p patinote-cli -p patinote-app

install -Dm755 target/release/patinote "$prefix/bin/patinote"
install -Dm755 target/release/patinote-app "$prefix/bin/patinote-app"
install -Dm644 packaging/patinote.svg "$prefix/share/icons/hicolor/scalable/apps/patinote.svg"
# The desktop entry runs the installed binary by its full path, so it works
# even when PREFIX/bin is not on the desktop's PATH.
sed "s|^Exec=patinote-app|Exec=$prefix/bin/patinote-app|" packaging/patinote.desktop |
    install -Dm644 /dev/stdin "$prefix/share/applications/patinote.desktop"

cli="$prefix/bin/patinote"
"$cli" completions bash | install -Dm644 /dev/stdin "$prefix/share/bash-completion/completions/patinote"
"$cli" completions zsh | install -Dm644 /dev/stdin "$prefix/share/zsh/site-functions/_patinote"
"$cli" completions fish | install -Dm644 /dev/stdin "$prefix/share/fish/vendor_completions.d/patinote.fish"

refresh
echo "Installed Patinote to $prefix"
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) echo "Add $prefix/bin to PATH to run patinote from a terminal." ;;
esac
