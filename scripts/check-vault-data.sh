#!/usr/bin/env bash
# Guards the public repository against private vault data (CLAUDE.md, section 7).
#
# Usage:
#   scripts/check-vault-data.sh                    # checks A, B, C on the git index (CI)
#   scripts/check-vault-data.sh --staged           # same, plus D when PATINOTE_REAL_VAULT is set (pre-commit)
#   scripts/check-vault-data.sh --update-manifest  # rewrites fixtures/vault-mockup.sha256
#
# Checks (docs/QUESTIONS.md, Q-001):
#   A. Vault-like files (.md, .canvas, .obsidian/, .trash/, *.sync-conflict-*) only live in allowed places.
#   B. fixtures/vault-mockup/ matches fixtures/vault-mockup.sha256.
#   C. No absolute home paths (/home/<name>/, /Users/<name>/).
#   D. Staged files are not copies of files in the real vault, and staged paths and added
#      lines do not contain real note names. Runs only with --staged and PATINOTE_REAL_VAULT,
#      never in CI. Reports repository paths and counts only, never vault names or content.
#
# Everything is read from the git index, so the check sees exactly what will be committed.

set -euo pipefail
export LC_ALL=C.UTF-8

FIXTURE_DIR="fixtures/vault-mockup"
MANIFEST="fixtures/vault-mockup.sha256"
# Real vault files smaller than this are not compared by hash: tiny files such as `{}`
# match unrelated files too often to mean anything.
MIN_HASH_BYTES=64
# Real note names shorter than this are not searched for, for the same reason.
MIN_NAME_CHARS=4

mode="all"
case "${1:-}" in
  "") ;;
  --staged) mode="staged" ;;
  --update-manifest) mode="manifest" ;;
  -h | --help)
    sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  *)
    echo "check-vault-data: unknown argument: $1" >&2
    exit 2
    ;;
esac

cd "$(git rev-parse --show-toplevel)" || exit 1

failures=0
fail() {
  echo "check-vault-data: $*" >&2
  failures=$((failures + 1))
}

# Prints "<sha256>  <path>" for every indexed file under the fixture, sorted by path.
fixture_hashes() {
  git ls-files -z --cached -- "$FIXTURE_DIR" | sort -z | while IFS= read -r -d '' path; do
    hash=$(git cat-file blob ":$path" | sha256sum | cut -c1-64)
    printf '%s  %s\n' "$hash" "$path"
  done
}

if [[ "$mode" == "manifest" ]]; then
  fixture_hashes >"$MANIFEST"
  echo "check-vault-data: wrote $MANIFEST from the git index ($(wc -l <"$MANIFEST") files)." >&2
  echo "check-vault-data: stage the fixture changes first, and mention them in the PR." >&2
  exit 0
fi

# --- A. Location of vault-like files ---------------------------------------------------

is_test_dir() {
  [[ "$1" == fixtures/* || "$1" == tests/* || "$1" == crates/*/tests/* || "$1" == tools/*/tests/* ]]
}

while IFS= read -r -d '' path; do
  name=${path##*/}
  if [[ "/$path" == */.obsidian/* || "/$path" == */.trash/* || "$name" == *.sync-conflict-* || "$name" == *.canvas ]]; then
    is_test_dir "$path" || fail "A: $path looks like vault data; it is only allowed under fixtures/ or a tests/ folder."
  elif [[ "$name" == *.md ]]; then
    case "$path" in
      docs/* | .github/* | README.md | */README.md | CLAUDE.md | AGENTS.md | CONTRIBUTING.md | CHANGELOG.md) ;;
      *) is_test_dir "$path" || fail "A: $path is a Markdown file outside docs/, fixtures/ and tests/ folders." ;;
    esac
  fi
done < <(git ls-files -z --cached)

# --- B. Frozen fixture ------------------------------------------------------------------

if [[ ! -f "$MANIFEST" ]]; then
  fail "B: $MANIFEST is missing; run scripts/check-vault-data.sh --update-manifest."
elif ! git ls-files --error-unmatch --cached -- "$MANIFEST" >/dev/null 2>&1; then
  fail "B: $MANIFEST is not staged."
elif ! diff -u <(git cat-file blob ":$MANIFEST") <(fixture_hashes) >&2; then
  fail "B: $FIXTURE_DIR does not match $MANIFEST (diff above). If the change is intended, run scripts/check-vault-data.sh --update-manifest and mention it in the PR."
fi

# --- C. Absolute home paths --------------------------------------------------------------

home_paths=$(git grep --cached -I -l -E '(/home|/Users)/[A-Za-z0-9._-]+/' -- . ':!scripts/check-vault-data.sh' ':!scripts/tests/*' || true)
if [[ -n "$home_paths" ]]; then
  while IFS= read -r path; do
    fail "C: $path contains an absolute home path; use ~/ or a placeholder such as /path/to/Vault."
  done <<<"$home_paths"
fi

# --- D. Comparison with the real vault (pre-commit only) --------------------------------

if [[ "$mode" == "staged" && -n "${PATINOTE_REAL_VAULT:-}" ]]; then
  if [[ -n "${CI:-}" ]]; then
    echo "check-vault-data: PATINOTE_REAL_VAULT must never be set in CI." >&2
    exit 1
  fi
  vault=${PATINOTE_REAL_VAULT/#\~/$HOME}
  if [[ ! -d "$vault" ]]; then
    echo "check-vault-data: PATINOTE_REAL_VAULT is set but is not a folder." >&2
    exit 1
  fi

  # Vault names and hashes stay in private temporary files that are always removed.
  tmp=$(mktemp -d "${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}/patinote-check.XXXXXX")
  chmod 700 "$tmp"
  trap 'rm -rf "$tmp"' EXIT

  # stderr is discarded because find and sha256sum errors would print vault paths.
  # Unreadable vault files are skipped.
  { find "$vault" \( -name .git -o -name .obsidian \) -prune -o -type f -size +"$((MIN_HASH_BYTES - 1))"c -print0 2>/dev/null |
    xargs -0 -r sha256sum 2>/dev/null || true; } | cut -c1-64 | sort -u >"$tmp/hashes"
  find "$vault" \( -name .git -o -name .obsidian \) -prune -o -type f -name '*.md' -printf '%f\n' 2>/dev/null |
    sed 's/\.md$//' |
    grep -v -E '^[0-9 ._-]*$' |
    awk -v min="$MIN_NAME_CHARS" 'length($0) >= min' |
    sort -u >"$tmp/names" || true

  while IFS= read -r -d '' path; do
    hash=$(git cat-file blob ":$path" | sha256sum | cut -c1-64)
    if grep -q -x -F "$hash" "$tmp/hashes"; then
      fail "D: $path is identical to a file in the real vault."
    fi
    if [[ -s "$tmp/names" ]]; then
      if printf '%s\n' "$path" | grep -q -i -w -F -f "$tmp/names"; then
        fail "D: the path $path contains the name of a note in the real vault."
      fi
      count=$(git diff --cached --no-color -U0 -- "$path" | grep '^+' | grep -v '^+++' |
        grep -c -i -w -F -f "$tmp/names" || true)
      if [[ "$count" -gt 0 ]]; then
        fail "D: $count added line(s) in $path contain the name of a note in the real vault."
      fi
    fi
  done < <(git diff --cached -z --name-only --diff-filter=ACMR)
fi

if [[ "$failures" -gt 0 ]]; then
  echo "check-vault-data: $failures problem(s) found." >&2
  exit 1
fi
echo "check-vault-data: ok" >&2
