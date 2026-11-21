#!/usr/bin/env python3
"""Expected results for the Vim engine tests, produced by Neovim (PRD F-06.1).

Reads the cases in crates/patinote-app/tests/vim/cases.json, runs each one in
`nvim --headless --clean` (scripts/vim-fixtures.lua) and writes what Neovim ends
with (lines, cursor, mode, unnamed register) to tests/vim/expected.json. The Rust
test (crates/patinote-app/tests/vim.rs) replays the same keys through the engine
and compares, without needing Neovim.

Usage:
  scripts/vim-fixtures.py            # rewrite expected.json
  scripts/vim-fixtures.py --check    # fail if expected.json differs from Neovim now
  scripts/vim-fixtures.py --only NAME [NAME...]   # print the results of some cases

Neovim is taken from $NVIM, or `nvim` on PATH; CI uses the release pinned in
.github/workflows/ci.yml. $VIM_FIXTURES points at another folder with a
cases.json (to try cases out), and $VIM_TIMEOUT sets the seconds a case may take
(default 10; a case whose keys leave a command unfinished never ends). Each case
has a `group`: the Rust test of that name runs it. All case text is synthetic
(CLAUDE.md, section 7).
"""

import json
import os
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# VIM_FIXTURES points at another folder with a cases.json, for trying out cases.
DIR = Path(os.environ.get("VIM_FIXTURES", ROOT / "crates" / "patinote-app" / "tests" / "vim"))
CASES = DIR / "cases.json"
EXPECTED = DIR / "expected.json"
RUNNER = Path(__file__).resolve().parent / "vim-fixtures.lua"
NVIM = os.environ.get("NVIM", "nvim")

# The window is 24 rows: 22 text lines, a status line and the command line, so
# Ctrl+d and Ctrl+u move 11 lines. The Rust test passes the same `viewport_lines`.
NVIM_ARGS = ["--headless", "--clean", "-n", "-i", "NONE", "--cmd", "set lines=24 columns=80"]

MODES = {"n": "normal", "i": "insert", "v": "visual", "V": "visual_line"}


# Key names the Rust test understands too (tests/vim.rs, `parse_keys`).
NAMES = {"Esc", "CR", "Enter", "BS", "Del", "Tab", "Left", "Right", "Up", "Down", "Home", "End",
         "lt", "Space"}


def split_keys(notation):
    """Splits Vim key notation into single keys: `d2w<Esc>` -> d, 2, w, <Esc>."""
    keys = []
    i = 0
    while i < len(notation):
        if notation[i] == "<":
            end = notation.find(">", i)
            name = notation[i + 1:end] if end > i + 1 else ""
            if name in NAMES or (name.startswith("C-") and len(name) == 3):
                keys.append(notation[i:end + 1])
                i = end + 1
                continue
        keys.append("<lt>" if notation[i] == "<" else notation[i])
        i += 1
    return keys


def run(case):
    with tempfile.TemporaryDirectory() as tmp:
        src = Path(tmp) / "in.json"
        dst = Path(tmp) / "out.json"
        src.write_text(json.dumps(dict(case, keys=split_keys(case["keys"]))), encoding="utf-8")
        env = dict(os.environ, VIM_CASE_IN=str(src), VIM_CASE_OUT=str(dst))
        try:
            proc = subprocess.run(
                [NVIM, *NVIM_ARGS, "-c", f"luafile {RUNNER}"],
                env=env, stdin=subprocess.DEVNULL, capture_output=True, check=False,
                timeout=float(os.environ.get("VIM_TIMEOUT", "10")),
            )
        except subprocess.TimeoutExpired:
            raise RuntimeError(f"{case['name']}: nvim did not finish (keys left a command pending?)")
        if not dst.exists():
            raise RuntimeError(f"{case['name']}: nvim wrote no result: {proc.stderr.decode()}")
        out = json.loads(dst.read_text(encoding="utf-8"))
    mode = MODES.get(out["mode"])
    if mode is None:
        raise RuntimeError(f"{case['name']}: unexpected nvim mode {out['mode']!r}")
    lines = out["lines"]
    # vim.json encodes an empty list as an object.
    if isinstance(lines, dict):
        lines = []
    return {
        "name": case["name"],
        "lines": lines,
        "cursor": out["cursor"],
        "mode": mode,
        "register": {"text": out["register"]["text"], "linewise": out["register"]["linewise"]},
    }


def render(results):
    rows = [json.dumps(r, ensure_ascii=False) for r in results]
    return "[\n" + ",\n".join(rows) + "\n]\n"


def main(argv):
    cases = json.loads(CASES.read_text(encoding="utf-8"))
    names = [c["name"] for c in cases]
    dupes = {n for n in names if names.count(n) > 1}
    if dupes:
        sys.exit(f"duplicate case names: {sorted(dupes)}")
    if argv[:1] == ["--only"] and argv[1:]:
        wanted = set(argv[1:])
        cases = [c for c in cases if c["name"] in wanted]
    with ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as pool:
        results = list(pool.map(run, cases))
    text = render(results)
    if argv[:1] == ["--only"]:
        sys.stdout.write(text)
        return
    if argv[:1] == ["--check"]:
        stored = EXPECTED.read_text(encoding="utf-8")
        if stored != text:
            old = {r["name"]: r for r in json.loads(stored)}
            for r in results:
                if old.get(r["name"]) != r:
                    print(f"differs: {r['name']}\n  stored: {old.get(r['name'])}\n  nvim:   {r}")
            sys.exit(f"{EXPECTED.relative_to(ROOT)} is stale: run scripts/vim-fixtures.py")
        print(f"{len(results)} Vim cases match Neovim")
        return
    EXPECTED.write_text(text, encoding="utf-8")
    print(f"wrote {len(results)} cases to {EXPECTED.relative_to(ROOT)}")


if __name__ == "__main__":
    main(sys.argv[1:])
