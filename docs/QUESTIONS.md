# Pertanyaan terbuka

Agent mencatat di sini setiap pertanyaan yang membuatnya berhenti (lihat `CLAUDE.md`, bagian 11). Pemilik menjawab di bawah pertanyaannya; jawaban yang mengubah kebutuhan dipindahkan ke `docs/PRD.md` atau dokumen desain.

Format:

```markdown
## Q-001 · Judul singkat (langkah 1.3, 20 November 2026)

**Konteks:** apa yang sedang dikerjakan dan apa yang tidak jelas.
**Pilihan:** A ..., B ...; rekomendasi agent dan alasannya.
**Jawaban:** (diisi pemilik)
```

## Q-001 · Isi "pemeriksaan data vault" di CI dan pre-commit (langkah 0.4, 20 November 2026)

**Konteks:** `CLAUDE.md` langkah 0.4 meminta pemeriksaan data vault, dan PRD (Risiko) menyebut pemeriksaan pre-commit agar isi vault pribadi tidak masuk repo. Isi pemeriksaannya tidak dirinci. CI tidak boleh membaca vault asli, jadi CI hanya bisa memeriksa aturan umum; pencocokan dengan vault asli hanya mungkin di laptop pemilik.
**Pilihan:**
- A. Lokasi file: file berbentuk vault (`.md`, `.canvas`, `.obsidian/`, `*.sync-conflict-*`, `.trash/`) hanya boleh ada di folder yang diizinkan (`docs/`, `fixtures/`, korpus uji, `README.md`, `CLAUDE.md`). Jalan di CI dan pre-commit.
- B. Fixture dibekukan: `fixtures/vault-mockup/` dijaga dengan daftar hash; mengubahnya harus memperbarui daftar itu dengan sengaja. Jalan di CI dan pre-commit.
- C. Path pribadi: menolak path absolut `/home/<nama>/` di file yang di-commit. Jalan di CI dan pre-commit.
- D. Pencocokan dengan vault asli, hanya di pre-commit dan hanya bila `PATINOTE_REAL_VAULT` diset: menolak commit bila file yang di-stage identik dengan file di vault asli, atau baris yang ditambahkan memuat nama catatan vault asli. Hanya mencetak path di repo dan jumlah kecocokan, tidak pernah nama atau isi catatan.

Rekomendasi agent: A, B, C, dan D. A sampai C murah dan menutup kesalahan yang umum; D satu-satunya yang benar-benar mengenali data pribadi, dan karena hanya berjalan di laptop pemilik, aturan CI di `CLAUDE.md` bagian 7 tetap terpenuhi.
**Jawaban:** A, B, C, dan D disetujui pemilik (20 November 2026), termasuk izin bagi hook pre-commit untuk membaca nama file dan hash vault asli bila `PATINOTE_REAL_VAULT` diset.

## Q-002 · Bentuk JSON korpus uji emas parser (langkah 0.5, 20 November 2026)

**Konteks:** F-02.1 meminta sekitar 40 file `.md`, masing-masing dengan `.json` hasil parse yang diharapkan. Tipe hasil parser baru dibuat di langkah 1.1, jadi bentuk JSON-nya harus ditetapkan sekarang dan nanti diikuti parser.
**Pilihan:**
- A. Tetapkan sekarang, mengikuti kolom skema `0001_init.sql`. Isinya: `title`; `aliases`; `links` (`kind`, `raw`, `target`, `subpath`, `display`, `line`, `byte_start`, `byte_end`); `tags` (`tag`, `display`, `source`, `line`); `headings` (`level`, `text`, `line`, `byte_start`); `blocks` (`block_id`, `line`); `properties` (`key`, `idx`, `type`, `value`); `warnings`. Aturannya: posisi byte dihitung dari byte file apa adanya (termasuk BOM dan frontmatter), nomor baris mulai dari 1 dihitung dari awal file, `slug` heading tidak ikut karena urusan resolver (1.4). Keputusan ini dicatat di `docs/design/0004-korpus-uji-parser.md`.
- B. Langkah 0.5 hanya membuat file `.md`; `.json` ditulis di langkah 1.1 bersama parsernya.

Rekomendasi agent: A. Bentuknya hampir seluruhnya mengikuti skema yang sudah diterima, dan korpus yang dibuat sebelum parser menjadi patokan yang independen, bukan sekadar menyalin hasil parser.
**Jawaban:** A, disetujui pemilik (20 November 2026).

## Q-003 · Lokasi benchmark (langkah 0.5, 20 November 2026)

**Konteks:** `0001` bagian 1 menggambar folder `benches/` di root repo, sedangkan `CLAUDE.md` bagian 8 menjalankan `cargo bench -p patinote-index`. Root repo adalah workspace tanpa paket, jadi tidak bisa punya `benches/` sendiri.
**Pilihan:**
- A. Benchmark di `crates/patinote-index/benches/` (dan nanti di crate lain bila perlu); gambar di `0001` bagian 1 diperbaiki.
- B. Membuat crate khusus `benches/` di root sebagai anggota workspace.

Rekomendasi agent: A, karena cocok dengan perintah di `CLAUDE.md` dan tidak menambah crate.
**Jawaban:** A, disetujui pemilik (20 November 2026).

## Q-004 · Renderer default: target RAM dan latensi tidak bisa dipenuhi bersamaan (langkah 0.6, 20 November 2026)

**Konteks:** Prototipe 0.6 diukur di laptop target (Iris Xe, Ubuntu 26.04.1, Wayland, layar 1920×1080 berskala 1,25, 48–60 Hz). Rinciannya di `docs/design/0002-editor-dan-renderer.md` bagian 4.

| Renderer | RAM idle (PSS) | Buka sampai frame pertama | Mengetik | Menahan `j` / seleksi besar |
| --- | --- | --- | --- | --- |
| tiny-skia | 28 MB | 137–179 ms | 1 frame | 120–220 ms per frame |
| wgpu (Vulkan) | 100–103 MB | 309–372 ms | 1 frame | 1 frame |

tiny-skia gagal F-06.6 (menahan `j` < 16 ms): merasterisasi satu layar penuh di skala 1,25 saja sudah 30 ms. wgpu gagal target RAM < 100 MB dan buka < 300 ms (F-28.8), padahal belum ada indeks, panel, atau vault yang dimuat.
**Pilihan:**
- A. tiny-skia default. RAM dan waktu buka jauh di bawah target; mengetik lancar. Target F-06.6 dilonggarkan untuk gulir layar penuh di HiDPI, atau dikejar dengan optimasi di Fase 2 (menelusuri kenapa jendela 4× lebih lambat dari headless).
- B. wgpu default. Semua gerakan mulus; target RAM dinaikkan (misalnya < 130 MB) dan target buka (misalnya < 400 ms).
- C. Keduanya ikut dalam binary, dipilih lewat konfigurasi (`renderer = "tiny-skia" | "wgpu"`), dengan default A, sehingga pengguna yang lebih mementingkan gulir mulus bisa beralih.

Rekomendasi agent: C dengan default tiny-skia. Target RAM dan waktu buka adalah alasan utama Patinote dibuat; yang tidak tercapai hanya gulir layar penuh, dan itu masih bisa dioptimasi. Iced memilih renderer saat start tanpa kode tambahan, dan wgpu tetap dibutuhkan untuk graf di Fase 7.
**Jawaban:** C dengan default tiny-skia, disetujui pemilik (20 November 2026). Kedua renderer ikut dalam binary; optimasi gulir layar penuh dikejar di Fase 2.

## Q-005 · Tempat menyimpan peringatan per file di indeks (langkah 1.3, 20 November 2026)

**Konteks:** Beberapa kriteria butuh peringatan per file yang tersimpan: F-01.4 (file Latin-1 "tercatat dengan peringatan"), F-01.5 (file tanpa izin baca "muncul di laporan error"), F-02.5 (frontmatter rusak "terindeks dengan peringatan"), catatan di atas 5 MB, dan A-01 `patinote vault info` ("peringatan"). Skema `0001_init.sql` tidak punya tempat untuk itu.
**Pilihan:**
- A. Tambah tabel `warnings (note_id → notes.id ON DELETE CASCADE, code TEXT, line INTEGER NULL)` langsung di `0001_init.sql`. Migrasi ini belum pernah dipakai di mesin mana pun, jadi mengubahnya tidak merusak apa pun.
- B. Tabel yang sama lewat migrasi baru `0002_warnings.sql`.
- C. Tidak disimpan: peringatan hanya ada di laporan `patinote index` saat itu, sehingga `vault info` dan perintah berikutnya tidak bisa menampilkannya.

Rekomendasi agent: A. Skema tetap satu file selama belum ada pengguna, dan `0001` bagian 4 cukup ditambah satu baris tabel. Kodenya: `not_utf8`, `too_large`, `unreadable`, `frontmatter_invalid`.
**Jawaban:** A, disetujui pemilik (20 November 2026). Langkah 1.3 dikerjakan dalam dua PR (1.3a penyimpanan, 1.3b pengindeks).

## Q-006 · `integrity_check` setiap indeks dibuka terlalu lambat (langkah 1.3, 20 November 2026)

**Konteks:** F-03 (Keamanan data) meminta indeks dibangun ulang bila gagal `PRAGMA integrity_check` saat dibuka. Diukur di laptop target dengan indeks vault 10.000 catatan (35 MB): `integrity_check` 861 ms, `quick_check` 555 ms, membuka tanpa pemeriksaan 0,9 ms. Biaya ini ditanggung setiap pemanggilan CLI dan setiap kali aplikasi dibuka, sehingga A-01.6 (`backlinks`, `read`, dan lainnya < 100 ms) dan F-28.8 (buka aplikasi < 300 ms) tidak mungkin tercapai.
**Pilihan:**
- A. Saat dibuka hanya pemeriksaan murah: file benar database SQLite dan versi skemanya cocok. Kerusakan di halaman data dideteksi lewat error `SQLITE_CORRUPT` yang dikembalikan SQLite saat query menyentuh halaman rusak; error itu memicu bangun ulang otomatis. Aplikasi menjalankan `integrity_check` lengkap di thread background sesekali (misalnya sekali sehari saat idle). F-03.6 tetap terpenuhi: database yang dirusak tetap dideteksi dan dibangun ulang.
- B. `quick_check` saat dibuka. Lebih cepat (555 ms), tetapi tetap melanggar kedua target.
- C. Tetap `integrity_check` saat dibuka, dan target A-01.6 serta F-28.8 dilonggarkan.

Rekomendasi agent: A. Indeks hanyalah cache yang aman dibangun ulang, jadi deteksi saat dipakai sudah cukup aman, dan target kecepatan adalah alasan utama Patinote dibuat. Teks F-03 di PRD perlu disesuaikan oleh pemilik bila A dipilih.
**Jawaban:** A, disetujui pemilik (20 November 2026). Teks F-03 di PRD disesuaikan oleh pemilik.

## Q-007 · File yang dikecualikan `.patinote/ignore` tetap jadi tujuan link (langkah 1.4, 20 November 2026)

**Konteks:** `0001` bagian 7 menyatakan file yang dikecualikan "tetap ada di disk, tetapi tidak muncul di pencarian, quick switcher, graf, dan unlinked mentions", dan "tetap dikenali sebagai tujuan link, sehingga link ke sana tidak dilaporkan rusak". Pemindai di langkah 1.2 membuang file yang cocok dengan `.patinote/ignore` sepenuhnya, jadi link ke file itu akan tercatat sebagai link ke catatan yang belum ada. Skema `notes` tidak punya kolom untuk menandai file yang dikecualikan.
**Pilihan:**
- A. Pemindai tetap mendaftar file yang cocok dengan `.patinote/ignore` dengan tanda `excluded`. Indeks menyimpannya di `notes` dengan kolom baru `excluded INTEGER NOT NULL DEFAULT 0` (diubah langsung di `0001_init.sql`, seperti Q-005), tanpa isi FTS, tag, atau link keluar. Resolver memakainya sebagai tujuan link; pencarian, quick switcher, dan daftar catatan melewatinya. Folder bawaan (`.obsidian/`, `.git/`, `.trash/`, folder tersembunyi) tetap dilewati sepenuhnya.
- B. File yang dikecualikan dibuang sepenuhnya seperti sekarang; link ke sana dilaporkan sebagai link ke catatan yang belum ada. `0001` bagian 7 diubah.

Rekomendasi agent: A, sesuai `0001` bagian 7. Pengguna biasanya mengecualikan folder arsip atau template yang tetap ditautkan dari catatan lain.
**Jawaban:** A, disetujui pemilik (20 November 2026).

## Q-008 · Exit code 130 untuk `Ctrl+C` (langkah 1.7, 20 November 2026)

**Konteks:** A-03 meminta exit code 130 saat dihentikan dengan `Ctrl+C`, dan A-03.5 meminta `Ctrl+C` di tengah `patinote index` keluar dengan 130 dengan indeks tetap utuh. Tanpa penangan sinyal, proses mati oleh SIGINT: shell melaporkannya sebagai 130, tetapi program lain yang menjalankan `patinote` (agent, skrip Python) melihat "dihentikan sinyal 2", bukan exit code 130. Indeks tetap utuh di kedua cara, karena setiap batch adalah satu transaksi. Memasang penangan sinyal sendiri butuh kode `unsafe` (`CLAUDE.md` bagian 9) atau dependensi baru.
**Pilihan:**
- A. Tambah dependensi `ctrlc` (MIT OR Apache-2.0, kecil, tanpa dependensi native). Penangannya menghentikan pengindeksan di batas batch, lalu keluar dengan exit code 130 sungguhan.
- B. Tanpa penangan: SIGINT mematikan proses seperti biasa. Shell tetap melihat 130, dan indeks tetap utuh; program pemanggil melihat sinyal 2.

Rekomendasi agent: A. Exit code 130 yang sungguhan sesuai dengan kontrak A-03 untuk agent dan skrip, dan pengindeksan berhenti dengan rapi di batas batch.
**Jawaban:** A, disetujui pemilik (20 November 2026). Kunci konfigurasi vault default: `[vault] default`.

## Q-009 · Pencarian kata yang sangat umum di 50.000 catatan sedikit di atas 50 ms (langkah 1.9, 20 November 2026)

**Konteks:** Target performa PRD: pencarian full-text < 50 ms pada 50.000 catatan. Diukur dengan `cargo bench -p patinote-index --bench targets` di laptop pemilik, pada vault `vaultgen` 50.000 catatan, 50 hasil teratas beserta body untuk nomor barisnya (seperti `patinote search`):

| Query | Catatan yang cocok | Median |
| --- | --- | --- |
| `kabut` | 28.727 (57%) | 52,0 ms |
| `dermaga pantai` | 16.769 (34%) | 42,6 ms |
| `"angin pilar"` (frasa) | 321 | 17,5 ms |

Hampir seluruh waktu untuk `kabut` (47 ms dari 52 ms) dipakai FTS5 menghitung skor bm25 untuk setiap catatan yang cocok sebelum bisa memilih 50 teratas, sekitar 1,6 µs per catatan. Yang sudah dicoba dan tidak membantu: `snippet()` (biayanya 1 ms), cache SQLite 16 MB (masih 47 ms), dan `optimize` FTS5 (masih 47 ms). Kosakata `vaultgen` kecil, jadi satu kata muncul di separuh vault. Di vault asli, kata umum seperti "yang" atau "dan" juga cocok dengan sebagian besar catatan.
**Pilihan:**
- A. Target berlaku untuk query biasa; query yang cocok dengan lebih dari separuh vault boleh sedikit di atasnya (sekitar 50 ms di laptop target). Di aplikasi, pencarian berjalan di thread background dengan debounce, jadi UI tidak tertahan.
- B. Bila hasil cocok melebihi batas (misalnya 10.000 catatan), urutkan menurut waktu ubah, bukan bm25, agar selalu di bawah 50 ms. Urutan hasil untuk query yang sangat umum jadi kurang relevan.
- C. Daftar stopword bahasa Indonesia dan Inggris di tokenizer FTS. Kata seperti "dan" tidak bisa dicari sama sekali, dan indeks harus dibangun ulang.

Benchmark target belum dijalankan di CI: runner GitHub lebih lambat dan tidak sama dengan laptop target, sehingga angkanya tidak bisa dibandingkan dengan target. CI hanya mengompilasinya (`cargo bench --workspace --no-run`). Pertanyaan tambahan: perlu runner khusus, atau cukup dijalankan di laptop pemilik sebelum rilis?

Rekomendasi agent: A, dan benchmark dijalankan di laptop pemilik untuk setiap PR yang menyentuh jalur panas (`CLAUDE.md` bagian 5). Selisihnya kecil, hanya terjadi pada query yang cocok dengan separuh vault, dan B maupun C menurunkan kualitas hasil.
**Jawaban:** A, mengikuti rekomendasi agent (21 November 2026). Target 50 ms berlaku untuk query biasa; query yang cocok dengan lebih dari separuh vault boleh sekitar 50 ms. Benchmark target dijalankan di laptop pemilik untuk setiap PR yang menyentuh jalur panas, tanpa runner CI khusus; CI tetap hanya mengompilasinya.

## Q-010 · `chrono` tidak dipelihara lagi, dan `rmcp` membutuhkannya (langkah 1.8, 20 November 2026)

**Konteks:** `cargo deny` di CI gagal karena RUSTSEC-2026-0543: crate `chrono` dinyatakan tidak dipelihara lagi. Advisory ini informasional, bukan celah keamanan, dan tidak ada versi perbaikan. `chrono` adalah dependensi wajib `rmcp` 3.11 (server MCP, A-04), dipakai di 10 file sumbernya, sehingga tidak bisa dimatikan lewat fitur.
**Pilihan:**
- A. Abaikan advisory itu saja di `deny.toml` dengan alasan tertulis. Pemeriksaan advisory lain tetap berjalan. Ditinjau ulang saat `rmcp` berhenti memakai `chrono`.
- B. Tulis server MCP sendiri tanpa `rmcp` (JSON-RPC lewat stdio). `rmcp`, `tokio`, dan `chrono` hilang, tetapi perubahan protokol MCP harus diikuti sendiri.
- C. Kunci `cargo-deny` di CI ke 0.22, yang tidak menandai advisory ini untuk dependensi transitif.

Rekomendasi agent: A.
**Jawaban:** A, disetujui pemilik (20 November 2026).

## Q-011 · Mencegah Iced memindai font sistem (langkah 2.1, 20 November 2026)

**Konteks:** Iced 0.14 selalu memanggil `fontdb::Database::load_system_fonts()` saat membuat database font; di laptop pemilik itu 513 font (183 MB). PRD (anggaran memori butir 3) meminta font sistem tidak dipindai. `fontdb` membaca folder dari `FONTCONFIG_FILE` bila variabel itu ada, dan kembali memindai `/usr/share/fonts` bila file itu tidak menyebut satu folder pun. Mengisi variabel dari dalam proses butuh `unsafe` di edition 2024 (`CLAUDE.md` bagian 9). Rinciannya di `docs/design/0002-editor-dan-renderer.md` bagian 5.1.
**Pilihan:**
- A. Saat start, `patinote-app` menulis file fontconfig yang hanya menyebut satu folder kosong di `$XDG_CACHE_HOME/patinote/fontconfig/`, lalu menjalankan ulang dirinya sekali (`exec`) dengan `FONTCONFIG_FILE` menunjuk file itu. Berlaku dari peluncur mana pun. Program luar yang dibuka aplikasi mendapat kembali `FONTCONFIG_FILE` milik pengguna. Huruf yang tidak ada di Inter dan JetBrains Mono (CJK, emoji) tampil sebagai kotak untuk sementara.
- B. Peluncur (`.desktop` dan `patinote open`) yang mengisi variabel itu. Menjalankan `patinote-app` langsung tetap memindai semua font.
- C. Pemindaian dibiarkan; aturan anggaran memori butir 3 dilonggarkan.

Rekomendasi agent: A.
**Jawaban:** A, disetujui pemilik (20 November 2026). Font cadangan untuk CJK atau emoji ditambahkan bila nanti dibutuhkan.

## Q-012 · Tiga dependensi Iced dinyatakan tidak dipelihara lagi (langkah 2.1, 20 November 2026)

**Konteks:** `cargo deny` di CI PR #19 gagal karena tiga advisory "unmaintained" (informasional, bukan celah keamanan, tanpa versi perbaikan): RUSTSEC-2026-0206 (`rustybuzz`), RUSTSEC-2026-0192 (`ttf-parser`), dan RUSTSEC-2024-0436 (`paste`). Ketiganya dependensi transitif `iced` 0.14: `ttf-parser` dipakai `cosmic-text`, `fontdb`, dan `winit` (inti render teks), `paste` lewat renderer, dan `rustybuzz` lewat `usvg`/`resvg` (fitur `svg` untuk ikon banner, juga dipakai `iced_tiny_skia`). Tidak ada yang bisa dihilangkan tanpa mengganti versi Iced. Di laptop pemilik `cargo-deny` 0.22 tidak menandainya; CI memakai versi lebih baru.
**Pilihan:**
- A. Abaikan ketiga advisory di `deny.toml` dengan alasan tertulis, seperti Q-010. Ditinjau ulang saat Iced naik versi.
- B. Ubah `unmaintained` di `deny.toml` menjadi `workspace` (hanya dependensi langsung yang diperiksa). Advisory serupa di masa depan tidak lagi menggagalkan CI, tetapi juga tidak terlihat.
- C. Kunci `cargo-deny` di CI ke 0.22, sama seperti laptop pemilik.

Rekomendasi agent: A. Pemeriksaan tetap ketat, dan setiap pengecualian punya alasan.
**Jawaban:** A, disetujui pemilik (20 November 2026).

## Q-013 · Bentuk kunci `layout` dan `renderer` di `config.toml` (langkah 2.2, 21 November 2026)

**Konteks:** PRD F-28 menulis `layout = "minimal"` (kunci di tingkat atas), dan F-28.12 memakai `layout = "classic"`. Bagian "Gerbang 2: dampak arsitektur" butir 3 menyebut bagian `[layout]`. Di TOML, `layout` tidak bisa sekaligus berupa string dan tabel. Q-004 juga meminta pilihan `renderer = "tiny-skia" | "wgpu"` tanpa menyebut bagiannya.
**Pilihan:**
- A. Kunci tingkat atas: `layout = "classic"` dan `renderer = "wgpu"`, persis seperti tulisan F-28 dan Q-004. Bagian `[layout]` tidak dipakai.
- B. Di dalam bagian: `[layout] default = "classic"` dan `[editor] renderer = "wgpu"`. F-28.12 perlu ditulis ulang.

Rekomendasi agent: A. Sudah diterapkan di PR langkah 2.2; bisa diubah bila pemilik memilih B.
**Jawaban:** A, mengikuti rekomendasi agent (21 November 2026). Kunci tingkat atas `layout` dan `renderer`; bagian `[layout]` tidak dipakai.

## Q-014 · Urutan memilih vault di aplikasi (langkah 2.2, 21 November 2026)

**Konteks:** F-01 menetapkan urutan untuk CLI (`--vault`, `PATINOTE_VAULT`, vault default di konfigurasi) dan menyebut aplikasi "membuka vault terakhir". Urutan lengkap untuk aplikasi tidak ditulis.
**Pilihan:**
- A. `--vault` (dari `patinote open`), lalu `PATINOTE_VAULT`, lalu vault terakhir, lalu `[vault] default`, lalu layar awal.
- B. Vault terakhir selalu menang atas `PATINOTE_VAULT` dan `[vault] default`.

Rekomendasi agent: A. Sudah diterapkan di PR langkah 2.2.
**Jawaban:** A, mengikuti rekomendasi agent (21 November 2026).

## Q-015 · Lisensi `nucleo` (langkah 2.4, 21 November 2026)

**Konteks:** `nucleo` ada di daftar dependensi yang disetujui (quick switcher F-07, autocomplete F-08), tetapi lisensinya MPL-2.0, yang tidak ada di daftar izin `deny.toml`. MPL-2.0 adalah copyleft lemah per file: kompatibel dengan MIT selama file `nucleo` dipakai tanpa diubah. Yang dipakai cukup `nucleo-matcher` (mesin pencocok `nucleo`, tanpa thread pool dan tanpa dependensi lain).
**Pilihan:**
- A. Izinkan MPL-2.0 hanya untuk `nucleo-matcher` lewat `[[licenses.exceptions]]`, dengan alasan tertulis.
- B. Izinkan MPL-2.0 untuk semua crate.
- C. Cari pengganti berlisensi MIT.

Rekomendasi agent: A.
**Jawaban:** A, disetujui pemilik (21 November 2026).

## Q-016 · Crate `image` untuk gambar di mode baca (langkah 2.8, 21 November 2026)

**Konteks:** Mode baca (F-27) menampilkan gambar. F-27.4 meminta kenaikan RAM tidak melebihi bitmap pada ukuran tampil (+10%), sehingga gambar 4000×3000 harus didekode lalu diperkecil ke lebar kolom sebelum diberikan ke Iced. Itu butuh crate `image` (Rust murni, MIT/Apache-2.0, sudah dipakai fitur `image` Iced) sebagai dependensi langsung.
**Pilihan:**
- A. Tambah `image` (hanya dekoder PNG, JPEG, GIF, WebP) dan fitur `image-without-codecs` Iced; gambar didekode dan diperkecil di thread background.
- B. Hanya fitur `image` Iced tanpa memperkecil; F-27.4 gagal.
- C. Gambar ditampilkan sebagai kotak nama file dulu.

Rekomendasi agent: A.
**Jawaban:** A, disetujui pemilik (21 November 2026).

## Q-017 · Gulir layar penuh dengan tiny-skia lambat di skala 1,25 (langkah 2.10, 21 November 2026)

**Konteks:** Pengukuran akhir (`0002` bagian 10) di laptop pemilik (Iris Xe, GNOME Wayland, skala 1,25, 48 Hz) dengan vault 10.000 catatan:

| | tiny-skia (bawaan) | wgpu |
| --- | --- | --- |
| Buka | 244–284 ms | 489–503 ms |
| RAM idle (PSS) | 82–83 MB | 157–165 MB |
| Menahan `j`, jarak antar-frame p50 | 173–182 ms | 20,8 ms |
| Mengetik, jarak antar-frame p50 | 20,8 ms | 20,8 ms |

Dengan tiny-skia, target buka (< 300 ms) dan RAM (< 100 MB) lolos, dan mengetik, mencari, serta seleksi visual tetap satu frame. Tetapi saat seluruh layar berubah (menahan `j`, `Ctrl+d`, berpindah catatan) satu frame butuh 170–180 ms. `draw` sendiri selesai dalam 5–6 ms, jadi waktunya habis di rasterisasi dan penayangan `softbuffer` pada skala pecahan. Dengan wgpu semuanya lancar, tetapi buka dan RAM melewati target. PRD tidak punya angka untuk gulir, tetapi aturan "UI tidak pernah menunggu" dan target latensi mengetik menunjukkan gulir yang tersendat ini tidak diharapkan.
**Pilihan:**
- A. Tetap tiny-skia sebagai bawaan untuk Gerbang 1. Selidiki jalur penayangan tiny-skia di skala pecahan sebagai langkah tersendiri (misalnya mengukur rasterisasi vs `softbuffer`, mencoba skala 1,0 atau 2,0, atau melapor ke Iced). Pengguna yang lebih suka gulir lancar bisa memasang `renderer = "wgpu"`.
- B. Ganti bawaan ke wgpu. Gulir lancar, tetapi target buka dan RAM PRD tidak terpenuhi.
- C. Ubah target RAM/buka di PRD untuk wgpu (perlu keputusan pemilik, agent tidak mengubah PRD).

Rekomendasi agent: A. Dua target PRD yang punya angka lolos dengan tiny-skia, dan gulir layar penuh hanya muncul saat seluruh layar berubah. Penyelidikan butuh waktu riset tersendiri sebelum bisa dijanjikan hasilnya.
**Jawaban:** A, disetujui pemilik (21 November 2026). tiny-skia tetap bawaan untuk Gerbang 1; penyelidikan jalur penayangan di skala pecahan dikerjakan sebagai langkah tersendiri.

## Q-018 · Menambal `iced_tiny_skia` agar gulir layar penuh satu frame (21 November 2026)

**Konteks:** Penyelidikan Q-017 (`0002` bagian 11). Gulir tersendat karena `iced_tiny_skia` 0.14.1 menggambar semua layer untuk setiap persegi damage, dan saat menggulir damage pecah menjadi 20–26 persegi. Menghapus latar ganda di aplikasi menurunkan menahan `j` dari 175 ms ke 58 ms per frame (p50). Sisa perbaikannya ada di Iced: menggabungkan damage menjadi satu persegi bila lebih dari 4 menurunkannya lagi ke 24 ms (p95 41 ms). Perubahannya sekitar 8 baris di `iced_tiny_skia/src/window/compositor.rs`. Branch `0.14` Iced belum memperbaikinya.
**Pilihan:**
- A. Bawa salinan `iced_tiny_skia` 0.14.1 dengan tambalan itu di `vendor/iced_tiny_skia/` lewat `[patch.crates-io]`, dan laporkan ke Iced (issue atau PR dengan angka pengukuran). Salinan dihapus begitu versi Iced yang dipakai sudah memperbaikinya. Lisensi MIT, tidak ada dependensi baru.
- B. Hanya laporkan ke Iced dan tunggu rilis berikutnya. Gulir tetap 58 ms per frame sampai itu.
- C. Biarkan; gunakan `renderer = "wgpu"` bila perlu gulir lancar.

Melapor ke repositori Iced berarti mengirim tulisan ke layanan luar atas nama pemilik, jadi agent tidak melakukannya tanpa persetujuan.

Rekomendasi agent: A. Tambalannya kecil dan mudah dibuang, dan dengan itu gulir mendekati satu frame tanpa mengorbankan target buka dan RAM.
**Jawaban:** A (21 November 2026). Salinan bertambal ada di `vendor/iced_tiny_skia/` (cara membuangnya di `README.md` folder itu). Dilaporkan ke Iced atas izin pemilik: [iced-rs/iced#3556](https://github.com/iced-rs/iced/issues/3556). Catatan: `master` Iced sudah mengganti `tiny_skia` dengan `vello_cpu`, jadi perbaikan hanya mungkin datang lewat rilis 0.14.x. Bila tidak ada, salinan ini dibuang saat Patinote pindah ke versi Iced berikutnya.

## Q-019 · Nama file NFD di disk (22 November 2026)

**Konteks:** PRD F-01: "Nama file dibandingkan dalam bentuk Unicode NFC; nama asli di disk tidak diubah." Pemindai menyimpan setiap path dalam NFC (`0001` bagian 3, tabel `notes`), lalu semua pembacaan dan penulisan memakai `root.join(path_nfc)`. Di Linux, `Cafe\u{301}.md` (NFD, sering muncul pada file dari macOS) dan `Café.md` (NFC) adalah dua file yang berbeda. Akibatnya catatan bernama NFD terindeks kosong, tidak bisa dibuka di aplikasi maupun `patinote read`, dan saat disimpan dari aplikasi muncul file NFC kedua di sebelahnya.
**Pilihan:**
- A. Simpan nama asli di disk di indeks: kolom baru `disk_path` di tabel `notes` (migrasi `0002`), diisi pemindai. Semua akses file (indeks, aplikasi, CLI, jalur tulis) memakai nama itu, sedangkan `path` NFC tetap menjadi kunci dan dipakai untuk perbandingan. Perlu mengubah skema indeks.
- B. Tanpa mengubah skema: satu fungsi core `on_disk(root, path)` yang mengembalikan path biasa bila file itu ada, dan bila tidak, mencari nama asli dengan membaca folder dan membandingkan dalam NFC. Semua akses file lewat fungsi itu. Jalur cepat tidak berubah; biayanya hanya muncul pada nama yang memang tidak NFC.
- C. Belum ditangani di Gerbang 1; dicatat sebagai keterbatasan.

Rekomendasi agent: B. Skema tidak berubah, perbaikannya terkumpul di satu fungsi, dan vault yang semua namanya NFC tidak terkena biaya tambahan. A lebih rapi bila nanti ada kebutuhan lain akan nama asli di indeks.
**Jawaban:** —
