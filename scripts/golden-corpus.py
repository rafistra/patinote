#!/usr/bin/env python3
"""Source of the parser golden corpus (docs/design/0004-korpus-uji-parser.md).

Each case below is written by hand: the note text and the expected parse result.
This script only computes `line`, `byte_start` and `byte_end` by locating each
item's text in the file, then writes crates/patinote-core/tests/golden/NN-name.{md,json}.

Usage:
  python3 scripts/golden-corpus.py           # rewrite the corpus
  python3 scripts/golden-corpus.py --check   # fail if the corpus on disk differs

All content is synthetic (CLAUDE.md, section 7).
"""

import json
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "crates" / "patinote-core" / "tests" / "golden"


# --- Expected items ------------------------------------------------------------------
# `skip` skips that many earlier occurrences of the searched text.


def L(kind, raw, target, subpath=None, display=None, skip=0):
    return {"kind": kind, "raw": raw, "target": target, "subpath": subpath, "display": display, "skip": skip}


def T(tag, display, source="inline", skip=0):
    return {"tag": tag, "display": display, "source": source, "skip": skip}


def H(level, text, at=None):
    return {"level": level, "text": text, "at": at if at is not None else "#" * level + " " + text}


def B(block_id):
    return {"block_id": block_id}


def P(key, idx, type_, value):
    return {"key": key, "idx": idx, "type": type_, "value": value}


def case(name, md, title=None, aliases=(), links=(), tags=(), headings=(), blocks=(),
         properties=(), warnings=(), bom=False, crlf=False):
    return dict(name=name, md=md, title=title, aliases=list(aliases), links=list(links),
                tags=list(tags), headings=list(headings), blocks=list(blocks),
                properties=list(properties), warnings=list(warnings), bom=bom, crlf=crlf)


CASES = [
    # --- Wikilinks: one positive and one negative case per form in PRD F-02 ---------
    case("wikilink-plain", """# Wikilink biasa

Lihat [[Catatan]] dan [[Café Bandung]] hari ini.
Dua link satu baris: [[Alfa]], [[Beta]].
""", title="Wikilink biasa", headings=[H(1, "Wikilink biasa")], links=[
        L("wiki", "[[Catatan]]", "Catatan"),
        L("wiki", "[[Café Bandung]]", "Café Bandung"),
        L("wiki", "[[Alfa]]", "Alfa"),
        L("wiki", "[[Beta]]", "Beta"),
    ]),
    case("wikilink-plain-negative", """Bukan link: [[]] dan [Catatan] dan [[Catatan tanpa penutup.

Juga bukan [[Catatan
terpotong]] di dua baris.
"""),
    case("wikilink-display", """Rapat dengan [[Catatan|teks]] dan [[Proyek Alfa|proyek kita yang besar]].
""", links=[
        L("wiki", "[[Catatan|teks]]", "Catatan", display="teks"),
        L("wiki", "[[Proyek Alfa|proyek kita yang besar]]", "Proyek Alfa", display="proyek kita yang besar"),
    ]),
    case("wikilink-display-negative", """Kurung tunggal [Catatan|teks] bukan link.
Penutup kurang [[Catatan|teks] juga bukan.
"""),
    case("wikilink-heading", """Ke bagian [[Catatan#Bagian]] dan sub [[Catatan#Bagian#Sub bagian]].
Dengan teks [[Catatan#Bagian|lihat bagian]].
""", links=[
        L("wiki", "[[Catatan#Bagian]]", "Catatan", subpath="Bagian"),
        L("wiki", "[[Catatan#Bagian#Sub bagian]]", "Catatan", subpath="Bagian#Sub bagian"),
        L("wiki", "[[Catatan#Bagian|lihat bagian]]", "Catatan", subpath="Bagian", display="lihat bagian"),
    ]),
    case("wikilink-heading-negative", """Dalam kode inline: `[[Catatan#Bagian]]`.

```text
[[Catatan#Bagian#Sub]]
```
"""),
    case("wikilink-block", """Kutip blok [[Catatan#^abc123]] dan [[Catatan#^blok-1|kutipan]].
""", links=[
        L("wiki", "[[Catatan#^abc123]]", "Catatan", subpath="^abc123"),
        L("wiki", "[[Catatan#^blok-1|kutipan]]", "Catatan", subpath="^blok-1", display="kutipan"),
    ]),
    case("wikilink-block-negative", """%%[[Catatan#^abc123]]%% tidak dihitung.
"""),
    case("wikilink-same-note", """## Bagian satu

Lompat ke [[#Bagian satu]] atau blok [[#^akhir]] atau [[#Bagian satu|ke atas]].

Paragraf akhir. ^akhir
""", headings=[H(2, "Bagian satu")], blocks=[B("akhir")], links=[
        L("wiki", "[[#Bagian satu]]", "", subpath="Bagian satu"),
        L("wiki", "[[#^akhir]]", "", subpath="^akhir"),
        L("wiki", "[[#Bagian satu|ke atas]]", "", subpath="Bagian satu", display="ke atas"),
    ]),
    case("wikilink-same-note-negative", r"""Escape: \[[#Bagian]] bukan link.
"""),
    case("wikilink-path", """Path [[Proyek/Catatan]] dan ekstensi [[Catatan.md]] dan keduanya [[Proyek/Catatan.md#Bagian|teks]].
""", links=[
        L("wiki", "[[Proyek/Catatan]]", "Proyek/Catatan"),
        L("wiki", "[[Catatan.md]]", "Catatan.md"),
        L("wiki", "[[Proyek/Catatan.md#Bagian|teks]]", "Proyek/Catatan.md", subpath="Bagian", display="teks"),
    ]),
    case("wikilink-path-negative", r"""Escape \[[Proyek/Catatan]] dan kode `[[Catatan.md]]`.
"""),
    case("embed", """![[Catatan]]

Gambar ![[foto.png|300]] dan bagian ![[Catatan#Bagian]] dan dokumen ![[laporan.pdf]].
""", links=[
        L("embed", "![[Catatan]]", "Catatan"),
        L("embed", "![[foto.png|300]]", "foto.png", display="300"),
        L("embed", "![[Catatan#Bagian]]", "Catatan", subpath="Bagian"),
        L("embed", "![[laporan.pdf]]", "laporan.pdf"),
    ]),
    case("embed-negative", """Dengan spasi ! [[Catatan]] adalah link biasa.

```
![[foto.png]]
```
""", links=[L("wiki", "[[Catatan]]", "Catatan")]),
    case("markdown-link", """Lihat [teks](Catatan.md) dan [catatan baru](Catatan%20Baru.md) dan [bagian](Proyek/Catatan.md#Bagian%20satu).
""", links=[
        L("markdown", "[teks](Catatan.md)", "Catatan.md", display="teks"),
        L("markdown", "[catatan baru](Catatan%20Baru.md)", "Catatan Baru.md", display="catatan baru"),
        L("markdown", "[bagian](Proyek/Catatan.md#Bagian%20satu)", "Proyek/Catatan.md", subpath="Bagian satu", display="bagian"),
    ]),
    case("markdown-link-negative", """Situs [contoh](https://example.com/Catatan.md) dan surel [kirim](mailto:a@example.com) dan kosong [kosong]().
"""),
    case("markdown-image", """![diagram](Lampiran/diagram%20indeks.png) dan ![](foto.png)
""", links=[
        L("embed", "![diagram](Lampiran/diagram%20indeks.png)", "Lampiran/diagram indeks.png", display="diagram"),
        L("embed", "![](foto.png)", "foto.png"),
    ]),
    case("frontmatter-link", """---
proyek: "[[Patinote]]"
terkait:
  - "[[Alfa]]"
  - "[[Beta|b]]"
---
Isi.
""", links=[
        L("frontmatter", "[[Patinote]]", "Patinote"),
        L("frontmatter", "[[Alfa]]", "Alfa"),
        L("frontmatter", "[[Beta|b]]", "Beta", display="b"),
    ], properties=[
        P("proyek", 0, "text", "[[Patinote]]"),
        P("terkait", 0, "text", "[[Alfa]]"),
        P("terkait", 1, "text", "[[Beta|b]]"),
    ]),
    case("frontmatter-link-negative", """---
proyek: [[Patinote]]
---
Isi tanpa link.
"""),
    case("table-escaped-pipe", r"""| Catatan | Keterangan |
| --- | --- |
| [[Catatan\|teks]] | dengan alias |
| [[Alfa]] | tanpa alias |
""", links=[
        L("wiki", r"[[Catatan\|teks]]", "Catatan", display="teks"),
        L("wiki", "[[Alfa]]", "Alfa"),
    ]),

    # --- Tags --------------------------------------------------------------------------
    case("tag-basic", """Belajar #rust hari ini.
#ide di awal baris.
Akhir kalimat #catatan, dan #lagi! juga #titik.
""", tags=[T("rust", "rust"), T("ide", "ide"), T("catatan", "catatan"), T("lagi", "lagi"), T("titik", "titik")]),
    case("tag-nested", """Proyek #proyek/patinote dan #area/kesehatan/olahraga.
""", tags=[T("proyek/patinote", "proyek/patinote"), T("area/kesehatan/olahraga", "area/kesehatan/olahraga")]),
    case("tag-characters", """Karakter #tag_satu #tag-dua #café #日本 #y2026.
""", tags=[T("tag_satu", "tag_satu"), T("tag-dua", "tag-dua"), T("café", "café"), T("日本", "日本"), T("y2026", "y2026")]),
    case("tag-numeric-negative", """Tahun #2026 dan nomor #123 bukan tag, tetapi #y2026 adalah tag.
""", tags=[T("y2026", "y2026")]),
    case("tag-case", """#Proyek, #proyek, dan #PROYEK sama.
""", tags=[T("proyek", "Proyek"), T("proyek", "proyek"), T("proyek", "PROYEK")]),
    case("tag-negative", r"""# Judul bukan tag

Satu # sendirian, kata#tengah, https://example.com#bagian, dan \#escape.
""", title="Judul bukan tag", headings=[H(1, "Judul bukan tag")]),
    case("tag-in-code", """Kode `#bukan` inline.

```
#bukan juga
```
"""),
    case("tag-frontmatter-list", """---
tags:
  - proyek/alfa
  - Rust
---
Isi #inline.
""", tags=[
        T("proyek/alfa", "proyek/alfa", "frontmatter"),
        T("rust", "Rust", "frontmatter"),
        T("inline", "inline"),
    ], properties=[P("tags", 0, "text", "proyek/alfa"), P("tags", 1, "text", "Rust")]),
    case("tag-frontmatter-inline-list", """---
tags: [ide, Bacaan]
---
""", tags=[T("ide", "ide", "frontmatter"), T("bacaan", "Bacaan", "frontmatter")],
        properties=[P("tags", 0, "text", "ide"), P("tags", 1, "text", "Bacaan")]),
    case("tag-frontmatter-string", """---
tags: ide, bacaan
---
""", tags=[T("ide", "ide", "frontmatter"), T("bacaan", "bacaan", "frontmatter")],
        properties=[P("tags", 0, "text", "ide, bacaan")]),
    case("tag-frontmatter-legacy", """---
tag: lama
---
""", tags=[T("lama", "lama", "frontmatter")], properties=[P("tag", 0, "text", "lama")]),

    # --- Headings, blocks, title -------------------------------------------------------
    case("headings-atx", """# Satu
## Dua ##
### Tiga
#### Empat
##### Lima
###### Enam
####### Tujuh bukan heading
#nospasi adalah tag
""", title="Satu", headings=[
        H(1, "Satu"), H(2, "Dua", at="## Dua ##"), H(3, "Tiga"), H(4, "Empat"), H(5, "Lima"), H(6, "Enam"),
    ], tags=[T("nospasi", "nospasi")]),
    case("headings-setext", """Judul setext
============

Sub judul
---------

Teks.
""", title="Judul setext", headings=[H(1, "Judul setext", at="Judul setext"), H(2, "Sub judul", at="Sub judul")]),
    case("heading-with-link-and-tag", """## Lihat [[Catatan]] #penting
""", headings=[H(2, "Lihat [[Catatan]] #penting")], links=[L("wiki", "[[Catatan]]", "Catatan")],
        tags=[T("penting", "penting")]),
    case("blocks", """Paragraf dengan ID. ^abc123

- Item daftar ^blok-1
- Item lain

Garis bawah tidak boleh ^abc_def
Tengah ^tengah baris.
""", blocks=[B("abc123"), B("blok-1")]),
    case("title-frontmatter", """---
title: Judul dari properti
---
# Judul H1
""", title="Judul dari properti", headings=[H(1, "Judul H1")],
        properties=[P("title", 0, "text", "Judul dari properti")]),

    # --- Frontmatter ---------------------------------------------------------------------
    case("frontmatter-types", """---
teks: halo dunia
angka: 42
pecahan: 3.5
benar: true
tanggal: 2026-11-19
dikutip: "2026-11-20"
waktu: 2026-11-19T10:30:00
kosong:
daftar:
  - satu
  - 2
  - false
---
Isi.
""", properties=[
        P("teks", 0, "text", "halo dunia"),
        P("angka", 0, "number", 42),
        P("pecahan", 0, "number", 3.5),
        P("benar", 0, "bool", True),
        P("tanggal", 0, "date", "2026-11-19"),
        P("dikutip", 0, "date", "2026-11-20"),
        P("waktu", 0, "datetime", "2026-11-19T10:30:00"),
        P("kosong", 0, "null", None),
        P("daftar", 0, "text", "satu"),
        P("daftar", 1, "number", 2),
        P("daftar", 2, "bool", False),
    ]),
    case("frontmatter-aliases-list", """---
aliases:
  - Second Brain
  - Otak Kedua
---
""", aliases=["Second Brain", "Otak Kedua"],
        properties=[P("aliases", 0, "text", "Second Brain"), P("aliases", 1, "text", "Otak Kedua")]),
    case("frontmatter-aliases-string", """---
aliases: Second Brain, Otak Kedua
---
""", aliases=["Second Brain", "Otak Kedua"], properties=[P("aliases", 0, "text", "Second Brain, Otak Kedua")]),
    case("frontmatter-alias-legacy", """---
alias: Nama Lama
---
""", aliases=["Nama Lama"], properties=[P("alias", 0, "text", "Nama Lama")]),
    case("frontmatter-invalid", """---
judul: [tidak ditutup
status: aktif
---
Isi tetap dibaca: [[Catatan]] #tag
""", links=[L("wiki", "[[Catatan]]", "Catatan")], tags=[T("tag", "tag")],
        warnings=["frontmatter_invalid"]),
    case("frontmatter-not-at-top", """
---
status: aktif
---
Isi.
""", headings=[H(2, "status: aktif", at="status: aktif")]),
    case("frontmatter-nested-map", """---
penulis:
  nama: Ana
  kota: Bandung
status: aktif
---
""", properties=[P("status", 0, "text", "aktif")]),

    # --- Areas that are not read ---------------------------------------------------------
    case("code-fenced", """Sebelum.

```markdown
[[Dalam pagar]] #dalam
```

~~~
[[Dalam tilde]]
~~~

Sesudah [[Luar]].
""", links=[L("wiki", "[[Luar]]", "Luar")]),
    case("code-indented", """Paragraf.

    [[Dalam indentasi]] #indentasi

Setelah [[Luar]].
""", links=[L("wiki", "[[Luar]]", "Luar")]),
    case("inline-code-and-math", """Kode `[[x]]` dan rumus $[[y]]$ dan blok:

$$
[[z]] #rumus
$$

Lalu [[Luar]].
""", links=[L("wiki", "[[Luar]]", "Luar")]),
    case("comments", """Satu baris %%[[x]] #rahasia%% lalu [[Luar]].

%%
[[Dalam komentar]]
#komentar
%%
""", links=[L("wiki", "[[Luar]]", "Luar")]),
    case("escapes", r"""Escape \[[Bukan]] dan \#bukan, tetapi [[Ya]] dan #ya.
""", links=[L("wiki", "[[Ya]]", "Ya")], tags=[T("ya", "ya")]),

    # --- Encoding --------------------------------------------------------------------------
    case("bom-crlf", """---
tags: [a]
---
# Judul
Link [[Catatan]] dan #tag. ^blok
""", bom=True, crlf=True, title="Judul", headings=[H(1, "Judul")], blocks=[B("blok")],
        links=[L("wiki", "[[Catatan]]", "Catatan")],
        tags=[T("a", "a", "frontmatter"), T("tag", "tag")],
        properties=[P("tags", 0, "text", "a")]),
]


# --- Building --------------------------------------------------------------------------


def file_bytes(c):
    text = c["md"]
    if c["crlf"]:
        text = text.replace("\n", "\r\n")
    data = text.encode("utf-8")
    if c["bom"]:
        data = b"\xef\xbb\xbf" + data
    return data


class Locator:
    """Finds items in order, so each item is located after the previous one."""

    def __init__(self, data, name):
        self.data = data
        self.name = name
        self.pos = 0

    def find(self, needle, skip=0):
        raw = needle.encode("utf-8")
        at = self.pos
        for _ in range(skip + 1):
            at = self.data.find(raw, at)
            if at < 0:
                raise SystemExit(f"{self.name}: cannot find {needle!r}")
            found = at
            at += len(raw)
        self.pos = found + len(raw)
        return found, found + len(raw)

    def find_at_line_end(self, needle):
        """Like find, but only accepts an occurrence that ends its line."""
        while True:
            start, end = self.find(needle)
            if end == len(self.data) or self.data[end:end + 1] in (b"\n", b"\r"):
                return start, end

    def line_of(self, offset):
        return self.data.count(b"\n", 0, offset) + 1

    def line_start(self, offset):
        return self.data.rfind(b"\n", 0, offset) + 1


def build(c, name):
    data = file_bytes(c)
    loc = Locator(data, name)
    links = []
    for l in c["links"]:
        start, end = loc.find(l["raw"], l["skip"])
        links.append({
            "kind": l["kind"], "raw": l["raw"], "target": l["target"], "subpath": l["subpath"],
            "display": l["display"], "line": loc.line_of(start), "byte_start": start, "byte_end": end,
        })

    loc = Locator(data, name)
    tags = []
    for t in c["tags"]:
        line = None
        if t["source"] == "inline":
            start, _ = loc.find("#" + t["display"], t["skip"])
            line = loc.line_of(start)
        tags.append({"tag": t["tag"], "display": t["display"], "source": t["source"], "line": line})

    loc = Locator(data, name)
    headings = []
    for h in c["headings"]:
        start, _ = loc.find(h["at"])
        headings.append({"level": h["level"], "text": h["text"], "line": loc.line_of(start),
                         "byte_start": loc.line_start(start)})

    loc = Locator(data, name)
    blocks = []
    for b in c["blocks"]:
        start, _ = loc.find_at_line_end("^" + b["block_id"])
        blocks.append({"block_id": b["block_id"], "line": loc.line_of(start)})

    expected = {
        "v": 1,
        "title": c["title"],
        "aliases": c["aliases"],
        "links": links,
        "tags": tags,
        "headings": headings,
        "blocks": blocks,
        "properties": c["properties"],
        "warnings": [{"code": w} for w in c["warnings"]],
    }
    return data, (json.dumps(expected, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def corpus():
    files = {}
    names = set()
    for i, c in enumerate(CASES, start=1):
        if c["name"] in names:
            raise SystemExit(f"duplicate case name {c['name']}")
        names.add(c["name"])
        stem = f"{i:02}-{c['name']}"
        md, js = build(c, stem)
        files[f"{stem}.md"] = md
        files[f"{stem}.json"] = js
    return files


def main():
    check = sys.argv[1:] == ["--check"]
    if sys.argv[1:] not in ([], ["--check"]):
        raise SystemExit("usage: golden-corpus.py [--check]")
    files = corpus()
    on_disk = {p.name: p.read_bytes() for p in OUT.glob("*")} if OUT.exists() else {}
    if check:
        stale = sorted(set(files) ^ set(on_disk) | {n for n in files if on_disk.get(n) != files[n]})
        if stale:
            for n in stale:
                print(f"golden-corpus: out of date: {n}", file=sys.stderr)
            print("golden-corpus: run python3 scripts/golden-corpus.py", file=sys.stderr)
            sys.exit(1)
        print(f"golden-corpus: ok ({len(CASES)} cases)", file=sys.stderr)
        return
    OUT.mkdir(parents=True, exist_ok=True)
    for name in set(on_disk) - set(files):
        (OUT / name).unlink()
    for name, data in files.items():
        (OUT / name).write_bytes(data)
    print(f"golden-corpus: wrote {len(CASES)} cases to {OUT.relative_to(Path.cwd()) if OUT.is_relative_to(Path.cwd()) else OUT}",
          file=sys.stderr)


if __name__ == "__main__":
    main()
