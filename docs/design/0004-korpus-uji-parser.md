# 0004 — Korpus uji emas parser

- Status: **usulan** (20 November 2026), menunggu persetujuan pemilik
- Terkait PRD: F-02 (kriteria F-02.1)
- Terkait: `0001` bagian 4 (skema indeks), `docs/QUESTIONS.md` Q-002

Korpus uji emas adalah sekumpulan catatan kecil beserta hasil parse yang diharapkan. Korpus ditulis sebelum parser (langkah 1.1) agar menjadi patokan yang independen. Bentuk hasilnya mengikuti kolom skema `0001_init.sql`, jadi hasil parser bisa langsung disimpan ke indeks.

## 1. Lokasi dan cara membuat

- Kasus ada di `crates/patinote-core/tests/golden/`: `NN-nama.md` dan `NN-nama.json`.
- Sumbernya `scripts/golden-corpus.py`. Isi tiap kasus (teks catatan dan hasil yang diharapkan) ditulis tangan di skrip itu. Skrip hanya menghitung `line`, `byte_start`, dan `byte_end` dengan mencari teks `raw` di file, agar posisi tidak salah hitung.
- `python3 scripts/golden-corpus.py` menulis ulang semua file; `--check` gagal bila file di repo tidak sama dengan hasil skrip. CI menjalankan `--check`.
- `crates/patinote-core/tests/golden_corpus.rs` memeriksa setiap `.json` secara mandiri: bentuknya benar, dan potongan byte file pada `byte_start..byte_end` sama dengan `raw`.
- Di langkah 1.1, test parser membandingkan hasil parse setiap `.md` dengan `.json`-nya (F-02.1).

## 2. Bentuk JSON

```json
{
  "v": 1,
  "title": "Judul",
  "aliases": ["Nama lain"],
  "links": [
    {
      "kind": "wiki",
      "raw": "[[Catatan#Bagian|teks]]",
      "target": "Catatan",
      "subpath": "Bagian",
      "display": "teks",
      "line": 3,
      "byte_start": 41,
      "byte_end": 64
    }
  ],
  "tags": [{ "tag": "proyek/alpha", "display": "Proyek/Alpha", "source": "inline", "line": 5 }],
  "headings": [{ "level": 2, "text": "Bagian", "line": 7, "byte_start": 80 }],
  "blocks": [{ "block_id": "abc123", "line": 9 }],
  "properties": [{ "key": "status", "idx": 0, "type": "text", "value": "aktif" }],
  "warnings": [{ "code": "frontmatter_invalid" }]
}
```

Semua field selalu ada; nilai kosong ditulis `null` atau `[]`. Urutan item mengikuti urutan kemunculan di file, kecuali `properties` yang mengikuti urutan kunci di frontmatter.

## 3. Aturan umum

- **Posisi.** `line` mulai dari 1, dihitung dari awal file termasuk frontmatter. `byte_start` dan `byte_end` adalah offset byte di file apa adanya, termasuk BOM dan `\r`, dengan `byte_end` eksklusif. Dengan posisi ini rename (Fase 3) bisa menulis ulang link tanpa parse ulang.
- **`title`.** Nilai properti `title` di frontmatter; bila tidak ada, teks H1 pertama (ATX atau setext); bila tidak ada, `null`. Cadangan nama file diisi pengindeks, bukan parser.
- **Area yang tidak dibaca** link dan tag-nya: blok kode berpagar (`` ``` `` dan `~~~`) dan berindentasi, kode inline, rumus `$...$` dan `$$...$$`, komentar `%%...%%` (satu baris atau beberapa baris), serta teks yang di-escape (`\[[`, `\#`).
- **Frontmatter** hanya bila baris pertama file (setelah BOM) tepat `---` dan ada baris penutup `---`. YAML yang rusak menghasilkan peringatan `frontmatter_invalid`; properti, alias, dan tag frontmatter kosong, tetapi isi catatan tetap di-parse.

## 4. Link

| Bentuk | `kind` | `target` | `subpath` | `display` |
| --- | --- | --- | --- | --- |
| `[[Catatan]]` | `wiki` | `Catatan` | `null` | `null` |
| `[[Catatan\|teks]]` | `wiki` | `Catatan` | `null` | `teks` |
| `[[Catatan#A#B]]` | `wiki` | `Catatan` | `A#B` | `null` |
| `[[Catatan#^abc]]` | `wiki` | `Catatan` | `^abc` | `null` |
| `[[#A]]` | `wiki` | `""` (catatan ini) | `A` | `null` |
| `[[folder/Catatan.md]]` | `wiki` | `folder/Catatan.md` | `null` | `null` |
| `![[foto.png\|300]]` | `embed` | `foto.png` | `null` | `300` |
| `[teks](Catatan%20Baru.md#A)` | `markdown` | `Catatan Baru.md` | `A` | `teks` |
| `![alt](foto.png)` | `embed` | `foto.png` | `null` | `alt` |
| `x: "[[Catatan]]"` di frontmatter | `frontmatter` | `Catatan` | `null` | `null` |

- `raw` adalah teks persis di file, termasuk `!` pada embed. `target` ditulis seperti di file; normalisasi (huruf kecil, NFC, tanpa `.md`) urusan resolver (langkah 1.4).
- Link Markdown dan gambar Markdown hanya dihitung bila tujuannya path relatif. URL dengan skema (`https:`, `mailto:`) dan tujuan kosong bukan link catatan. `target` dan `subpath` link Markdown di-decode dari URL.
- Di embed, teks setelah `|` (misalnya ukuran `300`) disimpan di `display`. Teks tampilan yang kosong (`![](foto.png)`) ditulis `null`.
- Di dalam tabel, `\|` di dalam wikilink adalah pemisah teks tampilan; `raw` tetap memuat garis miring terbaliknya.
- Link frontmatter hanya dikenali bila seluruh nilai (atau seluruh item daftar) berupa satu wikilink dalam string YAML. `x: [[Catatan]]` tanpa kutip dibaca YAML sebagai daftar bersarang, jadi bukan link.
- `! [[Catatan]]` (ada spasi setelah `!`) adalah link `wiki`, bukan embed.
- Bentuk yang tidak lengkap bukan link: `[[]]`, `[[Catatan` tanpa penutup, `[Catatan]`, dan wikilink yang terpotong baris baru.

## 5. Tag

- Diawali `#` di awal baris atau setelah spasi, diikuti huruf (termasuk non-Latin), angka, `_`, `-`, atau `/`. Tag berakhir di karakter lain (`.`, `,`, `!`, spasi).
- Minimal satu karakter bukan angka: `#2026` bukan tag, `#y2026` tag.
- `tag` dalam huruf kecil tanpa `#`; `display` adalah ejaan asli tanpa `#`.
- Bukan tag: heading (`# Judul`), `#` sendirian, `#` di tengah kata (`a#b`) termasuk anchor URL (`https://x.com#bagian`), dan `#` di area yang tidak dibaca.
- Tag di frontmatter (`tags`, atau `tag` bentuk lama) berupa daftar YAML, daftar inline, atau teks dipisah koma; `source` = `frontmatter` dan `line` = `null`.

## 6. Heading, blok, dan properti

- **Heading:** ATX `#` sampai `######` diikuti spasi, dan setext (`===` untuk level 1, `---` untuk level 2). `text` tanpa penanda dan tanpa `#` penutup, sudah di-trim. `line` dan `byte_start` menunjuk awal baris teks heading. `#######` (tujuh) bukan heading. Link dan tag di dalam heading tetap dihitung.
- **Blok:** `^id` di akhir paragraf atau item daftar, didahului spasi, dengan karakter `A-Z a-z 0-9 -`. `block_id` tanpa `^`. `^` di tengah baris atau dengan karakter lain (misalnya `_`) bukan ID blok.
- **Properti:** satu baris per nilai. Daftar dipecah dengan `idx` 0, 1, 2, …; nilai tunggal ber-`idx` 0. `type` mengikuti YAML: `text`, `number`, `bool`, `date` (`2026-11-19`), `datetime` (`2026-11-19T10:30:00`), atau `null` (nilai kosong). Teks berbentuk tanggal ISO dianggap `date` atau `datetime` dengan atau tanpa kutip, karena library YAML tidak menyimpan informasi kutip dan Obsidian juga menentukan tipe per nama properti, bukan dari kutipnya. `tags` dan `aliases` juga muncul sebagai properti, dengan nilai seperti tertulis (teks dipisah koma tetap satu nilai). Nilai bersarang (map, atau daftar di dalam daftar) tidak dipecah ke properti; isinya tetap ada di frontmatter mentah (`notes.frontmatter`).
- **Alias:** dari `aliases` (atau `alias` bentuk lama), berupa daftar atau teks dipisah koma.

## 7. Yang perlu diverifikasi

Aturan di atas disusun dari dokumentasi dan perilaku Obsidian yang diketahui. Beberapa butir perlu dicocokkan dengan vault asli pemilik saat F-02.3 (hanya di laptop pemilik): link frontmatter di tengah teks, `\|` di wikilink di luar tabel, dan gambar Markdown sebagai `embed`. Bila hasilnya berbeda, korpus dan dokumen ini diperbarui bersama.
