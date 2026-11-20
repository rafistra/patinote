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
**Jawaban:** (diisi pemilik)

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

## Q-010 · `chrono` tidak dipelihara lagi, dan `rmcp` membutuhkannya (langkah 1.8, 20 November 2026)

**Konteks:** `cargo deny` di CI gagal karena RUSTSEC-2026-0543: crate `chrono` dinyatakan tidak dipelihara lagi. Advisory ini informasional, bukan celah keamanan, dan tidak ada versi perbaikan. `chrono` adalah dependensi wajib `rmcp` 3.11 (server MCP, A-04), dipakai di 10 file sumbernya, sehingga tidak bisa dimatikan lewat fitur.
**Pilihan:**
- A. Abaikan advisory itu saja di `deny.toml` dengan alasan tertulis. Pemeriksaan advisory lain tetap berjalan. Ditinjau ulang saat `rmcp` berhenti memakai `chrono`.
- B. Tulis server MCP sendiri tanpa `rmcp` (JSON-RPC lewat stdio). `rmcp`, `tokio`, dan `chrono` hilang, tetapi perubahan protokol MCP harus diikuti sendiri.
- C. Kunci `cargo-deny` di CI ke 0.22, yang tidak menandai advisory ini untuk dependensi transitif.

Rekomendasi agent: A.
**Jawaban:** A, disetujui pemilik (20 November 2026).
