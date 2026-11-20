# vaultgen

Generates deterministic synthetic vaults for Patinote tests and benchmarks.
Every name and sentence is synthetic; nothing comes from a real vault.

```bash
cargo run -p vaultgen -- --notes 10000 --out target/vaults/10k
cargo run -p vaultgen -- --notes 1000 --long-note-lines 5000 --out target/vaults/long
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--notes N` | required | Number of notes (`.md` files, not counting Syncthing conflict copies) |
| `--out DIR` | required | Output folder; must not exist or be empty |
| `--seed S` | `1` | The same seed and options always give a byte-identical vault |
| `--long-note-lines L` | `0` | Pads the first note, `Catatan panjang.md`, to at least `L` lines |

The random generator is SplitMix64, written in `src/rng.rs`, so vaults do not
change when a dependency is updated.

## What a vault contains

- Notes spread over fixed folders (`Inbox`, `Proyek`, `Area/…`, `Arsip/…`, the
  root) plus one `Proyek/Topik NNN` folder per 250 notes. Every tenth note is a
  daily note in `Harian/YYYY-MM-DD.md`.
- Every 40th note reuses the previous note's name in another folder, making an
  ambiguous name.
- Frontmatter on about 70% of notes: `tags` (flat and nested), `aliases` (about
  10% of notes), `created`, `status`, `priority`, `done`, and sometimes a
  `related: "[[…]]"` link.
- Every note has `## Ringkasan`, `## Detail` and `### Tindak lanjut`, an H1 on
  about half of them, and a block ID `^b<index>` on its first paragraph, so
  heading and block links always point at something real.
- Links in every form from PRD F-02: `[[name]]`, `[[name|text]]`,
  `[[name#Heading]]`, `[[name#Detail#Tindak lanjut]]`, `[[name#^b12]]`,
  `[[folder/name]]`, `[[name.md]]`, `![[name]]`, `![[image.png|300]]`,
  `[text](folder/name%20with%20spaces.md)`, links through aliases, and links to
  notes that do not exist (`[[Belum ditulis …]]`).
- Inline tags, task lists, and in about 30% of notes a code block and inline
  code that contain link and tag look-alikes, which must not be counted.
- One note in 20 uses CRLF line endings; one in 100 starts with a UTF-8 BOM.
- `Lampiran/gambar-NNNN.png` (one per 50 notes), a Syncthing conflict copy for
  one note in 500, and `.obsidian/app.json`.

## Summary

stdout receives one JSON object with the counts of what was written, for
example to compare against `patinote index --json` later:

| Field | Meaning |
| --- | --- |
| `notes`, `attachments`, `sync_conflicts`, `folders` | Files and non-root folders written |
| `links` | All links; the sum of the four fields below |
| `links_wiki`, `links_embed`, `links_markdown`, `links_frontmatter` | Links by kind, as in the index schema |
| `links_unresolved` | Links to notes that do not exist |
| `links_ambiguous` | Name-only links to a name shared by two notes |
| `ambiguous_names` | Names shared by two notes |
| `aliases`, `inline_tags`, `frontmatter_tags`, `headings`, `blocks` | Other parsed items |
| `crlf_notes`, `bom_notes`, `long_note_lines` | Encoding variants and the long note's line count |

Links inside code and inside conflict copies are not counted, because they
are not links. Log messages go to stderr.

On a laptop, 10,000 notes (about 41 MB) take under a second and 50,000 notes
(about 200 MB) about three seconds.
