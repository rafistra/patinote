# 0001 — Struktur workspace dan skema indeks

- Status: **diterima** (19 November 2026)
- Tanggal: 19 November 2026
- Terkait roadmap: Fase 0 dan Fase 1

## 1. Struktur Cargo workspace

```text
patinote/
├── Cargo.toml                 # [workspace], versi dependensi terpusat
├── crates/
│   ├── patinote-core/         # model, parser, resolusi link, operasi vault, registri perintah
│   ├── patinote-index/        # SQLite + FTS5, migrasi, query
│   │   └── migrations/0001_init.sql
│   ├── patinote-cli/          # binary `patinote`: CLI + `patinote mcp`
│   └── patinote-app/          # binary `patinote-app`: UI Iced
├── tools/
│   └── vaultgen/              # generator vault uji 1k / 10k / 50k (Fase 0)
├── benches/                   # criterion
└── docs/
    ├── ROADMAP.md
    └── design/
```

### Arah dependensi

```text
patinote-cli ──┐
               ├──► patinote-index ──► patinote-core
patinote-app ──┘           ▲
                           └── (mengimplementasikan trait Store milik core)
```

**Masalah yang perlu dipecahkan:** registri perintah diputuskan ada di `core`, tetapi banyak perintah (cari, backlinks) butuh indeks, sementara `index` butuh tipe hasil parser dari `core`. Kalau `core` bergantung pada `index`, terjadi siklus.

**Usulan: dependency inversion.** `core` mendefinisikan trait `Store` (operasi baca/tulis indeks yang dibutuhkan), dan `index` mengimplementasikannya dengan SQLite. Semua logika ada di `Vault<S: Store>` di `core`. CLI dan app merakitnya menjadi `Vault<SqliteStore>`.

Keuntungannya:
- `core` tetap tanpa SQLite dan tanpa UI, dan bisa diuji dengan `MemoryStore` palsu yang cepat.
- Registri perintah tetap di `core`, sesuai keputusan awal.
- Mengganti backend pencarian (FTS5 ke tantivy) hanya menyentuh `index`.

### Isi masing-masing crate

| Crate | Isi | Dependensi utama |
| --- | --- | --- |
| `patinote-core` | Tipe (`NotePath`, `ParsedNote`, `Link`, `Tag`), parser Markdown + wikilink + tag + frontmatter, resolver link ala Obsidian, scanner vault, file watcher, operasi tulis (create/append/rename dengan `--dry-run`), registri perintah, trait `Store` | `pulldown-cmark`, `serde`, `serde_yaml_ng`, `ignore`, `notify`, `blake3`, `unicode-normalization`, `thiserror` |
| `patinote-index` | `SqliteStore`, migrasi, query backlinks/tag/orphan/properti, pencarian FTS5 | `rusqlite` (fitur `bundled`), `rusqlite_migration` |
| `patinote-cli` | Parsing argumen, output teks dan `--json`, exit code, server MCP | `clap`, `serde_json`, `rmcp`, `anyhow` |
| `patinote-app` | UI Iced; satu-satunya crate yang menyentuh Iced | `iced` (versi dikunci) |

Catatan:
- `rusqlite` dengan fitur `bundled` agar versi SQLite (dan FTS5) sama di semua mesin, tidak bergantung pada `libsqlite3` sistem Ubuntu.
- `serde_yaml` sudah tidak dirawat; pakai penerusnya (`serde_yaml_ng` atau `serde_norway`). Pastikan dulu lisensinya lolos `cargo-deny`.

## 2. Lokasi dan siklus hidup indeks

- Path: `$XDG_CACHE_HOME/patinote/<vault-id>/index.db` (default `~/.cache/patinote/...`).
- `vault-id` = 16 karakter heksadesimal pertama dari blake3 path vault kanonik, sehingga banyak vault tidak saling bertabrakan.
- Pragma saat koneksi dibuka: `journal_mode=WAL` (UI bisa membaca saat watcher menulis), `synchronous=NORMAL`, `foreign_keys=ON`.
- Versi skema di `PRAGMA user_version`. Kalau versi DB lebih baru dari aplikasi, atau migrasi gagal, **hapus dan bangun ulang**. Indeks hanyalah cache, jadi ini selalu aman.

## 3. Deteksi perubahan (indeks inkremental)

1. Pindai vault, lalu bandingkan `(size, mtime_ns)` dengan tabel `notes`.
2. Sama: lewati (jalur cepat, tanpa membaca file).
3. Beda: baca file dan hitung blake3. Kalau hash sama (misalnya file hanya di-`touch` atau disalin ulang oleh Syncthing), cukup perbarui `mtime_ns`.
4. Hash beda: parse ulang, lalu ganti semua baris anak catatan itu (links, tags, headings, blocks, properties, FTS) dalam **satu transaksi**.
5. File yang hilang dari disk dihapus dari `notes`. `ON DELETE CASCADE` membersihkan baris anak, dan `ON DELETE SET NULL` mengubah link yang menunjuk ke catatan itu menjadi unresolved.
6. Catatan baru dibuat: cari link unresolved dengan `target_lower` yang cocok (lewat indeks parsial `links_unresolved`), lalu resolve.

Semua perubahan dari watcher dikumpulkan (debounce sekitar 100 ms) dan diproses per batch dalam satu transaksi.

## 4. Skema

File lengkap: `crates/patinote-index/migrations/0001_init.sql`. Ringkasan:

| Tabel | Fungsi | Catatan desain |
| --- | --- | --- |
| `meta` | Versi aplikasi, path vault, waktu indeks penuh terakhir | |
| `notes` | Satu baris per file: catatan, lampiran, canvas | Lampiran ikut dicatat karena `[[gambar.png]]` juga link. `path` relatif dengan separator `/` dan Unicode NFC |
| `aliases` | Dari frontmatter `aliases:` | Dipakai resolver dan autocomplete `[[` |
| `links` | Semua link keluar | `dst_id NULL` = belum ada catatannya. Posisi byte disimpan agar rename (Fase 3) bisa menulis ulang link dengan presisi tanpa parse ulang |
| `tags` | Tag inline dan frontmatter | Tag bersarang disimpan utuh (`proyek/patinote`) |
| `headings` | Outline dan target `[[catatan#bagian]]` | `slug` untuk mencocokkan subpath |
| `blocks` | Target `[[catatan#^id]]` | |
| `properties` | Properti frontmatter, satu baris per nilai | Daftar dipecah per item (`idx`) supaya query Dataview (Fase 4) memakai indeks biasa. Frontmatter mentah tetap disimpan sebagai JSON di `notes.frontmatter` |
| `notes_fts` | Pencarian full-text (FTS5) | `rowid = notes.id`. Menyimpan salinan isi supaya `snippet()` dan `highlight()` bekerja tanpa membaca file |

Semua tabel memakai `STRICT` supaya kesalahan tipe ketahuan sejak awal.

**Tabel yang sengaja ditunda:** `tasks` (Fase 4), embedding untuk pencarian semantik (Fase 10), dan log aktivitas agent (Fase 3). Ketiganya ditambahkan lewat migrasi berikutnya.

### Query kunci yang sudah diuji

| Kebutuhan | Query | Rencana eksekusi |
| --- | --- | --- |
| Backlinks | `SELECT … FROM links WHERE dst_id = ?` | `SEARCH links USING INDEX links_dst` |
| Tag + turunannya | `tag = ? OR (tag >= ? || '/' AND tag < ? || '0')` | Rentang ini setara `LIKE 'proyek/%'`, tetapi bisa memakai indeks (`'0'` adalah karakter setelah `'/'`) |
| Link ke catatan yang belum ada | `WHERE dst_id IS NULL` | Indeks parsial `links_unresolved` |
| Resolve nama atau alias | `stem_lower = ?` UNION `alias_lower = ?` | Indeks `notes_stem_lower`, `aliases_lower` |
| Orphan | Catatan tanpa link masuk dan keluar | `NOT EXISTS` di dua indeks |

### Aturan resolusi link (ala Obsidian)

Untuk `[[target]]`, setelah dinormalisasi (huruf kecil, NFC, tanpa ekstensi `.md`):

1. Kalau target berisi `/`: cocokkan dengan akhir `path_lower`.
2. Kalau tidak: cocokkan dengan `stem_lower`.
3. Kalau tidak ada yang cocok: cocokkan dengan `alias_lower`.
4. Kalau ada lebih dari satu kandidat: pilih yang path-nya paling pendek, lalu urutan abjad. Kasus ini dicatat sebagai link ambigu untuk diperingatkan.
5. Kalau tetap tidak ada: `dst_id = NULL`.

Aturan ini perlu diverifikasi terhadap vault Obsidian asli dengan test kompatibilitas.

## 5. Pencarian: FTS5 dulu, tantivy bila perlu

**Usulan:** mulai dengan FTS5, dibungkus trait `SearchBackend` di `index`.

| | FTS5 | tantivy |
| --- | --- | --- |
| Penyimpanan | Satu file DB dengan indeks lain | Direktori indeks terpisah |
| Konsistensi | Satu transaksi dengan `links` dan lainnya | Harus disinkronkan sendiri |
| Snippet + highlight | Bawaan | Bawaan |
| Ranking | BM25 | BM25, lebih bisa diatur |
| Stemming Indonesia | Tidak ada | Tidak bawaan, perlu tokenizer kustom |
| Ukuran binary | Hampir nol (sudah ada di SQLite) | Bertambah beberapa MB |

**Kriteria pindah ke tantivy:** benchmark Fase 0 menunjukkan pencarian di vault 50.000 catatan melebihi 50 ms, atau kebutuhan ranking di Fase 4 tidak bisa dipenuhi FTS5.

Untuk pencarian sebagian kata (misalnya "koord" menemukan "koordinasi"), FTS5 mendukung prefix query (`koord*`). Kalau butuh substring di tengah kata, tambahkan tabel FTS5 kedua dengan tokenizer `trigram`.

## 6. Kontrak CLI dan JSON (awal)

Prinsip:
- Dengan `--json`, **stdout hanya berisi JSON** dan pesan lain ke stderr. Agent bisa langsung mem-pipe output ke `jq`.
- Setiap output JSON punya field `"v"` (versi kontrak). Mengubah bentuk berarti menaikkan versi, bukan diam-diam mengganti.
- Path selalu relatif ke root vault.

Exit code:

| Kode | Arti |
| --- | --- |
| 0 | Berhasil |
| 1 | Error umum (I/O, DB) |
| 2 | Argumen salah |
| 3 | Tidak ditemukan (catatan, vault) |
| 4 | Konflik atau ambigu (nama catatan ganda, file berubah dari luar) |
| 5 | Indeks terkunci atau perlu dibangun ulang |

Contoh `patinote backlinks "Patinote" --json`:

```json
{
  "v": 1,
  "note": { "path": "Proyek/Patinote.md", "title": "Patinote" },
  "backlinks": [
    {
      "path": "Harian/2026-11-19.md",
      "line": 3,
      "subpath": null,
      "context": "Lanjut desain skema [[Patinote]] hari ini"
    }
  ]
}
```

Error dengan `--json` (ke stderr, exit 4):

```json
{
  "v": 1,
  "error": {
    "code": "ambiguous_note",
    "message": "Ada 2 catatan bernama 'Rapat'",
    "candidates": ["Kantor/Rapat.md", "Pribadi/Rapat.md"]
  }
}
```

`context` diambil dari isi FTS pada baris link, sehingga tidak perlu membaca file.

## 7. Pengecualian indeks

"Dikecualikan" = file tetap ada di disk, tetapi tidak muncul di pencarian, quick switcher, graf, dan unlinked mentions. Catatan yang dikecualikan tetap dikenali sebagai tujuan link, sehingga link ke sana tidak dilaporkan rusak.

| Lapis | Aturan | Bisa diubah |
| --- | --- | --- |
| Bawaan | `.obsidian/`, `.git/`, `.trash/`, `.stfolder/`, `.stversions/`, dan semua folder/file tersembunyi (diawali `.`) dilewati | Tidak |
| Khusus | Lampiran (gambar, PDF) dicatat di `notes` dengan `kind = 'attachment'`, tanpa isi FTS | Tidak |
| Khusus | `*.sync-conflict-*` tidak diindeks sebagai catatan; masuk daftar konflik | Tidak |
| Pengguna | `.patinote/ignore` di dalam vault, sintaks `.gitignore`, path relatif terhadap root vault; tidak ada secara default | Ya |

Implementasi: `ignore::WalkBuilder` dengan `git_ignore(false)`, `git_global(false)`, `git_exclude(false)`, `hidden(true)`, lalu `add_ignore("<vault>/.patinote/ignore")` bila file itu ada. Karena satu file berlaku untuk seluruh vault, dipakai `add_ignore` (file global), bukan `add_custom_ignore_filename` (yang dicari di setiap folder). `.gitignore` sengaja tidak dipatuhi, karena lampiran yang di-gitignore tetap harus dikenali.

Ditunda: membaca "Excluded files" (`userIgnoreFilters`) dari `.obsidian/app.json`, ditambahkan bila dibutuhkan.

## 8. Keputusan

| Pertanyaan | Keputusan |
| --- | --- |
| Arah dependensi | Empat crate. `core` mendefinisikan trait `Store`; `index` mengisinya dengan SQLite. Tidak ada crate `engine` |
| Backend pencarian | FTS5 dulu, di belakang trait `SearchBackend`; pindah ke tantivy hanya bila benchmark 50.000 catatan > 50 ms |
| Binary | Dua binary dalam satu paket: `patinote` (CLI + `patinote mcp`, tanpa dependensi UI) dan `patinote-app` (GUI). `patinote open <catatan>` menjalankan GUI. Entri `.desktop` menunjuk ke `patinote-app` |
| Pengecualian indeks | Bagian 7 |
| Mode Vim | Modul `vim` di `patinote-app` ditulis sebagai logika murni tanpa Iced, sehingga setiap perintah bisa dites otomatis |
| Bahasa | Teks UI, pesan CLI, dan dokumentasi publik dalam bahasa Inggris |
