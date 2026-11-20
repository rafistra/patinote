#!/usr/bin/env bash
# Tests for scripts/check-vault-data.sh. Each case builds a throwaway git repository with
# synthetic files, runs the check, and asserts its exit code and, when given, its output.
#
# Usage: scripts/tests/check-vault-data.test.sh

set -euo pipefail

# CI runners set CI=true, which makes check D refuse to run. These tests only use a
# synthetic fake vault, so they clear it; the "refuses to run in CI" case sets it again.
unset CI

script="$(cd "$(dirname "$0")/.." && pwd)/check-vault-data.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

passed=0
failed=0

# Creates a clean repository with a valid fixture and manifest, and cds into it.
new_repo() {
  local dir="$work/$1"
  mkdir -p "$dir"
  cd "$dir" || exit 1
  git init -q
  git config user.name test
  git config user.email test@example.invalid
  mkdir -p scripts fixtures/vault-mockup/.obsidian fixtures/vault-mockup/Harian docs
  cp "$script" scripts/check-vault-data.sh
  echo '{}' >fixtures/vault-mockup/.obsidian/app.json
  echo '# Daily' >fixtures/vault-mockup/Harian/2026-11-19.md
  echo '# Docs' >docs/PRD.md
  echo '# Readme' >README.md
  git add -A
  scripts/check-vault-data.sh --update-manifest 2>/dev/null
  git add -A
}

# expect <name> <expected exit> <expected stderr substring or ""> -- <args...>
expect() {
  local name=$1 want=$2 needle=$3
  shift 4
  local got=0 out
  out=$(scripts/check-vault-data.sh "$@" 2>&1) || got=$?
  if [[ "$got" -ne "$want" ]]; then
    echo "FAIL $name: exit $got, want $want"$'\n'"$out"
    failed=$((failed + 1))
  elif [[ -n "$needle" && "$out" != *"$needle"* ]]; then
    echo "FAIL $name: output does not contain '$needle'"$'\n'"$out"
    failed=$((failed + 1))
  else
    echo "ok   $name"
    passed=$((passed + 1))
  fi
}

new_repo clean
expect "clean repository passes" 0 "ok" --

new_repo a_md_outside
mkdir -p crates/x/src && echo 'secret' >crates/x/src/notes.md && git add -A
expect "A: Markdown outside allowed folders fails" 1 "A: crates/x/src/notes.md" --

new_repo a_obsidian_outside
mkdir -p data/.obsidian && echo '{}' >data/.obsidian/app.json && git add -A
expect "A: .obsidian outside fixtures fails" 1 "A: data/.obsidian/app.json" --

new_repo a_conflict_in_docs
echo x >"docs/Note.sync-conflict-20261119-000000-ABC.md" && git add -A
expect "A: sync-conflict file in docs fails" 1 "A: docs/Note.sync-conflict" --

new_repo a_tests_allowed
mkdir -p crates/core/tests/golden && echo '[[x]]' >crates/core/tests/golden/link.md && git add -A
expect "A: Markdown in crates/*/tests is allowed" 0 "" --

new_repo b_fixture_changed
echo 'changed' >>fixtures/vault-mockup/Harian/2026-11-19.md && git add -A
expect "B: changed fixture without manifest update fails" 1 "B: fixtures/vault-mockup does not match" --

new_repo b_fixture_added
echo 'new' >fixtures/vault-mockup/New.md && git add -A
expect "B: added fixture file fails" 1 "B:" --

new_repo b_manifest_updated
echo 'changed' >>fixtures/vault-mockup/Harian/2026-11-19.md && git add -A
scripts/check-vault-data.sh --update-manifest 2>/dev/null && git add -A
expect "B: changed fixture with manifest update passes" 0 "" --

new_repo b_unstaged_change_ignored
echo 'unstaged' >>fixtures/vault-mockup/Harian/2026-11-19.md
expect "B: unstaged working tree change is not checked" 0 "" --

new_repo c_home_path
echo 'vault at /home/someone/Vault' >>docs/PRD.md && git add -A
expect "C: /home/<name>/ fails" 1 "C: docs/PRD.md" --

new_repo c_users_path
echo 'see /Users/someone/Notes/' >>README.md && git add -A
expect "C: /Users/<name>/ fails" 1 "C: README.md" --

new_repo c_placeholder
echo 'vault at ~/Vault or /path/to/Vault, docs at /home/<name>/' >>docs/PRD.md && git add -A
expect "C: ~/ and placeholders pass" 0 "" --

# D uses a fake "real" vault with synthetic content.
real="$work/real-vault"
mkdir -p "$real/.obsidian" "$real/Private"
printf 'Meeting notes with a long enough body to be hashed, about the garden plan.\n' >"$real/Private/Garden plan.md"
printf 'x\n' >"$real/Ok.md"
echo '{}' >"$real/.obsidian/app.json"

new_repo d_copied_file
cp "$real/Private/Garden plan.md" docs/copied.md && git add -A
export PATINOTE_REAL_VAULT="$real"
expect "D: copy of a real vault file fails" 1 "D: docs/copied.md is identical" -- --staged
unset PATINOTE_REAL_VAULT

new_repo d_name_in_line
echo 'See the Garden Plan for details.' >>docs/PRD.md && git add -A
export PATINOTE_REAL_VAULT="$real"
expect "D: real note name in added line fails" 1 "D: 1 added line(s) in docs/PRD.md" -- --staged
unset PATINOTE_REAL_VAULT

new_repo d_name_in_path
echo 'synthetic' >"docs/Garden plan notes.md" && git add -A
export PATINOTE_REAL_VAULT="$real"
expect "D: real note name in path fails" 1 "D: the path docs/Garden plan notes.md" -- --staged
unset PATINOTE_REAL_VAULT

new_repo d_output_hides_names
echo 'See the Garden Plan for details.' >>docs/PRD.md && git add -A
out=$(PATINOTE_REAL_VAULT="$real" scripts/check-vault-data.sh --staged 2>&1 || true)
if [[ "$out" != *"D: 1 added line"* || "$out" == *"Garden plan"* || "$out" == *"Garden Plan"* || "$out" == *"$real"* ]]; then
  echo "FAIL D: output leaks a vault name or path"$'\n'"$out"
  failed=$((failed + 1))
else
  echo "ok   D: output does not contain vault names or paths"
  passed=$((passed + 1))
fi

new_repo d_short_and_config_ignored
echo 'Ok then. {}' >>docs/PRD.md && git add -A
export PATINOTE_REAL_VAULT="$real"
expect "D: short names and .obsidian files are ignored" 0 "" -- --staged
unset PATINOTE_REAL_VAULT

new_repo d_not_without_staged
echo 'See the Garden Plan for details.' >>docs/PRD.md && git add -A
export PATINOTE_REAL_VAULT="$real"
expect "D: does not run without --staged" 0 "" --
unset PATINOTE_REAL_VAULT

new_repo d_refuses_ci
export PATINOTE_REAL_VAULT="$real" CI=true
expect "D: refuses to run in CI" 1 "never be set in CI" -- --staged
unset PATINOTE_REAL_VAULT CI

new_repo d_missing_vault
export PATINOTE_REAL_VAULT="$work/does-not-exist"
expect "D: missing vault folder fails" 1 "not a folder" -- --staged
unset PATINOTE_REAL_VAULT

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
