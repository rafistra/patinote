# CLAUDE.md — Instruksi kerja agent untuk Patinote

Patinote adalah aplikasi second brain native Rust untuk Ubuntu: pengganti Obsidian yang ringan, memakai vault Markdown yang sama, dengan mode Vim, CLI, dan server MCP. File ini mengatur cara agent AI membangunnya. Baca sampai habis sebelum mengerjakan apa pun.

## 1. Sumber kebenaran

Urutan prioritas bila ada pertentangan (yang atas menang):

1. `docs/PRD.md` — kebutuhan produk (Bagian I) dan spesifikasi beserta kriteria penerimaan Gerbang 1 (Bagian II).
2. `docs/design/*.md` — keputusan desain teknis (ADR). Mulai dari `0001-workspace-dan-indeks.md`. Untuk tampilan: `0003-ui-design-system.md` (token, komponen, keadaan yang tidak digambar), lalu `docs/mockups/*.html` (susunan layar dan teks antarmuka). Bila keduanya berbeda, `0003` yang benar. Aturan kerjanya di bagian 6.
3. `crates/patinote-index/migrations/*.sql` — skema indeks.
4. File ini.
5. Kode yang sudah ada.

Bila dua sumber bertentangan, atau spesifikasi tidak cukup untuk memutuskan, **berhenti dan tanya** (lihat bagian 11). Jangan menebak, dan jangan mengubah isi kebutuhan di `docs/PRD.md`.

## 2. Bahasa

- Kode, nama tipe dan fungsi, komentar kode, pesan commit, judul dan isi PR, teks UI, pesan CLI, dan dokumentasi publik (`README.md`, `--help`): **bahasa Inggris**.
- Dokumen perencanaan di `docs/` (PRD, dokumen desain, `QUESTIONS.md`): **bahasa Indonesia**, mengikuti dokumen yang sudah ada.
- Saat menjelaskan sesuatu kepada pemilik proyek di percakapan: bahasa Indonesia.

## 3. Status saat ini dan rencana Gerbang 1

Kita sedang mengerjakan **Gerbang 1 (daily driver)**. Kerjakan langkah sesuai urutan dan ketergantungannya. Satu langkah = satu branch = satu PR (pecah lagi bila PR terlalu besar). Jangan mengerjakan fitur Fase 3 ke atas.

### Fase 0 · Fondasi proyek

| Langkah | Isi | Bergantung pada |
| --- | --- | --- |
| 0.1 | (Pemilik) Repo GitHub dan nama crate diamankan | — |
| 0.2 | Workspace 4 crate, `rust-toolchain.toml` (stable, versi dikunci), edition 2024, `README.md`, `LICENSE` (MIT) | 0.1 |
| 0.3 | `docs/PRD.md`, `docs/design/0001-*.md`, `docs/design/0003-*.md`, `docs/mockups/` (HTML, `fonts.css`, `png/`), font di `crates/patinote-app/assets/fonts/`, `fixtures/vault-mockup/`, migrasi `0001_init.sql`, `docs/QUESTIONS.md` kosong. Semua file ini sudah disiapkan pemilik; cukup dimasukkan apa adanya | 0.2 |
| 0.4 | CI GitHub Actions: `fmt`, `clippy`, `test`, `cargo-deny`, pemeriksaan data vault; hook pre-commit | 0.2 |
| 0.5 | `tools/vaultgen` (vault uji 1.000/10.000/50.000 catatan), korpus uji emas parser (~40 kasus), kerangka benchmark `criterion` | 0.2 |
| 0.6 | Prototipe editor di `experiments/editor-proto/`: mode Vim minimal, pengukuran RAM (PSS) dan latensi mengetik dengan renderer tiny-skia vs wgpu, dan uji kemampuan tampilan editor (daftar di bawah). Hasil: `docs/design/0002-editor-dan-renderer.md` | 0.2 |

**Daftar uji tampilan untuk langkah 0.6.** Highlighter bawaan `text_editor` Iced, setidaknya di versi yang sudah dicek, hanya mengatur warna dan font per bagian teks. Prototipe harus menjawab, untuk setiap butir: bisa dengan widget bawaan, bisa dengan widget editor sendiri, atau tidak bisa.

1. Warna sintaks per bagian teks dengan font yang dibawa sendiri (`crates/patinote-app/assets/fonts/`), dimuat dari byte, bukan dari font sistem.
2. Latar per bagian teks: seleksi mode visual (`editor.selection`), semua hasil `/` (`editor.search.match`), hasil aktif (`editor.search.current`).
3. Latar satu baris penuh untuk baris kursor (`bg.cursorline`).
4. Garis bawah bergelombang berwarna untuk link ambigu (`editor.link.ambiguous`).
5. Kursor blok di mode normal dan visual, garis 2 px di insert, masing-masing dengan warna token.
6. Kolom nomor baris relatif yang tetap sejajar saat soft wrap, dengan baris aktif bernomor asli.
7. Kolom teks selebar maksimal 920 px di tengah jendela.
8. Screenshot headless dengan `iced_test` dan backend tiny-skia berjalan di CI.

`0002` mencatat hasil tiap butir dan pendekatan yang dipilih (widget bawaan, widget sendiri di atas `iced::advanced`, atau gabungan). Butir yang tidak bisa dipenuhi memakai cadangan di `0003` bagian 10, dan pemilik diberi tahu sebelum Fase 2 dimulai.

### Fase 1 · Core tanpa UI

| Langkah | Isi | Kebutuhan |
| --- | --- | --- |
| 1.1 | Model data dan parser; lolos korpus uji emas | F-02 |
| 1.2 | Pemindai vault dan pengecualian (`.patinote/ignore`) | F-01 |
| 1.3 | Trait `Store`, `SqliteStore`, migrasi, indeks inkremental, kunci tulis | F-03 |
| 1.4 | Resolusi link ala Obsidian (nama, path, alias, ambigu, belum ada) | F-02, F-03 |
| 1.5 | File watcher di core | F-04 |
| 1.6 | Jalur tulis: transaksi `plan`/`commit` dan jurnal | PRD "Gerbang 2: dampak arsitektur" poin 1 |
| 1.7 | Registri perintah dan CLI baca, `--json`, exit code | A-01, A-02, A-03 |
| 1.8 | Server MCP read-only dan `patinote agents init` | A-04, A-05 |
| 1.9 | Benchmark terhadap target performa | Bagian I, Performa |

### Fase 2 · Aplikasi desktop

| Langkah | Isi | Kebutuhan | Mockup | Bergantung pada |
| --- | --- | --- | --- | --- |
| 2.1 | Design system: `theme/tokens.rs` dan `theme/styles.rs` dari `docs/design/0003-ui-design-system.md`, pemuatan font dari `assets/fonts/`, kerangka snapshot UI `tests/ui_snapshots.rs` | — | 17 (lembar komponen) | 0.6 |
| 2.2 | Kerangka app: satu instance + socket, layar awal, mode minimal, bufferline, status line, baris perintah, konfigurasi dan state, `patinote open` | F-01 (layar awal), F-28 (sebagian) | 01, 10, 11 | 2.1 |
| 2.3 | Editor dan autosave, lalu mode Vim, termasuk banner simpan gagal dan read-only | F-05, F-06 | 01, 13, 17 | 0.6 |
| 2.4 | Quick switcher, autocomplete `[[`, mengikuti link, link ambigu | F-07, F-08 | 05, 06, 12 | 2.3 |
| 2.5 | Backlinks, outline, riwayat, buffer, pemulihan sesi | F-09 | 01, 02 | 2.2 |
| 2.6 | Explorer, buat/ganti nama/pindah/hapus lewat jalur tulis, dialog | F-10 | 02, 09, 14 | 1.6 |
| 2.7 | Watcher di UI, status indeks, banner perubahan dari luar, diff, daftar dan diff konflik Syncthing | F-03, F-04, F-11 | 08, 15, 16, 17 | 1.5 |
| 2.8 | Mode baca | F-27 | 07 | 2.3 |
| 2.9 | Mode `classic` dan `focus`, popup leader, `:layout` | F-28 | 02, 03, 04 | 2.2 |
| 2.10 | Pengukuran akhir dan paket instalasi pribadi | Performa | — | semua |

Nomor mockup menunjuk file `docs/mockups/NN-*.html`.

Gerbang 1 lolos setelah semua kriteria di Bagian II tercentang dan pemilik memakai Patinote dua minggu tanpa kehilangan data.

Acuan visual Fase 2 dan aturannya ada di bagian 6.

## 4. Alur kerja per tugas

1. **Baca** bagian PRD untuk kebutuhan yang dikerjakan (Bagian II, termasuk kriteria penerimaannya) dan dokumen desain terkait. Untuk pekerjaan UI, buka juga mockup yang disebut di baris **Desain** kebutuhan itu dan bagian `0003` yang dirujuk.
2. **Rencanakan** singkat: file yang disentuh, tipe dan fungsi baru, test yang akan ditulis. Bila rencana menyimpang dari dokumen desain, tanya dulu.
3. **Tulis test dulu** dari kriteria penerimaan. Beri nama yang menunjuk ID kriteria, misalnya `f03_4_kill_during_index_keeps_db_intact`.
4. **Implementasikan** sampai test lolos.
5. **Jalankan semua pemeriksaan** (bagian 8). Jangan membuka PR bila ada yang gagal.
6. **Centang kriteria** yang lolos di `docs/PRD.md` Bagian II (ubah `- [ ]` menjadi `- [x]` dan tambahkan nama test di akhir baris). Hanya kotak centang yang boleh diubah; teks kebutuhan tidak.
7. **Buka PR** dengan isi: langkah rencana (misalnya "1.3"), ID kebutuhan, daftar kriteria yang dipenuhi beserta nama test, cara memverifikasi manual bila ada, dan hal yang sengaja belum dikerjakan. PR yang mengubah tampilan juga menyertakan screenshot pembanding (bagian 6).

Kriteria yang hanya bisa diperiksa manual atau dengan vault asli: tulis langkah pemeriksaannya di PR dan biarkan kotaknya tidak dicentang. Pemilik yang mencentangnya.

## 5. Aturan arsitektur

**Crate dan arah dependensi**

```text
patinote-cli ──┐
               ├──► patinote-index ──► patinote-core
patinote-app ──┘
```

- `patinote-core`: model, parser, resolver, pemindai, watcher, jalur tulis, registri perintah, trait `Store`. **Tidak boleh** bergantung pada `rusqlite`, `iced`, atau crate UI apa pun.
- `patinote-index`: satu-satunya crate yang memakai SQLite. Mengisi trait `Store`.
- `patinote-cli`: binary `patinote` (CLI dan `patinote mcp`). **Tidak boleh** bergantung pada `iced`, `wgpu`, atau crate grafis.
- `patinote-app`: binary `patinote-app`. Satu-satunya crate yang memakai Iced. Modul `vim/` ditulis sebagai logika murni tanpa tipe Iced, agar bisa dites tanpa jendela.

**Vault**

- Semua penulisan ke vault lewat transaksi core (`plan` lalu `commit`) yang memeriksa hash dan mencatat jurnal. Tidak ada `std::fs::write` langsung ke vault di luar modul itu.
- Simpan secara atomik: file sementara di folder yang sama, lalu `rename`. Pertahankan izin file, akhir baris, BOM, dan newline akhir.
- Tidak pernah menulis ke `.obsidian/`. Tidak pernah menulis state, cache, atau log ke dalam vault. `.patinote/` hanya ditulis saat pengguna menyimpan pengaturan per vault; `.trash/` hanya saat menghapus.
- Hapus selalu ke `.trash/`, tidak pernah permanen.
- Membuka vault tidak boleh mengubah satu byte pun di dalamnya.

**Lokasi data (XDG)**

- Cache indeks: `$XDG_CACHE_HOME/patinote/<vault-id>/`.
- State dan jurnal: `$XDG_STATE_HOME/patinote/<vault-id>/`.
- Konfigurasi global: `$XDG_CONFIG_HOME/patinote/config.toml`; per vault: `<vault>/.patinote/config.toml`.
- Socket: `$XDG_RUNTIME_DIR/patinote/<vault-id>.sock`.

**Kontrak CLI dan MCP**

- Dengan `--json`, stdout hanya berisi satu dokumen JSON; log dan progres ke stderr.
- Tipe output didefinisikan di Rust, JSON Schema dibuat dengan `schemars` ke `docs/schema/`, dan dijaga dengan snapshot test (`insta`).
- Menambah field boleh. Mengubah atau menghapus field wajib menaikkan `"v"` perintah itu dan ditanyakan dulu.
- Exit code mengikuti tabel A-03; jangan menambah kode baru tanpa persetujuan.
- Tool MCP harus mengembalikan JSON yang identik dengan perintah CLI setaranya. Server MCP Gerbang 1 tidak boleh punya tool yang menulis ke vault.
- Perintah di UI, CLI, dan MCP berasal dari registri perintah yang sama, dengan nama bertitik yang stabil (`note.new`, `switcher.open`).

**Performa dan memori**

- UI tidak pernah menunggu: I/O, pengindeksan, dan pencarian di thread background.
- Jangan memuat seluruh vault ke RAM. Hanya catatan yang terbuka yang ada di memori.
- Bawa font sendiri; jangan memindai seluruh font sistem.
- Panel yang ditutup melepas datanya.
- Perubahan yang menyentuh jalur panas (parser, indeks, editor, quick switcher) disertai hasil benchmark sebelum/sesudah di PR.

## 6. Desain UI

**Sumber**

- `docs/design/0003-ui-design-system.md`: semua warna, huruf, ukuran, jarak, sudut, keadaan komponen, dan aturan untuk keadaan yang tidak digambar (bagian 7 dokumen itu). Bagian 2 berisi daftar mockup beserta kebutuhan yang diacu.
- `docs/mockups/*.html`: susunan layar dan teks antarmuka. Buka di browser dari dalam repo pada jendela 1440×860, skala 100% (mockup 07 dan 14 setinggi 1180); font dimuat dari `crates/patinote-app/assets/fonts/` lewat `fonts.css`, tanpa internet. Mockup 14 dan 17 adalah lembar komponen: beberapa keadaan digambar berdampingan, bukan satu layar.
- `docs/mockups/png/*.png`: screenshot referensi dari setiap mockup, dipakai sebagai pembanding.
- `fixtures/vault-mockup/`: vault sintetis yang isinya sama dengan contoh di mockup. Pemetaannya ada di `fixtures/README.md`.
- `90-target-final-phase6.html` adalah gambaran Fase 6. **Jangan** mengerjakan apa pun darinya di Gerbang 1.
- Isi catatan di mockup adalah contoh sintetis; jangan dipakai sebagai fixture yang dianggap data asli, dan jangan diganti dengan isi vault asli.

**Aturan**

- Semua warna, ukuran huruf, jarak, dan radius diambil dari `theme::tokens`. Tidak ada literal warna (`Color::from_rgb`, hex) atau angka ukuran di kode widget. Nama konstanta mengikuti nama token (`bg.app` → `BG_APP`).
- Gaya widget Iced dibuat di `theme/styles.rs` dari token. Widget memanggil gaya itu, tidak membuat gaya sendiri.
- Teks antarmuka (label tombol, judul, pesan, petunjuk tombol) disalin persis dari mockup atau PRD. Teks yang tidak ada di keduanya ditulis dalam bahasa Inggris dengan gaya yang sama (kalimat pendek, tanpa tanda seru) dan disebut di PR.
- Pesan baris perintah memakai tiga gaya: info, `W:` untuk peringatan, `E:` untuk error. Merah (`status.error`) hanya untuk kegagalan.
- Setiap elemen bisa dicapai dengan keyboard, dan petunjuk tombolnya terlihat seperti di mockup.
- Token baru hanya boleh ditambahkan lewat perubahan `0003` di PR yang sama, dengan angka kontras (minimal 4,5:1 untuk teks). Bila ragu, tanya.
- Layar atau keadaan yang tidak ada di mockup maupun `0003` bagian 7: berhenti dan tanya. Jangan merancang layar baru sendiri.

**Memeriksa hasil: snapshot UI**

- Screenshot aplikasi dibuat headless dengan `iced_test` di `crates/patinote-app/tests/ui_snapshots.rs`, memakai backend tiny-skia:

  ```bash
  ICED_TEST_BACKEND=tiny-skia cargo test -p patinote-app --test ui_snapshots
  ```

- Setiap test menyalin `fixtures/vault-mockup/` ke folder sementara (jangan pernah menulis ke fixture), menyiapkan keadaan yang sama dengan satu mockup (catatan terbuka, mode, panel, popup), lalu menyimpan PNG ke `target/ui-snapshots/` dengan nama yang sama dengan mockupnya, misalnya `05-quick-switcher.png`.
- Ukuran 1440×860 (1440×1180 untuk 07 dan 14) pada skala 1,0. Untuk layar yang diubah, buat juga versi skala 1,25 dan 2,0 (F-28.6).
- Di PR, letakkan screenshot aplikasi berdampingan dengan `docs/mockups/png/` yang sama. Yang harus sama: susunan, urutan elemen, teks, warna token, dan ukuran dari `0003` bagian 5. Perbedaan kecil karena render font boleh. Perbedaan lain yang disengaja ditulis beserta alasannya.
- Setelah pemilik menyetujui sebuah layar, PNG-nya disimpan sebagai baseline di `crates/patinote-app/tests/snapshots/` dan test membandingkan render berikutnya dengan baseline itu. Mengubah baseline harus disebut di PR.
- Screenshot hanya dari vault sintetis, tidak pernah dari vault asli pemilik. Pemeriksaan akhir di jendela Wayland sungguhan dilakukan pemilik.

**Bila desain berubah**

Kanvas sumber hanya bisa dibuka pemilik: <https://claude.ai/artifact/MGDPFvb1jgUhtQs6J5Pcd7>. Perubahan desain dilakukan pemilik di kanvas lalu diekspor ulang ke `docs/mockups/`, bersama perubahan `0003` bila nilainya berubah. Agent tidak mengubah file di `docs/mockups/`.

## 7. Privasi data uji (aturan keras)

Repo ini publik. Vault pemilik berisi catatan pribadi.

- **Jangan pernah** meng-commit isi vault asli, potongannya, nama catatannya, atau hasil yang mengandung isinya. Fixture hanya boleh sintetis (buatan `vaultgen`, `fixtures/vault-mockup/`, atau ditulis tangan).
- Test yang memakai vault asli ditandai `#[ignore]`, hanya berjalan bila `PATINOTE_REAL_VAULT` diset, dan hanya mencetak angka ringkasan (jumlah catatan, jumlah link, selisih), bukan isi atau nama catatan.
- CI tidak pernah menjalankan test dengan vault asli.
- Log, pesan error, dan panic boleh menyebut path relatif, tetapi tidak pernah isi catatan.
- Jangan membaca vault asli pemilik kecuali diminta secara eksplisit untuk tugas tersebut.
- Bila ragu apakah sesuatu mengandung data pribadi, jangan commit dan tanya.

## 8. Perintah pemeriksaan

Sebelum membuka PR, semua harus lolos:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

Perintah lain:

```bash
cargo run -p vaultgen -- --notes 10000 --out target/vaults/10k   # buat vault uji
cargo bench -p patinote-index                                       # benchmark indeks
cargo insta review                                                  # tinjau snapshot JSON
ICED_TEST_BACKEND=tiny-skia cargo test -p patinote-app --test ui_snapshots   # screenshot UI ke target/ui-snapshots/
PATINOTE_REAL_VAULT=~/Vault cargo test -- --ignored                 # hanya di laptop pemilik
```

Test mode Vim (F-06) membutuhkan `nvim` terpasang: hasil yang diharapkan dibuat dengan `nvim --headless` dan disimpan sebagai fixture. CI memasang Neovim.

## 9. Gaya kode

- Rust stable sesuai `rust-toolchain.toml`, edition 2024.
- Error: `thiserror` di library, `anyhow` hanya di binary. Tidak ada `unwrap()`/`expect()` di kode library kecuali di test atau dengan komentar yang membuktikan tidak mungkin gagal.
- Tidak ada `unsafe` tanpa persetujuan dan komentar `// SAFETY:`.
- Logging dengan `tracing`, keluaran ke stderr.
- Fungsi publik diberi doc comment singkat dalam bahasa Inggris.
- Utamakan kode sederhana dan eksplisit daripada abstraksi yang belum dibutuhkan.

**Dependensi**

Dependensi yang sudah disetujui: `pulldown-cmark`, `serde`, `serde_json`, `serde_yaml_ng` (lisensinya dicek `cargo-deny`), `ignore`, `notify`, `notify-debouncer-full`, `blake3`, `unicode-normalization`, `rusqlite` (fitur `bundled`), `rusqlite_migration`, `clap`, `clap_complete`, `schemars`, `rmcp`, `nucleo`, `iced` (versinya ditetapkan dan dikunci setelah langkah 0.6), `tracing`, `tracing-subscriber`, `thiserror`, `anyhow`, `toml` (file konfigurasi), `similar` (diff per baris untuk F-11), `rfd` dengan fitur `xdg-portal` dan tanpa GTK (dialog pilih folder F-01, hanya di `patinote-app`). Untuk test dan benchmark: `insta`, `criterion`, `tempfile`, `iced_test` (versinya mengikuti `iced`, hanya di `patinote-app`).

Dependensi baru di luar daftar ini harus dijelaskan di PR (alasan, ukuran, lisensi) dan ditanyakan dulu bila menambah ketergantungan besar atau native.

## 10. Git dan PR

- Jangan commit langsung ke `main`. Branch per langkah: `feat/1.3-sqlite-index`, `fix/f11-hash-check`.
- Pesan commit memakai Conventional Commits dalam bahasa Inggris, misalnya `feat: add incremental scan`.
- Pesan commit tidak boleh berisi baris `Co-Authored-By`.
- Satu PR untuk satu langkah atau satu kebutuhan. PR kecil lebih baik.
- CI harus hijau sebelum minta review. Jangan menonaktifkan lint, test, atau pemeriksaan CI agar lolos.
- Pemilik yang mereview dan me-merge.

## 11. Kapan berhenti dan bertanya

Berhenti dan tanya pemilik (lalu catat pertanyaannya di `docs/QUESTIONS.md`) bila:

- Spesifikasi ambigu, saling bertentangan, atau tidak mencakup kasus yang ditemui.
- Perlu mengubah skema indeks, kontrak JSON, exit code, atau struktur crate.
- Perlu dependensi besar atau native di luar daftar.
- Sebuah kriteria penerimaan tampak tidak mungkin dipenuhi (misalnya target performa), sertakan hasil pengukuran.
- Pekerjaan menyentuh data vault asli atau ada risiko privasi.
- Ingin mengerjakan sesuatu di luar langkah yang sedang dikerjakan.

## 12. Yang tidak boleh dikerjakan di Gerbang 1

- Live preview, embed yang menampilkan isi catatan, render LaTeX (Fase 6).
- Tool MCP atau perintah CLI yang menulis ke vault (A-06, Fase 3), selain lewat GUI.
- Sistem plugin, sinkronisasi sendiri, akun, atau panggilan jaringan apa pun.
- Tema terang, layar pengaturan, paket Flatpak/AppImage (Fase 8).
- Dukungan Windows/macOS yang khusus.

## 13. Definisi selesai

Sebuah langkah selesai bila:

- [ ] Semua kriteria penerimaan yang relevan punya test otomatis yang lolos, atau langkah pemeriksaan manual tertulis di PR.
- [ ] `fmt`, `clippy`, `test`, dan `cargo deny` lolos di CI.
- [ ] Kotak kriteria di `docs/PRD.md` dicentang dengan nama test.
- [ ] Tidak ada data vault asli di diff.
- [ ] Dokumen desain diperbarui bila ada keputusan teknis baru.
- [ ] Untuk perubahan tampilan: screenshot pembanding dengan mockup ada di PR, dan tidak ada literal warna atau ukuran di luar `theme::tokens`.
- [ ] PR menjelaskan apa yang dikerjakan, cara memverifikasi, dan apa yang belum.
