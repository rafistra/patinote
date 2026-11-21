# PRD Patinote

> Diekspor dari dokumen PRD (Claude Docs) pada 20 November 2026. Dokumen ini adalah acuan implementasi. Bagian I berisi kebutuhan produk; Bagian II berisi rincian dan kriteria penerimaan Gerbang 1.
>
> Penulis: Rafi · Status: Gerbang 1 siap diimplementasikan.

---

# Bagian I · Kebutuhan produk

## Ringkasan

Patinote adalah aplikasi second brain native Rust untuk Ubuntu: pengganti Obsidian yang ringan, memakai vault Markdown yang sama, dengan mode Vim dan integrasi agent AI sejak awal.

**Masalah.** Obsidian berjalan di atas Electron (Chromium + Node.js), sehingga terasa berat di Ubuntu: startup lambat, RAM besar, dan mengetik kadang tersendat. Alternatif native yang kompatibel dengan vault Obsidian, ramah Vim, dan bisa dikendalikan agent AI lewat terminal belum ada.

**Solusi.** Aplikasi desktop dengan UI Iced (tanpa browser), indeks SQLite yang hanya berfungsi sebagai cache, dan satu core yang dipakai bersama oleh UI, CLI, dan server MCP.

**Status dokumen.** Disusun dari sesi perencanaan 13, 19, dan 20 November 2026. Gerbang 1 siap diimplementasikan: semua kebutuhannya punya rincian dan kriteria penerimaan di Bagian II. Gerbang 2 dan seterusnya dirinci menjelang fasenya.

## Tujuan dan non-tujuan

Tujuan utamanya: Patinote menggantikan Obsidian untuk seluruh alur kerja harian, dengan terasa instan dan tanpa kehilangan data.

**Tujuan**

1. Membuka dan mengedit vault Obsidian yang ada tanpa konversi, dan tetap bisa dibuka bolak-balik di Obsidian.
2. Terasa instan: buka aplikasi di bawah 300 ms dan mengetik tanpa jeda, di vault 10.000 catatan.
3. Bisa dipakai sepenuhnya dengan keyboard lewat mode Vim.
4. Setiap kemampuan bisa dipanggil dari CLI dan MCP, sehingga agent AI bekerja lewat jalur yang sama dengan aplikasi.
5. Dirilis sebagai proyek open source berlisensi MIT.

**Non-tujuan**

- Bukan tiruan Obsidian satu per satu. Fitur diambil dari yang benar-benar dipakai.
- Tidak menjalankan plugin Obsidian (JavaScript + Electron).
- Tidak menyediakan layanan sinkronisasi atau akun cloud sendiri.
- Tidak menargetkan Windows dan macOS di versi pertama, meski kode tidak sengaja dikunci ke Linux.

## Pengguna dan skenario

Pengguna utama adalah pembuatnya sendiri. Pengguna open source lain menjadi sasaran kedua setelah gerbang "pengganti Obsidian" terlewati.

| Pengguna | Ciri | Kebutuhan utama |
| --- | --- | --- |
| Pengguna utama | Ubuntu, pengguna Vim, punya vault Obsidian, memakai agent AI di terminal | Cepat, bisa dikendalikan keyboard, data tetap di file Markdown sendiri |
| Pengguna open source | Pengguna Linux yang merasa Obsidian berat | Instalasi mudah, impor vault sekali klik, dokumentasi |
| Agent AI | Claude Code, Codex CLI, Gemini CLI, atau agent lain lewat MCP | Perintah non-interaktif, output JSON stabil, penulisan yang aman |

**Skenario utama**

1. **Catatan harian.** Tekan pintasan, daily note terbuka, langsung menulis dalam mode insert, autosave.
2. **Menangkap ide cepat.** Dari aplikasi apa pun, pintasan global membuka jendela kecil; tulis, Enter, tersimpan ke inbox.
3. **Menelusuri pengetahuan.** Buka catatan dengan quick switcher, ikuti wikilink, lihat backlinks, kembali dengan riwayat navigasi.
4. **Menemukan kembali.** Cari full-text atau filter berdasarkan tag dan properti, langsung lompat ke baris yang cocok.
5. **Bekerja bersama agent.** Agent mencari, membaca, membuat, dan menautkan catatan lewat MCP, sementara link dan indeks tetap utuh.
6. **Berpindah perangkat.** Vault tersinkron lewat Syncthing; konflik terdeteksi dan bisa digabungkan.

## Prinsip produk

Tujuh prinsip ini menjadi penentu saat dua pilihan sama-sama masuk akal.

1. **Native, bukan browser.** UI dengan Iced, tanpa Electron atau WebView.
2. **Data milik pengguna.** File `.md` adalah sumber kebenaran; indeks hanyalah cache yang aman dihapus. Patinote hanya menulis ke vault saat catatan diubah, saat menghapus ke `.trash/`, dan saat pengguna menyimpan pengaturan di `.patinote/`.
3. **Kompatibel dengan Obsidian.** Wikilink, frontmatter, tag, dan struktur folder bisa dibuka bolak-balik.
4. **Keyboard dulu.** Setiap aksi bisa dicapai tanpa mouse, lewat mode Vim, `<leader>`, atau command palette.
5. **Siap untuk agent AI.** Setiap kemampuan core punya perintah CLI dan tool MCP.
6. **UI tidak pernah menunggu.** Pengindeksan, pencarian, dan I/O berjalan di background.
7. **Bayar hanya untuk yang dipakai.** Fitur berat seperti graf atau pencarian semantik baru bekerja saat dibuka.

## Kebutuhan fungsional

Ada 28 kebutuhan: 13 wajib untuk aplikasi yang bisa dipakai harian (Fase 1–2), 9 penting untuk menggantikan Obsidian (Fase 3–5), dan 6 untuk tahap lanjut (Fase 6–10). Kebutuhan agent AI ada di bagiannya sendiri.

| ID | Area | Kebutuhan | Prioritas | Fase |
| --- | --- | --- | --- | --- |
| F-01 | Vault | Membuka folder vault Obsidian apa adanya, menghormati folder tersembunyi dan pola pengecualian | Wajib | 1 |
| F-02 | Vault | Parser wikilink, alias, link heading dan blok, tag bersarang, frontmatter YAML | Wajib | 1 |
| F-03 | Indeks | Indeks SQLite inkremental di `~/.cache/patinote/`, bisa dihapus dan dibangun ulang | Wajib | 1 |
| F-04 | Indeks | Perubahan dari editor lain terdeteksi dalam kurang dari 1 detik | Wajib | 1 |
| F-05 | Editor | Editor Markdown dengan syntax highlighting dasar dan autosave | Wajib | 2 |
| F-06 | Editor | Mode Vim dasar: normal, insert, visual; gerakan inti, `f`/`t`, `/`; operator `d` `c` `y` `p` dengan objek kata, kutip, dan kurung; `.`; undo/redo; `:w` | Wajib | 2 |
| F-07 | Navigasi | Quick switcher fuzzy (`<leader>f` atau `Ctrl+Shift+O`) | Wajib | 2 |
| F-08 | Navigasi | Autocomplete `[[` dan pindah catatan lewat link | Wajib | 2 |
| F-09 | Navigasi | Panel backlinks dan outline, riwayat maju/mundur (`Ctrl+O`/`Ctrl+I`), buffer | Wajib | 2 |
| F-10 | File | Buat, ganti nama, pindah, hapus catatan dan folder; link ikut diperbarui saat ganti nama; hapus masuk ke `.trash/` | Wajib | 2 |
| F-11 | Sinkron | Peringatan saat file berubah dari luar; deteksi file `.sync-conflict-*` | Wajib | 2 |
| F-27 | Tampilan | Mode baca: Markdown dirender, dibuka lewat `<leader>r` | Wajib | 2 |
| F-28 | Tampilan | Tata letak: bufferline, status line dengan mode Vim, baris perintah, popup `<leader>`, panel explorer dan backlinks; tiga mode tampilan (minimal, classic, focus) | Wajib | 2 |
| F-12 | Linking | Rename lanjutan: pratinjau per perubahan, undo, deteksi rename dari luar (pembaruan link dasar sudah di F-10) | Penting | 3 |
| F-13 | Linking | Unlinked mentions, catatan yatim, dan link rusak | Penting | 3 |
| F-14 | Linking | Panel tag hierarkis dan editor properti frontmatter | Penting | 3 |
| F-15 | Cari | Pencarian full-text dengan operator `tag:`, `path:`, frasa, dan pengecualian | Penting | 4 |
| F-16 | Cari | Query ala Dataview (daftar, tabel, checklist) dan query tugas lintas vault | Penting | 4 |
| F-17 | Harian | Daily, weekly, monthly notes dan template dengan variabel | Penting | 5 |
| F-18 | Capture | Quick capture global di Ubuntu, terbuka dalam kurang dari 200 ms (di Wayland lewat pintasan keyboard GNOME yang menjalankan `patinote capture`, atau portal GlobalShortcuts) | Penting | 5 |
| F-19 | Perintah | Command palette (`Ctrl+P`) dan keymap kustom, termasuk `<leader>` | Penting | 5 |
| F-20 | Editor | Tempel gambar dari clipboard ke folder lampiran | Penting | 5 |
| F-21 | Editor | Live preview inline, embed `![[…]]`, callout, LaTeX, tabel dan checklist interaktif | Nanti | 6 |
| F-22 | Editor | Mode Vim lanjutan: objek teks lain (paragraf, kalimat), register, macro, visual blok, `:s`, file keymap | Nanti | 6 |
| F-23 | Visual | Graf lokal dan global, outline, timeline catatan | Nanti | 7 |
| F-24 | Desktop | Tema terang/gelap ikut sistem, tema TOML, paket `.deb`/AppImage/Flatpak, URI `patinote://` | Nanti | 8 |
| F-25 | Otomasi | API lokal (socket Unix), ekspor HTML/PDF/digital garden, terminal terintegrasi opsional | Nanti | 9 |
| F-26 | Modul | Pilihan Fase 10: Git, pencarian semantik, asisten AI, spaced repetition, canvas, enkripsi | Nanti | 10 |

**Wajib** = harus ada sebelum gerbang daily driver. **Penting** = harus ada sebelum gerbang pengganti Obsidian. **Nanti** = boleh dipilih dan diurutkan sesuai kebutuhan.

Rincian dan kriteria penerimaan per kebutuhan ada di Bagian II.

## Pengalaman pengguna dan tampilan

Tampilan disusun dari komponen yang sama dalam tiga mode. Acuannya di `docs/mockups/` (HTML statis, salinan dari [kanvas mockup](https://claude.ai/artifact/MGDPFvb1jgUhtQs6J5Pcd7)) untuk susunan layar, dan `docs/design/0003-ui-design-system.md` untuk nilai warna, huruf, dan ukuran. File `90-target-final-phase6.html` menggambarkan tampilan akhir sekitar Fase 6.

Ada 18 mockup: 01–09 untuk layar utama dan layar khusus, 10–17 untuk kondisi khusus (layar awal, tanpa catatan terbuka, link ambigu, mode visual, buat/pindah/hapus folder, konflik Syncthing, banner dan pesan), dan 90 untuk tampilan akhir. Daftar lengkap beserta kebutuhan yang diacu ada di `0003` bagian 2. Setiap kebutuhan di Bagian II menyebut mockupnya di baris **Desain**.

| Mode | Susunan | Mockup |
| --- | --- | --- |
| `minimal` (default) | Tanpa panel; bufferline, editor, status line, baris perintah | `01-minimal.html` |
| `classic` | Explorer dan panel samping terbuka; tab dengan tombol ×; panel samping memakai tab | `03-classic.html` |
| `focus` | Hanya kolom teks; status line diperkecil | `04-focus.html` |

**Tata letak (mode minimal)**

- Bufferline di atas, satu kolom teks di tengah (sekitar 100 karakter), status line dan baris perintah di bawah.
- Explorer di kiri (`<leader>e`); panel kanan berisi Outline dan Backlinks bertumpuk (`<leader>b`, `<leader>o`). "Belum ditautkan" menyusul di Fase 3 (F-13).
- Saat panel samping tertutup, backlinks tampil di bawah catatan.
- Popup pintasan muncul bila Spasi (`<leader>`) ditahan 300 ms.
- Mode diganti dengan `:layout` atau `<leader>l`; mode dan panel terakhir diingat.

**Mode Vim di UI**

- Badge mode di status line: NORMAL (hijau), INSERT (oranye), VISUAL (ungu), READ (biru), CONFLICT (oranye).
- Kursor blok di normal, garis di insert.
- Baris perintah `:` di bawah, seperti Neovim.
- Nomor baris relatif di semua baris, baris aktif menampilkan nomor aslinya; bisa dimatikan.

**Gaya visual**

- Fase 2 hanya tema gelap, aksen verdigris `#5CC4A8`, tembaga sebagai alternatif. Tema terang dan ikut sistem di Fase 8 (F-24).
- Bahasa antarmuka Inggris; isi catatan tetap dalam bahasa pengguna.
- Editor dan status line memakai JetBrains Mono; teks UI dan mode baca memakai Inter.
- Diff memakai oranye (hanya di disk) dan biru (hanya di versi pengguna), bukan merah-hijau, agar terbaca oleh pengguna buta warna.
- Banner muncul di bawah bufferline dalam tiga jenis: peringatan (oranye), error (merah), info (biru). Merah hanya untuk kegagalan; hapus ke `.trash/` memakai oranye karena bisa dipulihkan.
- Semua pasangan warna teks dan latar lolos kontras WCAG AA (minimal 4,5:1); warna tidak pernah menjadi satu-satunya penanda.

**Catatan fase.** Di Fase 2, editor menampilkan Markdown mentah yang hanya diwarnai; hasil render dilihat lewat mode baca (F-27). Live preview baru di Fase 6 (F-21).

## Integrasi agent AI

Agent AI tidak boleh mengedit file vault secara buta: semua baca dan tulis lewat core yang sama dengan aplikasi, sehingga link dan indeks tetap utuh.

| ID | Kebutuhan | Fase |
| --- | --- | --- |
| A-01 | Setiap kemampuan core punya perintah CLI `patinote` yang non-interaktif | 1 |
| A-02 | Opsi `--json`: stdout hanya berisi JSON, setiap output punya field versi `"v"`, error ke stderr | 1 |
| A-03 | Exit code tetap: 0 berhasil, 1 error umum, 2 argumen salah, 3 tidak ditemukan, 4 konflik atau ambigu, 5 indeks terkunci, 130 dihentikan; opsi `--strict` | 1 |
| A-04 | Server MCP read-only lewat stdio (`patinote mcp`, SDK `rmcp`) dengan 11 tool yang mencerminkan perintah CLI | 1 |
| A-05 | `AGENTS.md` di vault lewat `patinote agents init`, plus penunjuk untuk Claude Code dan Gemini CLI | 1 |
| A-06 | Tool tulis `create`, `append`, `rename`, `link` lewat core, dengan `--dry-run` yang menampilkan diff | 3 |
| A-07 | Izin baca dan tulis terpisah untuk agent, plus log aktivitas agent | 3 |
| A-08 | Semua perintah aplikasi (query, template, ekspor) tersedia di CLI dan MCP | 9 |

Perintah di UI, CLI, dan MCP berasal dari satu registri perintah di core, jadi ketiganya tidak pernah berbeda perilaku.

## Kebutuhan non-fungsional

Target performa diukur dengan benchmark otomatis di CI sejak Fase 0, memakai vault uji buatan generator.

**Performa**

| Metrik | Target | Ukuran vault |
| --- | --- | --- |
| Buka aplikasi (indeks sudah ada) | < 300 ms | 10.000 catatan |
| RAM saat idle (PSS) | < 100 MB | 10.000 catatan |
| Quick switcher dan backlinks | < 16 ms | 10.000 catatan |
| Pencarian full-text | < 50 ms | 50.000 catatan |
| Latensi mengetik | < 16 ms (1 frame) | catatan 5.000 baris |
| Indeks ulang penuh dari nol | < 10 detik | 10.000 catatan |
| Rename dengan pembaruan link | < 1 detik | 10.000 catatan |
| Buka quick capture | < 200 ms | — |

**Anggaran memori**

Arsitektur saja tidak menjamin RAM kecil: pemakai terbesar di aplikasi GUI Rust biasanya driver GPU, font, dan cache, bukan logika aplikasi. Karena itu RAM dijaga dengan aturan berikut.

1. **Renderer dipilih lewat pengukuran.** Prototipe editor membandingkan renderer perangkat lunak Iced (tiny-skia) dan wgpu di Intel Iris Xe: RAM idle dan latensi mengetik di layar HiDPI. Renderer yang lolos kedua target jadi default; wgpu dipakai untuk graf (Fase 7) bila perlu.
2. **Vault tidak dimuat ke RAM.** Hanya catatan yang terbuka ada di memori; sisanya di SQLite dengan `PRAGMA cache_size` dibatasi dan `mmap` dimatikan.
3. **Font dibawa sendiri.** Inter dan JetBrains Mono disertakan di aplikasi; font sistem tidak dipindai seluruhnya.
4. **Cache gambar dibatasi.** Lampiran di-decode hanya saat terlihat, diperkecil ke ukuran tampil, disimpan di cache LRU dengan batas ukuran.
5. **Panel yang ditutup melepas memori.** Graf, hasil pencarian, dan backlinks dibuang saat panelnya ditutup, bukan disembunyikan.
6. **Diukur di CI.** Benchmark mencatat PSS di vault uji 10.000 catatan; build gagal bila melewati batas.
7. **Allocator dievaluasi.** Bila RSS terus naik karena fragmentasi glibc, coba `mimalloc` atau `malloc_trim` saat idle.

Binary `patinote` (CLI dan MCP) tidak memuat GPU maupun font, sehingga pemanggilan oleh agent AI tetap ringan.

Rujukan: [laporan RAM Iced dengan renderer perangkat lunak](https://feddit.it/post/862651), [penjelasan memori driver GPU](https://dev.to/trystan_sarrade/how-i-took-my-rust-gui-from-135-mb-to-30-mb-by-ditching-the-gpu-2i7d), [diskusi r/rust tentang memori wgpu](https://redlib.hbubli.cc/r/rust/comments/17np5sl/rust_guis_why_are_modern_graphics_apis_so_heavy).

**Kompatibilitas Obsidian**

- Resolusi link mengikuti aturan Obsidian: nama file tanpa membedakan huruf besar-kecil, path relatif, alias, dan link ke catatan yang belum ada.
- Patinote tidak menulis apa pun ke `.obsidian/` dan tidak mengubah format file.
- Kompatibilitas diuji dengan vault Obsidian asli milik pengguna di setiap fase, hanya di komputer pengguna (lihat Data dan privasi).

**Data dan privasi**

- Tidak ada data yang keluar dari komputer kecuali pengguna mengaktifkannya (misalnya asisten AI dengan API eksternal).
- Indeks di `$XDG_CACHE_HOME/patinote/`, terpisah per vault, tidak ikut tersinkron.
- Penghapusan masuk ke `.trash/` di vault, bukan hapus permanen.
- State aplikasi (buffer terbuka, jurnal) di `~/.local/state/patinote/`; konfigurasi per vault di `.patinote/` di dalam vault.
- **Data uji.** Repo Patinote publik, jadi isi vault pengguna tidak pernah di-commit, dijadikan fixture, atau diproses di CI. Test dengan vault asli hanya berjalan di komputer pengguna, dipicu manual (misalnya `PATINOTE_REAL_VAULT=~/Vault cargo test -- --ignored`), dan hanya mencetak angka ringkasan, bukan isi catatan. CI memakai vault buatan generator dan korpus uji sintetis.
- Log, pesan error, dan laporan crash boleh memuat path, tetapi tidak pernah memuat isi catatan.

**Platform**

- Ubuntu 26.04 LTS dengan Wayland sebagai target utama, GPU Intel (Mesa/Vulkan) sebagai mesin uji pertama, layar HiDPI.
- Rust stable, UI Iced dengan versi dikunci.
- Semua dependensi lolos `cargo-deny` dengan lisensi yang kompatibel MIT.

## Rilis bertahap dan ukuran keberhasilan

Pengembangan dibagi menjadi 11 fase dalam tiga babak. Keberhasilan diukur di dua gerbang, bukan di tanggal rilis.

```text
Babak 1 · Fondasi      Fase 0 Fondasi proyek → Fase 1 Core headless → Fase 2 MVP UI Iced
                          ◆ Gerbang 1 · Daily driver: dipakai 2 minggu menggantikan Obsidian tanpa kehilangan data
Babak 2 · Kematangan   Fase 3 Linking matang → Fase 4 Pencarian dan query → Fase 5 Capture dan harian
                          ◆ Gerbang 2 · Pengganti Obsidian: semua alur harian tanpa Obsidian; quick capture < 200 ms
Babak 3 · Ultimate     Fase 6 Editor lanjutan · Fase 7 Visualisasi · Fase 8 Polishing Ubuntu ·
                       Fase 9 Otomasi dan agent · Fase 10 Modul pilihan (urutan bebas)
```

- **Gerbang 1 (daily driver)** butuh F-01 sampai F-11, F-27, F-28, dan A-01 sampai A-05.
- **Gerbang 2 (pengganti Obsidian)** butuh F-12 sampai F-20, A-06, A-07, dan semua target performa untuk vault 10.000 catatan.
- Fase baru tidak dimulai sebelum kriteria selesai fase sebelumnya terpenuhi; rinciannya ada di roadmap.

## Gerbang 2: dampak arsitektur

Rincian dan kriteria penerimaan Gerbang 2 ditulis setelah dua minggu uji daily driver. Bagian ini hanya menetapkan keputusan yang harus ada sejak Gerbang 1 agar Gerbang 2 tidak memaksa bongkar ulang. Status: keempat poin disetujui 20 November 2026.

**1. Jalur tulis dan jurnal undo (untuk F-10, F-12, A-06, A-07)**

- Semua penulisan ke vault lewat satu jalur di core: transaksi berisi daftar perubahan file (buat, ubah, pindah, hapus ke `.trash/`), masing-masing dengan hash yang diharapkan sebelum diubah.
- `plan()` menghasilkan diff tanpa menulis apa pun; dipakai untuk dialog konfirmasi (F-10) dan `--dry-run` agent (A-06).
- `commit()` memeriksa semua hash dulu; bila satu file sudah berubah, tidak ada yang ditulis (konflik, seperti F-11). Bila I/O gagal di tengah, file yang sudah ditulis dikembalikan dari jurnal.
- Setiap transaksi dicatat di jurnal `~/.local/state/patinote/<vault-id>/journal/` beserta isi file sebelum diubah dan pelakunya (`user`, `cli`, `agent:<nama>`). Retensi: semua transaksi 7 hari terakhir, dan minimal 100 transaksi terakhir walau lebih lama, dengan batas keras 100 MB. Jurnal ada di disk dan tidak dimuat ke RAM kecuali saat undo atau membuka log.
- Jurnal yang sama menjadi dasar undo rename (F-12) dan log aktivitas agent (A-07). Di Fase 2 jurnal sudah ditulis oleh rename dan hapus, walau tombol undo baru ada di Fase 3.

**2. Satu instance aplikasi dan socket lokal (untuk `patinote open`, F-18, F-25)**

- `patinote-app` berjalan sebagai satu instance per vault dan mendengarkan socket Unix di `$XDG_RUNTIME_DIR/patinote/<vault-id>.sock`.
- `patinote open <catatan>` (Fase 2) mengirim pesan ke aplikasi yang sedang berjalan; bila tidak ada, menjalankan aplikasi.
- `patinote capture` (Fase 5) memakai socket yang sama, sehingga jendela capture muncul seketika. Bila aplikasi belum berjalan, `patinote-app --capture` membuka jendela capture saja tanpa memuat panel dan indeks penuh.
- Socket ini kelak menjadi API lokal di Fase 9 (F-25).

**3. Konfigurasi, state, dan cache (XDG)**

| Jenis | Lokasi | Ditulis oleh | Isi |
| --- | --- | --- | --- |
| Konfigurasi global | `~/.config/patinote/config.toml` | Pengguna | Editor, mode tampilan default, keymap (Fase 5), tema (Fase 8) |
| Konfigurasi per vault | `.patinote/config.toml` di dalam vault, ikut tersinkron Syncthing | Pengguna | Folder catatan baru, folder lampiran, symlink, aturan khusus vault; pengecualian indeks di `.patinote/ignore` |
| State | `~/.local/state/patinote/<vault-id>/` | Aplikasi | Buffer terbuka, posisi kursor, lebar panel, mode terakhir, catatan terakhir dibuka, jurnal |
| Cache | `~/.cache/patinote/<vault-id>/` | Aplikasi | Indeks SQLite |

- Urutan prioritas: opsi CLI, lalu `.patinote/config.toml` vault, lalu konfigurasi global, lalu nilai bawaan.
- `.patinote/` hanya dibuat saat pengguna menyimpan pengaturan per vault; tidak pernah otomatis saat vault dibuka. Folder ini termasuk folder tersembunyi, jadi tidak diindeks.
- State dan cache tidak pernah ditulis ke vault, karena isinya khusus satu perangkat dan akan memicu konflik Syncthing.
- Konfigurasi punya `config_version = 1`; kunci yang tidak dikenal menghasilkan peringatan, bukan error.
- Bagian konfigurasi: `[editor]`, `[layout]`, `[notes]`, `[vault]`, lalu `[keys]` (Fase 5) dan `[theme]` (Fase 8).
- Setiap perintah di registri punya nama bertitik yang stabil (`note.new`, `switcher.open`, `layout.cycle`). Nama yang sama dipakai keymap, perintah `:`, command palette (F-19), dan kelak MCP.

**4. Skema indeks untuk F-15 dan F-16**

- F-16 menambah tabel `tasks` (catatan, baris, status, teks) lewat migrasi `0002` di Fase 4; tabel `properties` yang sudah dipecah per nilai cukup untuk filter properti.
- Karena pengguna tidak memakai plugin Dataview, sintaks query boleh dirancang sendiri yang lebih sederhana; diputuskan di Fase 4.
- Operator pencarian (F-15) tercakup oleh FTS5, dengan opsi beralih ke tantivy di belakang trait `SearchBackend`.

## Di luar ruang lingkup

Hal berikut sengaja tidak dikerjakan. Masing-masing bisa dipertimbangkan ulang hanya lewat perubahan PRD ini.

- **Aplikasi mobile.** Tidak ada pendamping Android atau iOS.
- **Sistem plugin.** Ditunda. Fondasinya registri perintah di Fase 1. Bila kelak dibutuhkan, urutannya Lua (`mlua`), lalu proses eksternal lewat API lokal dan MCP, lalu WASM (`wasmtime`). Library dinamis `.so` tidak dipakai.
- **Plugin Obsidian.** Tidak bisa dijalankan; fitur yang dibutuhkan dibangun langsung ke inti.
- **Server sinkronisasi dan kolaborasi real-time.** Sinkron memakai Syncthing, riwayat memakai Git lokal.
- **Windows dan macOS** di versi pertama.

## Asumsi, risiko, dan mitigasi

Risiko terbesar adalah editor: live preview dan mode Vim di atas Iced mungkin butuh widget editor buatan sendiri.

**Asumsi**

- Proyek dikerjakan satu orang dengan bantuan penuh agent AI, tanpa tenggat eksternal.
- Vault pengguna utama bisa dipakai sebagai data uji kompatibilitas, hanya di komputer pengguna.
- Iced cukup matang untuk aplikasi desktop harian (desktop COSMIC dibangun di atasnya).

**Risiko**

| Risiko | Dampak | Mitigasi |
| --- | --- | --- |
| Widget `text_editor` Iced belum setara CodeMirror | Live preview dan mode Vim tertunda atau harus dibangun dari nol | Prototipe editor sebagai eksperimen terpisah sejak awal, sebelum Fase 2; sekaligus mengukur RAM renderer tiny-skia dan wgpu |
| API Iced masih berubah (belum 1.0) | Kode UI perlu disesuaikan tiap rilis | Kunci versi; kode UI hanya di crate `patinote-app` |
| Kasus tepi kompatibilitas Obsidian | Link rusak atau salah resolusi | Korpus uji sintetis di CI; test dengan vault asli dijalankan lokal di setiap fase |
| Kode buatan AI sulit dipahami dan dirawat | Bug tersembunyi; arsitektur melenceng dari desain | Dokumen desain dan `CLAUDE.md` sebagai acuan agent; test dan CI wajib lulus; setiap perubahan direview |
| Cakupan membengkak | Proyek tidak pernah mencapai daily driver | Tidak masuk fase baru sebelum kriteria fase sebelumnya terpenuhi |
| File `.sync-conflict-*` Syncthing menumpuk | Isi catatan terpecah | Deteksi otomatis, daftar konflik, diff dan pilihan versi (F-11); gabung per bagian di Fase 3 |
| Batas inotify Ubuntu terlampaui | Perubahan file tidak terdeteksi tanpa error | Cek `max_user_watches` saat startup, sarankan `sysctl`, fallback pemindaian berkala |
| Folder `.git` ikut tersinkron Syncthing | Repositori riwayat rusak | Buat `.stignore` otomatis yang mengecualikan `.git` |
| Isi vault pribadi masuk ke repo publik | Catatan pribadi terpublikasi | Aturan data uji di bagian Data dan privasi; `CLAUDE.md` melarang commit data vault; pemeriksaan pre-commit |

Risiko sinkronisasi dan Git lainnya dirinci di roadmap.

## Keputusan yang sudah dijawab

| Pertanyaan | Jawaban | Dampak |
| --- | --- | --- |
| Pengalaman Rust dan waktu tersedia | Pengembangan sepenuhnya dibantu AI | Fase bisa lebih cepat; risiko bergeser ke menjaga kode tetap dipahami dan teruji |
| Kondisi vault saat ini | Kurang dari 200 catatan, beberapa lampiran gambar | Target performa mudah tercapai untuk pengguna utama; target 10.000–50.000 catatan tetap dipakai untuk pengguna open source |
| Plugin Obsidian | Belum memakai plugin | Tidak ada fitur plugin yang wajib dibangun ke inti |
| Bahasa antarmuka | Inggris | Semua teks UI, pesan CLI, dan dokumentasi publik dalam bahasa Inggris |
| Lingkungan Ubuntu | Ubuntu 26.04.1 LTS, Wayland, GPU Intel Iris Xe (Alder Lake) | Wayland jadi target utama pengujian; Iced merender lewat Vulkan dengan driver Mesa bawaan; quick capture global harus lewat pintasan GNOME atau portal |
| Binary | Dua binary dalam satu paket: `patinote` (CLI dan MCP) dan `patinote-app` (GUI) | CLI tetap kecil dan bisa dipasang tanpa layar; `patinote open` membuka GUI dari terminal |
| Struktur crate | Empat crate; core mendefinisikan trait `Store`, `patinote-index` mengisinya dengan SQLite | Core bisa dites tanpa database; modul `vim` di app ditulis sebagai logika murni yang bisa dites |
| Pengecualian indeks | Aturan bawaan + `.patinote/ignore` opsional; `.gitignore` tidak dipatuhi | Folder sistem (`.obsidian/`, `.git/`, `.trash/`, data Syncthing) selalu dilewati; lampiran tetap dikenali sebagai tujuan link; file konflik Syncthing masuk daftar konflik |

Pertanyaan baru yang muncul saat pengembangan dicatat di `docs/QUESTIONS.md`, lalu dipindah ke tabel ini setelah dijawab.

---

# Bagian II · Spesifikasi Gerbang 1

Rincian dan kriteria penerimaan untuk kebutuhan yang harus selesai sebelum gerbang daily driver (F-01 sampai F-11, F-27, F-28, A-01 sampai A-05). Setiap kriteria harus bisa dites otomatis atau diperiksa dengan satu langkah manual yang jelas; centang saat lolos. Kriteria yang memakai vault asli pengguna hanya dijalankan di komputer pengguna, tidak pernah di CI, dan tidak pernah menyimpan isi catatan ke repo (lihat "Data dan privasi" di Bagian I).

Baris **Desain** di setiap kebutuhan menunjuk mockup di `docs/mockups/` dan bagian dokumen `docs/design/0003-ui-design-system.md` yang berlaku. Mockup menentukan susunan dan teks antarmuka; `0003` menentukan nilai warna, huruf, dan ukuran, serta keadaan yang tidak digambar. Bila keduanya berbeda, `0003` yang benar. Kebutuhan tanpa baris Desain tidak punya tampilan.

Kriteria yang menyebut "screenshot pembanding" diperiksa dengan snapshot UI headless dari vault sintetis `fixtures/vault-mockup/`, dibandingkan dengan `docs/mockups/png/` (caranya di `CLAUDE.md`, bagian 6). Pemeriksaan akhir di jendela Wayland sungguhan dilakukan pengguna.

### F-01 · Membuka vault

Patinote membuka folder vault Obsidian apa adanya, tanpa mengubah atau menambah file apa pun saat membuka.

**Desain:** `10-welcome-and-missing-vault.html`.

**Apa itu vault**

- Folder apa saja bisa jadi vault; `.obsidian/` tidak wajib ada.
- Subfolder yang punya `.obsidian/` sendiri diperlakukan sebagai folder biasa.

**Memilih vault**

- CLI, dari prioritas tertinggi: opsi `--vault <path>`, variabel lingkungan `PATINOTE_VAULT`, lalu vault default di file konfigurasi.
- Aplikasi: membuka vault terakhir; bila belum ada, menampilkan layar awal dengan tombol pilih folder (dialog folder sistem lewat portal). Bila folder vault terakhir tidak ada lagi, tampil layar "Vault not found" dengan pilihan coba lagi atau pilih folder lain; tidak ada yang dibuat atau dihapus (mockup `10-welcome-and-missing-vault.html`).

**Jenis file**

| Jenis | Aturan | `kind` di indeks |
| --- | --- | --- |
| `.md` | Di-parse sebagai catatan | `note` |
| `.canvas` | Didaftar, isi belum dibaca | `canvas` |
| File lain apa pun | Didaftar sebagai lampiran, isi tidak dibaca | `attachment` |

**Aturan membaca**

- Tidak menulis apa pun ke vault saat membuka. Cache dan state tidak pernah di dalam vault; `.patinote/` baru dibuat saat pengguna menyimpan pengaturan per vault, dan `.trash/` saat ada penghapusan.
- Symlink tidak diikuti secara default (bisa diaktifkan lewat konfigurasi).
- Encoding UTF-8; BOM diabaikan. File yang bukan UTF-8 tetap didaftar dengan peringatan dan tidak di-parse.
- CRLF dan LF dibaca sama; saat menyimpan, gaya akhir baris asli dipertahankan.
- Nama file dibandingkan dalam bentuk Unicode NFC; nama asli di disk tidak diubah. Nama yang hanya beda huruf besar-kecil (`Rapat.md`, `rapat.md`) tetap dicatat keduanya; link ke keduanya ditandai ambigu.
- Catatan di atas 5 MB tetap didaftar, tetapi tidak di-parse dan tidak masuk pencarian.
- File yang tidak bisa dibaca (izin ditolak, rusak) dilaporkan; pemindaian tetap berjalan.
- Pengecualian mengikuti aturan bawaan dan `.patinote/ignore` (dokumen desain 0001, bagian 7).

**Error**

| Kondisi | Exit code |
| --- | --- |
| Folder vault tidak ada | 3 |
| Path menunjuk file, bukan folder | 2 |
| Tidak ada vault yang ditentukan sama sekali | 2 |

**Kriteria penerimaan**

- [ ] F-01.1 Vault Obsidian asli pengguna terbuka; jumlah catatan dan lampiran sama dengan yang terlihat di Obsidian.
- [x] F-01.2 Hash semua file vault identik sebelum dan sesudah `patinote index`. `f01_2_indexing_does_not_change_the_vault`
- [x] F-01.3 File di `.obsidian/`, `.git/`, `.trash/`, dan folder tersembunyi lain tidak muncul di `patinote list`. `f01_3_hidden_folders_are_not_listed`
- [x] F-01.4 File berisi teks Latin-1 tercatat dengan peringatan, tanpa crash. `f01_4_latin1_note_is_recorded_with_a_warning`
- [x] F-01.5 File tanpa izin baca muncul di laporan error; file lain tetap terindeks. `f01_5_unreadable_file_is_reported_and_others_indexed`
- [x] F-01.6 Vault kosong menghasilkan indeks kosong tanpa error. `f01_6_empty_vault_gives_an_empty_index`
- [x] F-01.7 Symlink ke folder di luar vault tidak diikuti; symlink yang membentuk loop tidak membuat pemindaian macet. `f01_7_symlinks_are_not_followed_and_loops_do_not_hang`
- [x] F-01.8 Catatan 6 MB terdaftar tetapi tidak muncul di hasil pencarian isi. `f01_8_a_6_mb_note_is_listed_but_not_found_by_search`
- [x] F-01.9 `--vault` mengalahkan `PATINOTE_VAULT`, yang mengalahkan vault default di konfigurasi. `f01_9_vault_flag_beats_env_beats_config`

### F-02 · Parser catatan

Setiap catatan dibaca menjadi data terstruktur (link, tag, heading, blok, properti) dengan hasil yang sama seperti Obsidian membacanya.

**Wikilink dan embed**

| Bentuk | Arti |
| --- | --- |
| `[[Catatan]]` | Link biasa |
| `[[Catatan\|teks]]` | Link dengan teks tampilan |
| `[[Catatan#Heading]]`, `[[Catatan#Heading#Sub]]` | Link ke heading, termasuk bertingkat |
| `[[Catatan#^abc123]]` | Link ke blok |
| `[[#Heading]]` | Heading di catatan yang sama |
| `[[folder/Catatan]]`, `[[Catatan.md]]` | Dengan path atau ekstensi |
| `![[Catatan]]`, `![[foto.png\|300]]` | Embed, dengan ukuran opsional |
| `[teks](Catatan.md)`, `[teks](Catatan%20Baru.md)` | Link Markdown ke catatan, termasuk yang di-encode URL |
| `proyek: "[[Patinote]]"` di frontmatter | Link dari properti, jenis `frontmatter` |

Di dalam tabel Markdown, pipa pada wikilink ditulis dengan escape (garis miring terbalik sebelum `|`) dan tetap dikenali sebagai pemisah teks tampilan.

**Tag**

- Bentuk `#tag` dan `#tag/bersarang`; karakter: huruf (termasuk non-Latin), angka, `_`, `-`, `/`.
- Minimal satu karakter bukan angka: `#2026` bukan tag, `#y2026` adalah tag.
- Pencocokan tidak membedakan huruf besar-kecil; ejaan asli disimpan untuk tampilan.
- Bukan tag: heading (`# Judul`), anchor URL (`https://x.com#bagian`), `#` di dalam kode.

**Heading dan blok**

- Heading ATX (`#` sampai `######`) dan setext (`===` atau `---` di bawah judul).
- ID blok `^abc-123` di akhir paragraf atau item daftar; karakter `A-Z a-z 0-9 -`.

**Frontmatter**

- YAML di antara `---` pada baris paling awal file.
- YAML rusak: catatan tetap terindeks, properti kosong, muncul peringatan.
- Tipe nilai: teks, angka, boolean, tanggal (`2026-11-19`), tanggal-waktu, daftar.
- Properti khusus `tags` dan `aliases`, sebagai daftar atau teks dipisah koma. Bentuk lama `tag` dan `alias` tetap dibaca, tetapi Patinote tidak pernah menulisnya.

**Area yang tidak dibaca link dan tag-nya**

- Blok kode (pagar dan indentasi), kode inline, rumus matematika (`$...$`, `$$...$$`).
- Komentar `%%...%%`.
- Teks yang di-escape: `\[[` dan `\#`.

Sintaks yang hanya untuk tampilan (`==highlight==`, callout, footnote) cukup tidak merusak parser di Fase 1; tampilannya dikerjakan di Fase 2 dan 6.

**Kriteria penerimaan**

- [x] F-02.1 Korpus uji emas sekitar 40 file `.md`, masing-masing dengan `.json` hasil parse yang diharapkan; setiap baris tabel wikilink dan setiap aturan tag punya minimal satu kasus positif dan satu kasus negatif. Semua lolos. `f02_1_golden_corpus`
- [ ] F-02.2 Semua catatan di vault asli pengguna ter-parse tanpa error.
- [ ] F-02.3 Jumlah link keluar per catatan sama dengan yang dilaporkan Obsidian; setiap selisih dijelaskan dan dicatat.
- [x] F-02.4 Link di dalam blok kode, kode inline, rumus, dan komentar `%%` tidak terhitung. `f02_4_links_in_code_math_comments_are_ignored`
- [x] F-02.5 Catatan dengan frontmatter rusak tetap terindeks dengan peringatan. `f02_5_broken_frontmatter_indexed_with_warning`
- [ ] F-02.6 Parse vault uji 10.000 catatan muat di target indeks ulang penuh di bawah 10 detik.

### F-03 · Indeks SQLite inkremental

Indeks selalu sesuai isi vault, cepat dibangun, aman dihapus, dan tidak rusak walau proses terhenti. Skema, lokasi, dan deteksi perubahan ada di dokumen desain 0001; bagian ini mengatur perilakunya.

**Desain:** status line saat pengindeksan pertama di `17-banners-and-messages.html`.

**Kapan indeks diperbarui**

- Aplikasi: memindai saat dibuka, lalu file watcher menjaga indeks tetap baru (F-04).
- CLI: setiap perintah lebih dulu menjalankan pengecekan cepat (ukuran dan waktu ubah file, tanpa membaca isi). `--no-refresh` melewatinya, untuk skrip yang memanggil banyak perintah berturut-turut.
- `patinote index` memperbarui secara eksplisit; `patinote index --rebuild` membangun ulang dari nol.

**Keamanan data**

- Perubahan ditulis per batch dalam satu transaksi; proses yang mati di tengah meninggalkan indeks pada kondisi utuh terakhir.
- Indeks dibangun ulang otomatis bila versi skema tidak cocok atau database gagal `PRAGMA integrity_check` saat dibuka.
- Vault yang dipindah atau diganti namanya mendapat indeks baru, karena ID vault diambil dari path. Tidak ada file ID yang ditulis ke vault.
- `patinote cache clean` menghapus indeks milik vault yang sudah tidak ada.

**Aplikasi dan CLI bersamaan**

- Keduanya bisa membaca kapan saja (mode WAL). Penulisan bergantian; CLI menunggu kunci sampai 5 detik, lalu keluar dengan exit code 5.

**Progres dan hasil**

- Pengindeksan lebih dari 1 detik menampilkan progres di stderr; dengan `--json`, stdout hanya berisi JSON hasil akhir.
- `patinote index --json` mengembalikan jumlah catatan, lampiran, file yang diproses ulang, peringatan, dan durasi.

**Kriteria penerimaan**

- [ ] F-03.1 Indeks ulang penuh vault uji 10.000 catatan selesai di bawah 10 detik.
- [x] F-03.2 Mengubah 1 catatan lalu `patinote index` hanya memproses ulang 1 file. `f03_2_changing_one_note_reprocesses_one_file`
- [x] F-03.3 File yang hanya di-`touch` tidak di-parse ulang. `f03_3_touched_file_is_not_parsed_again`
- [x] F-03.4 `kill -9` di tengah pengindeksan tidak merusak indeks; perintah berikutnya berjalan normal. `f03_4_kill_during_index_keeps_db_intact`
- [x] F-03.5 Menghapus `~/.cache/patinote/` lalu menjalankan perintah apa pun membangun indeks baru tanpa error. `f03_5_deleting_the_cache_rebuilds_on_the_next_command`
- [x] F-03.6 Database yang sengaja dirusak dideteksi dan dibangun ulang otomatis. `f03_6_corrupt_index_is_rebuilt_with_full_content`
- [x] F-03.7 Saat aplikasi menulis indeks, CLI tetap bisa membaca; penulisan CLI yang terkunci lebih dari 5 detik keluar dengan exit code 5. `f03_7_reads_work_while_another_process_writes_and_writers_give_up_with_5`
- [ ] F-03.8 Pengecekan cepat di vault 10.000 catatan tanpa perubahan selesai di bawah 200 ms.
- [x] F-03.9 `patinote cache clean` menghapus indeks vault yang foldernya sudah tidak ada, dan tidak menyentuh indeks vault lain. `f03_9_cache_clean_removes_indexes_of_vaults_that_are_gone`

### F-04 · Mendeteksi perubahan dari luar

Perubahan file dari luar Patinote (Neovim, Syncthing, `git checkout`) tercermin di indeks dan UI dalam waktu kurang dari 1 detik.

**Desain:** banner batas watcher dan status line polling di `17-banners-and-messages.html`.

**Mekanisme**

- Library `notify` lewat inotify; event dikumpulkan selama sekitar 100 ms lalu diproses dalam satu transaksi.
- Ditangani: buat, ubah, hapus, pindah atau ganti nama, termasuk folder.
- Hanya aplikasi GUI yang memasang watcher. CLI dan server MCP menyegarkan indeks di setiap perintah atau pemanggilan tool (F-03).

**Kasus khusus**

- Simpan atomik (tulis file sementara lalu ganti nama) dikenali sebagai "file diubah".
- Perubahan massal diproses dalam satu batch, tanpa UI berkedip berulang.
- File sementara Syncthing (`.syncthing.*.tmp`) dan editor (`*.swp`, `*~`) diabaikan.
- Event dari simpanan Patinote sendiri dikenali lewat hash dan tidak diproses ulang.
- Ganti nama dari luar membuat link ke nama lama menjadi link ke catatan yang belum ada, seperti Obsidian. Di Fase 3, rename dari luar dideteksi lewat hash yang sama lalu ditawarkan "Perbarui link?".

**Bila watcher gagal**

- Saat aplikasi dibuka, `max_user_watches` dicek; bila tidak cukup, muncul peringatan beserta perintah `sysctl`.
- Bila watcher gagal atau mati, Patinote beralih ke pemindaian cepat setiap 30 detik dan status line memberi tahu.

**Kriteria penerimaan**

- [ ] F-04.1 Catatan yang disimpan dengan `:w` di Neovim tercermin di indeks dan UI di bawah 1 detik.
- [ ] F-04.2 Catatan baru dari `echo "..." > baru.md` muncul di quick switcher di bawah 1 detik.
- [x] F-04.3 Catatan yang dihapus dari terminal hilang dari indeks; link ke sana menjadi link ke catatan yang belum ada. `f04_3_deleted_note_leaves_the_index_and_its_links_dangle`
- [x] F-04.4 Folder berisi 50 catatan yang dipindah dari terminal tercermin seluruhnya dalam satu batch. `f04_4_moved_folder_of_50_notes_is_one_change`
- [x] F-04.5 `git checkout` yang mengubah 100 file diproses dalam satu transaksi tanpa error. `f04_5_checkout_of_100_files_is_one_change_and_one_transaction`
- [ ] F-04.6 Menyimpan dari Patinote tidak memicu parse ulang kedua.
- [ ] F-04.7 Dengan `max_user_watches` diturunkan sampai watcher gagal, muncul peringatan dan perubahan tetap terdeteksi dalam 30 detik.

### F-05 · Editor Markdown

Menulis dan mengedit catatan dengan nyaman: mengetik tidak pernah tersendat dan tidak ada tulisan yang hilang.

**Desain:** `01-minimal.html` (editor dan pewarnaan), banner simpan gagal dan read-only di `17-banners-and-messages.html`. Warna sintaks: `0003` bagian 3.4; huruf: bagian 4.

**Tampilan teks**

- Markdown mentah dengan pewarnaan dasar: heading, tebal/miring, kode inline dan blok kode (tanpa pewarnaan per bahasa sampai Fase 6), wikilink, tag, checkbox, kutipan, daftar; frontmatter ditampilkan redup.
- Font monospace (JetBrains Mono). Mode baca dan live preview Fase 6 memakai Inter.
- Satu kolom tengah selebar maksimal sekitar 100 karakter, dengan soft wrap.
- `j`/`k` berpindah per baris asli (standar Vim), `gj`/`gk` per baris tampilan; bisa ditukar lewat konfigurasi.
- Nomor baris relatif, baris aktif menampilkan nomor aslinya; bisa dimatikan.
- Di Fase 2 karakter Markdown tidak disembunyikan, diredupkan, atau diubah ukurannya; hanya diwarnai. Hasil render dilihat lewat mode baca (F-27); live preview baru di Fase 6 (F-21).

**Menyimpan**

- Autosave 1 detik setelah berhenti mengetik, saat pindah catatan, saat jendela kehilangan fokus, dan saat aplikasi ditutup.
- Penyimpanan atomik (file sementara di folder yang sama lalu ganti nama). Izin file, akhir baris (LF/CRLF), BOM, dan newline di akhir file dipertahankan. File tanpa perubahan isi tidak disimpan ulang.
- Catatan yang belum tersimpan ditandai titik di bufferline.
- Simpan gagal (disk penuh, izin ditolak): pesan error yang jelas, isi editor tetap, penyimpanan dicoba lagi.

**Lainnya**

- Undo/redo per catatan selama terbuka; undo antar-sesi di Fase 6.
- Indentasi mengikuti gaya file; file baru memakai tab (default Obsidian).
- Paste sebagai teks polos; paste gambar di F-20.
- Catatan di atas 5 MB dan file yang bukan UTF-8 dibuka read-only: isinya ditampilkan, banner info menjelaskan alasannya dan menawarkan "Open in default app", dan perintah edit tidak berlaku (mockup `17-banners-and-messages.html`).

**Kriteria penerimaan**

- [ ] F-05.1 Latensi mengetik di bawah 16 ms pada catatan 1.000 baris, diukur di laptop Intel Iris Xe.
- [x] F-05.2 Setelah berhenti mengetik 1 detik, file di disk sudah berisi teks itu. `f05_2_one_second_after_typing_the_file_has_the_text`
- [ ] F-05.3 Aplikasi ditutup paksa 2 detik setelah berhenti mengetik: tidak ada teks yang hilang.
- [x] F-05.4 File CRLF tetap CRLF dan file ber-BOM tetap ber-BOM setelah diedit; file tanpa perubahan isi tidak tersimpan ulang. `f05_4_crlf_and_bom_round_trip`, `f05_4_crlf_and_bom_are_kept_and_unchanged_files_are_not_rewritten`
- [x] F-05.5 Disk penuh (disimulasikan): pesan error muncul, isi editor utuh, file asli di disk tidak rusak. `f05_5_a_failed_save_shows_an_error_keeps_the_text_and_the_file`
- [ ] F-05.6 Satu sesi pengeditan tidak menghasilkan event perubahan ganda di indeks.
- [x] F-05.7 Undo dan redo mengembalikan isi persis setelah 100 langkah pengeditan. `f05_7_undo_redo_100_edits_restore_exactly`
- [x] F-05.8 Membuka catatan 6 MB atau file Latin-1 menampilkan banner read-only; menekan `i`, `dd`, atau `p` tidak mengubah apa pun dan file di disk identik byte per byte. `f05_8_large_and_latin1_notes_are_read_only`

### F-06 · Mode Vim dasar

Mengedit dengan kebiasaan Vim sehari-hari tanpa terasa berbeda. Patokan perilakunya adalah Neovim.

**Desain:** badge dan kursor di `01-minimal.html` (NORMAL), `06-link-autocomplete.html` (INSERT), `13-visual-mode-and-search.html` (VISUAL, seleksi, sorotan `/`). Warna: `0003` bagian 3.3 dan 3.5.

**Perintah di Fase 2**

| Kelompok | Perintah |
| --- | --- |
| Mode | Normal, insert, visual (`v`), visual baris (`V`); `Esc` dan `Ctrl+[` keluar ke normal |
| Masuk insert | `i` `a` `I` `A` `o` `O` `s` `S` |
| Gerakan | `h` `j` `k` `l`, `w` `b` `e`, `W` `B` `E`, `0` `^` `$`, `gg` `G`, `{n}G`, `Ctrl+d` `Ctrl+u` |
| Lompat di baris | `f` `F` `t` `T` `;` `,` |
| Cari di catatan | `/` `n` `N` |
| Operator + gerakan | `d` `c` `y` dengan gerakan apa pun di atas (`dw`, `c$`, `y3j`, `dtx`) |
| Objek teks | `iw` `aw` (kata), `i"` `a"` `i'` `a'` (kutip), `i(` `a(` `i[` `a[` (kurung); contoh `ciw`, `di"` |
| Operator baris | `dd` `cc` `yy`, `D` `C` `Y` |
| Edit cepat | `x` `X`, `r`, `J`, `>>` `<<`, `p` `P` |
| Ulangi | `.` (ulangi perubahan terakhir) |
| Hitungan | Angka di depan perintah: `3dd`, `5j`, `2w` |
| Undo | `u`, `Ctrl+r` |
| Di mode insert | `Ctrl+w` (hapus kata), `Ctrl+u` (hapus ke awal baris), tombol panah |
| Perintah `:` | `:w`, `:q` (tutup catatan, bukan aplikasi), `:wq`, `:x`, `:q!`, `:e <catatan>` |

Register bernama, macro, visual blok, `:s`, objek teks lain, dan file keymap tetap di Fase 6 (F-22).

**Perilaku lain**

- `y` dan `p` memakai clipboard sistem (setara `clipboard=unnamedplus`).
- `:w` menyimpan seketika; autosave tetap berjalan sendiri.
- Mode aktif tampil di status line; kursor blok di normal, garis di insert.
- Opsi konfigurasi `insert_escape` (misalnya `"jk"`) untuk keluar dari insert; mati secara default.
- Logika Vim adalah modul murni tanpa Iced, sehingga setiap perintah bisa dites tanpa jendela.

**Kriteria penerimaan**

- [x] F-06.1 Setiap perintah di tabel punya minimal satu kasus uji (teks dan kursor awal, urutan tombol, teks dan kursor akhir). Hasil yang diharapkan dibuat otomatis dengan `nvim --headless` pada kasus yang sama; semua kasus lolos. `f06_1_every_command_matches_neovim`, `f06_1_wq_and_x_save_and_close`, `e_opens_a_note_by_name_alias_or_path_and_reports_the_rest`
- [x] F-06.2 Hitungan bekerja untuk semua gerakan dan operator di tabel. `f06_2_counts_work_for_every_motion_and_operator`
- [x] F-06.3 Satu `u` membatalkan satu perintah utuh: `3dd` utuh, satu sesi insert utuh, satu `.` utuh. `f06_3_one_undo_reverts_one_whole_command`
- [x] F-06.4 `:q` menutup catatan aktif, bukan aplikasi; `:q!` membuang perubahan sejak simpan terakhir. `f06_4_q_closes_the_note_and_q_bang_drops_unsaved_changes`
- [ ] F-06.5 `yy` di Patinote lalu paste di aplikasi lain menghasilkan teks yang sama, dan sebaliknya.
- [ ] F-06.6 Latensi tetap di bawah 16 ms saat menahan `j` di catatan 1.000 baris.
- [x] F-06.7 Dengan `insert_escape = "jk"`, mengetik `jk` di insert keluar ke normal tanpa menyisakan huruf; mengetik `j` lalu jeda lebih dari 1 detik tetap menyisipkan `j`. `f06_7_insert_escape_jk_and_timeout`

### F-07 · Quick switcher

Membuka catatan mana pun dengan beberapa ketukan, dengan hasil yang muncul seketika.

**Desain:** `05-quick-switcher.html`. Kotak kosong dan tanpa hasil: `0003` bagian 7.

**Membuka dan mencari**

- `<leader>f` dari mode normal, atau `Ctrl+Shift+O`; kotak di tengah atas, editor tetap terlihat. (`Ctrl+O` dipakai untuk kembali di riwayat, lihat F-09.)
- Dicari: judul (nama file), alias frontmatter, dan path folder. Lampiran tidak ikut di Fase 2.
- Fuzzy dengan `nucleo` ("dsn idx" cocok dengan "Desain indeks Patinote"); huruf besar-kecil dan diakritik tidak dibedakan.

**Urutan hasil**

1. Skor kecocokan.
2. Bonus untuk catatan yang baru-baru ini dibuka.
3. Bila sama, path lebih pendek dulu.

Kotak kosong menampilkan catatan yang terakhir dibuka.

**Tampilan hasil**

- Judul dengan huruf yang cocok disorot; folder ditampilkan redup.
- Bila yang cocok alias, alias ikut tampil ("Second Brain → Patinote").
- Maksimal 50 hasil.

**Tombol**

- `Ctrl+n`/`Ctrl+p`, `Ctrl+j`/`Ctrl+k`, atau panah untuk berpindah; `Enter` membuka; `Esc` menutup.
- `Shift+Enter` membuat catatan baru bernama sesuai yang diketik, di folder catatan baru (default root vault, bisa diubah di konfigurasi; aturan yang sama dipakai F-10).

**Kriteria penerimaan**

- [x] F-07.1 Hasil diperbarui di bawah 16 ms per tombol di vault uji 10.000 catatan. `cargo bench -p patinote-app --bench switcher` (10.000 catatan: 1,6–2,4 ms per tombol)
- [x] F-07.2 Mengetik alias menemukan catatan pemiliknya. `f07_2_an_alias_finds_its_note`, `f07_2_an_alias_finds_its_note_and_enter_opens_it`
- [x] F-07.3 "cafe" menemukan "Café Bandung"; "PATI" menemukan "Patinote". `f07_3_case_and_diacritics_are_ignored`
- [x] F-07.4 Kotak kosong menampilkan catatan terakhir dibuka, terbaru di atas. `f07_4_an_empty_query_lists_recent_notes_newest_first`, `f07_4_the_empty_box_lists_recent_notes_newest_first`
- [x] F-07.5 `Shift+Enter` dengan nama baru membuat catatan dan langsung membukanya dalam mode insert. `f07_5_shift_enter_creates_the_note_and_opens_it_in_insert_mode`
- [ ] F-07.6 Catatan baru dari luar Patinote muncul di switcher di bawah 1 detik.
- [x] F-07.7 Semua aksi switcher bisa dilakukan tanpa mouse. `f07_7_every_switcher_action_works_from_the_keyboard`

### F-08 · Autocomplete link dan mengikuti link

Membuat link secepat mengetik nama catatan, dan berpindah lewat link tanpa mouse.

**Desain:** `06-link-autocomplete.html`, `12-ambiguous-link.html`. Saran heading dan tanpa kecocokan: `0003` bagian 7.

**Autocomplete**

| Yang diketik | Yang disarankan |
| --- | --- |
| `[[desa` | Catatan dan alias, fuzzy, bonus catatan yang baru dibuka (mesin F-07) |
| `![[fo` | Catatan dan lampiran |
| `[[Patinote#` | Heading di catatan Patinote, urut sesuai dokumen |
| `[[Patinote#^` | ID blok di catatan Patinote |
| `[[Patinote\|` | Tidak ada saran (teks bebas) |

- Yang disisipkan: nama terpendek yang masih unik (`[[Patinote]]`, atau `[[Proyek/Rapat]]` bila ada dua "Rapat"); alias menjadi `[[Patinote\|Second Brain]]`.
- Mengetik `[[` otomatis menambahkan `]]` di belakang kursor.
- Tanpa kecocokan, `Enter` menyisipkan teks yang diketik sebagai link ke catatan yang belum ada.
- `Ctrl+n`/`Ctrl+p` atau panah memilih; `Enter` atau `Tab` menyisipkan; `Esc` pertama menutup saran, `Esc` kedua keluar dari insert.

**Mengikuti link**

| Aksi | Hasil |
| --- | --- |
| `gd` atau `Enter` di atas link (normal), `Ctrl+klik` | Membuka catatan tujuan |
| Link ke heading atau blok | Membuka catatan dan menggulir ke heading atau blok itu |
| Link ke catatan yang belum ada | Membuka catatan kosong; file ditulis ke disk saat ada isinya |
| Link ambigu | Daftar pilihan |
| `gx` di atas URL | Membuka browser |
| Link ke lampiran | Dibuka dengan aplikasi bawaan sistem (`xdg-open`) |

**Kriteria penerimaan**

- [ ] F-08.1 Saran muncul di bawah 50 ms setelah `[[` dan diperbarui di bawah 16 ms per tombol di vault 10.000 catatan.
- [ ] F-08.2 Memilih alias menghasilkan link dengan alias sebagai teks tampilan.
- [ ] F-08.3 Bila ada dua catatan bernama sama, link yang disisipkan menyertakan folder.
- [ ] F-08.4 `[[Catatan#` menampilkan heading catatan yang benar, urut sesuai dokumen.
- [ ] F-08.5 `gd` pada link heading membuka catatan dengan heading itu terlihat di layar.
- [ ] F-08.6 `gd` pada link ke catatan yang belum ada lalu `:q` tanpa mengetik: tidak ada file baru di vault.
- [ ] F-08.7 Link dari autocomplete langsung terhitung di backlinks catatan tujuan setelah autosave.

### F-09 · Backlinks, outline, riwayat, dan buffer

Melihat siapa yang menautkan catatan ini, berpindah maju-mundur seperti di Vim, dan menjaga beberapa catatan tetap terbuka.

**Desain:** panel samping di `02-minimal-panels-open.html`, backlinks di bawah catatan di `01-minimal.html`, panel bertab di `03-classic.html`.

**Panel samping**

- Panel kanan berisi dua bagian bertumpuk: Outline di atas, Backlinks di bawah. `<leader>b` dan `<leader>o` membuka panel lalu memindahkan fokus ke bagian itu. Saat panel tertutup, backlinks tampil di bawah catatan.
- Backlinks dikelompokkan per catatan sumber; tiap link menampilkan kalimat di sekitarnya dengan link disorot. Urutan: catatan sumber yang paling baru diubah di atas.
- Ikut dihitung: link ke heading atau blok catatan ini (bagiannya ditampilkan, misalnya "→ #Tabel inti") dan link dari properti frontmatter (label "properti").
- Outline: daftar heading catatan aktif, heading tempat kursor berada disorot; `Enter` melompat ke heading itu.
- `Ctrl+w l` / `Ctrl+w h` memindahkan fokus antara editor dan panel; `j`/`k` berpindah item; `Enter` membuka catatan sumber tepat di baris link.

**Riwayat navigasi**

- Setiap perpindahan catatan dan lompatan jauh di dalam catatan dicatat beserta posisi kursor, maksimal 100 entri per sesi.
- `Ctrl+O` kembali dan `Ctrl+I` maju, seperti jump list Vim; memulihkan catatan dan posisi kursor.

**Buffer**

- Catatan terbuka tampil di bufferline. Pindah dengan `gt`/`gT` atau `]b`/`[b`; pilih dengan `:b <nama>`; tutup dengan `:q` atau `<leader>x`.
- Maksimal 20 buffer. Buffer tertua yang tidak sedang diubah ditutup otomatis; buffer yang belum tersimpan tidak pernah ditutup otomatis.
- Saat aplikasi dibuka, buffer dan posisi kursor sesi sebelumnya dipulihkan (disimpan di `~/.local/state/patinote/`).

**Kriteria penerimaan**

- [ ] F-09.1 Backlinks memperlihatkan link baru di bawah 1 detik setelah catatan sumber disimpan.
- [ ] F-09.2 Link ke heading, ke blok, dan dari properti frontmatter tampil dengan penandanya.
- [ ] F-09.3 `Enter` pada item backlink membuka catatan sumber dengan kursor di baris link.
- [ ] F-09.4 Outline menyorot heading tempat kursor berada; `Enter` melompat ke heading itu.
- [ ] F-09.5 Buka A, B, C lalu `Ctrl+O` dua kali: kembali di A dengan posisi kursor yang sama; `Ctrl+I` maju ke B.
- [ ] F-09.6 Setelah aplikasi ditutup dan dibuka lagi, buffer yang sama terbuka di posisi kursor yang sama.
- [ ] F-09.7 Membuka buffer ke-21 menutup buffer tertua yang tidak diubah; buffer belum tersimpan tidak pernah ditutup otomatis.
- [ ] F-09.8 Semua aksi di bagian ini bisa dilakukan tanpa mouse.

### F-10 · Membuat, mengganti nama, dan menghapus

Mengelola catatan dan folder dari dalam Patinote tanpa kehilangan data dan tanpa memutus link.

**Desain:** explorer di `02-minimal-panels-open.html`, dialog di `09-rename-delete-dialogs.html`, catatan baru, input explorer, pindah, dan hapus folder di `14-create-move-delete-folder.html`.

**Membuat**

- `<leader>n` lalu ketik nama (kosong menjadi "Untitled", "Untitled 1", dan seterusnya), `:e <nama>`, atau `Shift+Enter` di quick switcher.
- Disimpan di folder catatan baru (default root vault); file ditulis ke disk saat sudah ada isinya (F-08).
- Karakter terlarang di nama: `\ / : * ? " < > |`. Karakter `# ^ [ ]` diberi peringatan karena merusak wikilink.

**Explorer (`<leader>e`)**

| Tombol | Aksi |
| --- | --- |
| `a` | Catatan baru di folder terpilih |
| `A` | Folder baru |
| `r` | Ganti nama |
| `m` | Pindah ke folder lain |
| `d` | Hapus ke `.trash/` |
| `Enter` / `l` | Buka catatan atau folder |
| `h` | Tutup folder |

**Ganti nama dan pindah**

- Semua link yang menunjuk catatan itu ikut diperbarui: wikilink, embed, link Markdown, dan link di properti frontmatter. Teks tampilan dan bagian `#heading` atau `#^blok` tetap utuh.
- Link yang hanya memakai nama tidak diubah saat catatan dipindah, selama nama itu masih unik; link yang memakai path disesuaikan.
- Sebelum menulis, muncul konfirmasi jumlah link dan catatan yang akan diubah. Hanya teks link yang diubah; isi lain setiap file tidak tersentuh.
- Pratinjau per perubahan, undo, deteksi rename dari luar, dan akses agent menyusul di F-12 (Fase 3).

**Menghapus**

- Catatan atau folder dipindah ke `.trash/` dengan path relatif yang sama (`Proyek/Rapat.md` menjadi `.trash/Proyek/Rapat.md`). Bila nama sudah ada di `.trash/`, ditambah waktu penghapusan.
- Konfirmasi menyebut jumlah backlink ("3 catatan menautkan ke sini. Hapus?").
- Pemulihan dari `.trash/` sementara manual.

**Kriteria penerimaan**

- [ ] F-10.1 `<leader>n`, ketik nama, isi teks: file muncul di folder yang benar dan langsung ada di quick switcher.
- [ ] F-10.2 Nama dengan karakter terlarang ditolak dengan pesan yang menjelaskan alasannya.
- [ ] F-10.3 Hapus memindahkan catatan ke `.trash/` dengan path relatif sama; isi utuh, tidak ada yang terhapus permanen.
- [ ] F-10.4 Dua catatan dengan path sama yang dihapus bergantian tidak saling menimpa di `.trash/`.
- [ ] F-10.5 Hapus folder berisi 10 catatan memindahkan seluruh isinya ke `.trash/`.
- [ ] F-10.6 Ganti nama catatan yang ditautkan dari 5 catatan memperbarui kelima link; teks tampilan dan `#heading` utuh; selain teks link, setiap file identik byte per byte.
- [ ] F-10.7 Pindah catatan ke folder lain memperbarui link yang memakai path; link yang hanya memakai nama tidak berubah bila nama masih unik.
- [ ] F-10.8 Setelah ganti nama, panel backlinks catatan itu berisi catatan sumber yang sama seperti sebelumnya.

### F-11 · Perubahan dari luar saat mengedit dan konflik Syncthing

Tidak ada tulisan yang tertimpa diam-diam, baik oleh editor lain, Syncthing, maupun Patinote sendiri.

**Desain:** `08-external-change-conflict.html`, `15-sync-conflict-list.html`, `16-sync-conflict-diff.html`, banner file dihapus dari luar di `17-banners-and-messages.html`.

**Catatan terbuka berubah dari luar**

| Kondisi buffer | Perilaku |
| --- | --- |
| Tanpa perubahan belum tersimpan | Dimuat ulang otomatis; kursor tetap di baris yang sama sebisa mungkin; status line menampilkan "Diperbarui dari luar" |
| Ada perubahan belum tersimpan | Autosave buffer itu dihentikan; banner dengan pilihan Pakai versi disk, Pertahankan versi saya, Bandingkan |
| File dihapus dari luar | Banner dengan pilihan Simpan ulang di lokasi yang sama, Tutup |

Sebelum setiap simpan, hash file di disk dibandingkan dengan hash saat terakhir dimuat; bila berbeda, simpan dibatalkan dan banner konflik muncul, walau watcher melewatkan event.

**File konflik Syncthing**

- Salinan `*.sync-conflict-*` tidak diindeks sebagai catatan, tetapi dicatat sebagai konflik.
- Status line menampilkan jumlah konflik; daftar dibuka dengan `<leader>c` atau `:conflicts`.
- Membuka konflik menampilkan diff per baris antara catatan asli dan salinan, dengan aksi Pakai salinan, Pertahankan asli, atau Buka keduanya untuk menggabungkan manual.
- Setelah selesai, salinan dipindah ke `.trash/`.
- Penggabungan per bagian (memilih potongan dari kedua versi) menyusul di Fase 3.

**Kriteria penerimaan**

- [ ] F-11.1 Catatan terbuka tanpa perubahan belum tersimpan diedit dari Neovim: isi baru tampil di bawah 1 detik, kursor di baris yang sama.
- [ ] F-11.2 Catatan dengan ketikan belum tersimpan diedit dari luar: banner muncul, isi disk tidak ditimpa, ketikan tidak hilang.
- [ ] F-11.3 Dengan watcher yang disimulasikan melewatkan event, simpan tetap dibatalkan karena hash berbeda dan banner muncul.
- [ ] F-11.4 "Bandingkan" menampilkan diff yang benar antara versi disk dan versi editor.
- [ ] F-11.5 Salinan `.sync-conflict-*` tidak muncul di quick switcher, tetapi muncul di daftar konflik di bawah 1 detik.
- [ ] F-11.6 "Pakai salinan" mengganti isi catatan asli dengan isi salinan, lalu salinan pindah ke `.trash/`.
- [ ] F-11.7 Dengan dua folder yang disinkronkan Syncthing sungguhan, konflik yang dibuat sengaja terdeteksi dan bisa diselesaikan tanpa kehilangan teks dari kedua sisi.

### F-27 · Mode baca

Membaca catatan yang sudah dirender, nyaman di mata, dan tetap bisa dinavigasi dengan keyboard.

**Desain:** `07-reading-mode.html`. Warna link dan label: `0003` bagian 3.6.

**Membuka**

- `<leader>r` berpindah antara mode edit dan mode baca untuk catatan aktif; catatan selalu dibuka dalam mode edit.
- Saat berpindah mode, posisi gulir tetap di bagian yang sama.

**Yang dirender di Fase 2**

| Elemen | Tampilan |
| --- | --- |
| Heading, tebal/miring, daftar, kutipan, garis, tabel | Dirender penuh dengan Inter, di kolom tengah yang sama dengan editor |
| Wikilink dan link Markdown | Bisa diklik; link ke catatan yang belum ada tampil redup |
| Tag | Label |
| Checkbox | Bisa dicentang; perubahan langsung disimpan ke file |
| Blok kode | Monospace, tanpa pewarnaan per bahasa |
| Gambar (`![[foto.png]]`, `![](foto.png)`) | Tampil di dalam teks, diperkecil sesuai lebar kolom |
| Embed catatan (`![[Catatan]]`) | Kartu link; isi catatan tampil di Fase 6 |
| Callout (`> [!note]`) | Kutipan dengan judul tebal; tampilan penuh di Fase 6 |
| Rumus (`$...$`) | Teks mentah; render di Fase 6 |
| Frontmatter | Kotak properti ringkas di atas, bisa dilipat |

**Navigasi**

- `j`/`k`, `Ctrl+d`/`Ctrl+u`, `gg`/`G` untuk menggulir; `/` untuk mencari.
- `Tab`/`Shift+Tab` berpindah antar link; `Enter` mengikuti link. Label huruf ala Vimium menyusul di Fase 6.

**Kriteria penerimaan**

- [ ] F-27.1 Catatan contoh (`fixtures/vault-mockup/Proyek/Desain indeks Patinote.md`) yang berisi semua elemen di tabel dirender sesuai kolom Tampilan (dicek dengan screenshot pembanding).
- [ ] F-27.2 Berpindah ke mode baca di catatan 1.000 baris selesai di bawah 100 ms.
- [ ] F-27.3 Mencentang checkbox mengubah `- [ ]` menjadi `- [x]` di file; bagian lain file tidak berubah.
- [ ] F-27.4 Gambar 4000×3000 piksel tampil selebar kolom; kenaikan RAM tidak melebihi ukuran bitmap pada ukuran tampil (lebar × tinggi × 4 byte) ditambah 10%.
- [ ] F-27.5 `Tab` lalu `Enter` membuka catatan tujuan link.
- [ ] F-27.6 Kembali ke mode edit menempatkan layar di bagian yang sedang dibaca.

### F-28 · Tata letak dan panel

Layar utama sesuai `docs/mockups/01-minimal.html` dan `02-minimal-panels-open.html`, dengan nilai dari `docs/design/0003-ui-design-system.md`: minimalis secara default, setiap elemen bisa dicapai dengan keyboard, dan tetap bisa dipakai dengan mouse. File `90-target-final-phase6.html` menggambarkan tampilan akhir (sekitar Fase 6).

**Desain:** `01-minimal.html` sampai `04-focus.html`, popup leader di `02-minimal-panels-open.html`, tanpa catatan terbuka di `11-no-note-open.html`. Ukuran panel dan popup: `0003` bagian 5.

**Susunan layar**

| Bagian | Isi |
| --- | --- |
| Bufferline (atas) | Catatan terbuka, bernomor, bertanda titik bila belum tersimpan |
| Area tengah | Editor (F-05) atau mode baca (F-27) di kolom tengah |
| Explorer (kiri, `<leader>e`) | Pohon folder, daftar tag, petunjuk tombol; lebar default 240 px |
| Panel samping (kanan, `<leader>b` / `<leader>o`) | Outline dan Backlinks bertumpuk; lebar default 300 px |
| Status line | Mode Vim, path catatan, `[+]` bila belum tersimpan, jumlah kata, jumlah backlinks, status indeks/watcher, jumlah konflik (hanya bila ada), tombol panel, `baris:kolom` |
| Baris perintah (paling bawah) | Perintah `:`, pesan, dan error, bergaya Vim |

Lebar panel bisa diubah dengan menyeret batasnya dan diingat antar sesi.

**Mode tampilan**

Komponen yang sama disusun dalam tiga mode; berpindah mode tidak menutup buffer atau mengubah catatan.

| Mode | Susunan | Acuan mockup |
| --- | --- | --- |
| `minimal` (default) | Tanpa panel; bufferline, editor, status line, baris perintah | `01-minimal.html`, `02-minimal-panels-open.html` |
| `classic` | Explorer dan panel samping terbuka; bufferline sebagai tab dengan tombol ×; panel samping memakai tab (Backlinks, Outline) | `03-classic.html` |
| `focus` | Hanya kolom teks; bufferline, nomor baris, dan panel disembunyikan; status line diperkecil menjadi mode dan jumlah kata | `04-focus.html` |

- Berpindah mode dengan `:layout minimal|classic|focus` atau `<leader>l` (berputar ke mode berikutnya).
- Mode default diatur di `~/.config/patinote/config.toml` (`layout = "minimal"`).
- Mode terakhir dan panel yang terakhir terbuka diingat antar sesi.
- Bagian tampilan A dan C yang bergantung pada fase lain mengikuti fasenya: live preview Inter (Fase 6), Graf (Fase 7), "Belum ditautkan" (Fase 3), tema terang C dan pilihan mode di layar pengaturan (Fase 8).

**Popup `<leader>`**

- Leader adalah Spasi. Popup muncul bila tidak ada tombol lanjutan dalam 300 ms.
- Isi Fase 2: `f` find note, `n` new note, `e` explorer, `b` backlinks, `o` outline, `r` reading mode, `c` conflicts, `x` close buffer, `l` switch layout.

**Jendela**

- Judul: "Nama Catatan — Patinote". Ukuran diingat; posisi diatur oleh desktop (Wayland).
- Skala HiDPI mengikuti GNOME; ukuran teks `Ctrl+=`, `Ctrl+-`, `Ctrl+0`.
- Tanpa catatan terbuka: daftar catatan terakhir dan pintasan utama (mockup `11-no-note-open.html`).

**Fokus, bahasa, dan tema**

- `Ctrl+w h` / `Ctrl+w l` memindahkan fokus antar panel; `Esc` di panel mengembalikan fokus ke editor. Semua elemen juga bisa diklik.
- Teks antarmuka berbahasa Inggris; isi catatan tetap dalam bahasa pengguna.
- Fase 2 hanya tema gelap, aksen verdigris `#5CC4A8`. Tema terang dan ikut sistem di Fase 8 (F-24).

**Kriteria penerimaan**

- [ ] F-28.1 Layar utama sesuai `docs/mockups/01-minimal.html` dan `02-minimal-panels-open.html` (dicek dengan screenshot pembanding).
- [x] F-28.2 Saat pertama kali dibuka, tampil mode minimal: tanpa panel, hanya bufferline, editor, status line, baris perintah. `f28_2_first_open_is_minimal_with_no_note_open`
- [ ] F-28.3 `<leader>e`, `<leader>b`, `<leader>o` membuka dan menutup panelnya; panel yang ditutup melepas memorinya.
- [ ] F-28.4 Spasi lalu tunggu 300 ms memunculkan popup; Spasi lalu `f` dengan cepat membuka quick switcher tanpa popup sempat tampil.
- [ ] F-28.5 Status line menampilkan mode yang benar dalam satu frame setelah mode berganti.
- [ ] F-28.6 Di skala GNOME 125% dan 200%, teks tajam dan tata letak tidak terpotong.
- [ ] F-28.7 Lebar panel yang diseret tetap sama setelah aplikasi ditutup dan dibuka lagi.
- [ ] F-28.8 Membuka aplikasi sampai layar utama tampil (indeks sudah ada) di bawah 300 ms.
- [ ] F-28.9 Semua elemen di tabel bisa dicapai tanpa mouse.
- [ ] F-28.10 `:layout classic`, `:layout focus`, dan `<leader>l` berpindah mode di bawah 100 ms tanpa menutup buffer atau mengubah posisi kursor.
- [ ] F-28.11 Mode dan panel yang terakhir terbuka sama setelah aplikasi ditutup dan dibuka lagi.
- [ ] F-28.12 `layout = "classic"` di konfigurasi membuat aplikasi yang baru dipasang langsung terbuka dalam mode classic.

### A-01 · Perintah CLI

Semua yang bisa dibaca dari vault di Gerbang 1 bisa diakses dari terminal tanpa interaksi, oleh pengguna, skrip, dan agent AI. Perintah menulis menyusul di Fase 3 (A-06).

**Perintah Gerbang 1**

| Perintah | Fungsi |
| --- | --- |
| `patinote index [--rebuild]` | Memperbarui atau membangun ulang indeks |
| `patinote vault info` | Lokasi vault dan indeks, jumlah catatan dan lampiran, peringatan |
| `patinote list [--tag T] [--folder F] [--kind note\|attachment] [--sort name\|modified] [--limit N]` | Daftar catatan dengan filter |
| `patinote read <catatan> [--lines 10:40]` | Isi catatan; dengan `--json` ikut frontmatter, tag, heading, link |
| `patinote search <kueri> [--limit N]` | Pencarian isi dasar (kata dan frasa); operator menyusul di Fase 4 |
| `patinote backlinks <catatan>` | Link masuk beserta kalimat di sekitarnya |
| `patinote links <catatan>` | Link keluar, termasuk ke catatan yang belum ada |
| `patinote tags [--prefix P]` | Tag beserta jumlah pemakaian, termasuk tag bersarang |
| `patinote orphans` | Catatan tanpa link masuk maupun keluar |
| `patinote unresolved` | Link ke catatan yang belum ada, dikelompokkan per nama tujuan |
| `patinote resolve <nama>` | Ke catatan mana sebuah nama atau link mengarah |
| `patinote conflicts` | Daftar file konflik Syncthing |
| `patinote open <catatan>` | Membuka catatan di GUI (Fase 2) |
| `patinote cache clean` | Menghapus indeks vault yang sudah tidak ada |
| `patinote completions bash\|zsh\|fish` | Shell completion, dibuat dari definisi `clap` |
| `patinote mcp` | Server MCP (A-04) |
| `patinote agents init [--update] [--for claude,gemini]` | Membuat atau memperbarui `AGENTS.md` di vault (A-05) |

Opsi global: `--vault <path>`, `--json`, `--no-refresh`, `--quiet`, `--strict`, `--color auto|always|never`.

**Menyebut catatan**

`<catatan>` bisa berupa nama, alias, atau path, dengan aturan resolusi yang sama seperti wikilink. Nama ambigu keluar dengan exit code 4 dan daftar kandidat.

**Output tanpa `--json`**

Satu hasil per baris, path di kolom pertama, kolom dipisah tab, agar mudah diproses `grep`, `cut`, dan `awk`:

```text
Harian/2026-11-19.md	3	Lanjut desain skema [[Patinote]] hari ini
Rapat/Rapat 19 Nov.md	12	Keputusan untuk [[Patinote]]: FTS5 dulu
```

**Kriteria penerimaan**

- [x] A-01.1 Setiap perintah di tabel berjalan tanpa prompt dan tanpa menunggu input. `a01_1_every_command_runs_without_input`
- [x] A-01.2 Setiap perintah punya `--help` dengan minimal satu contoh pemakaian. `a01_2_every_command_has_help_with_an_example`
- [x] A-01.3 Nama, alias, dan path untuk catatan yang sama memberi hasil yang sama. `a01_3_name_alias_and_path_give_the_same_result`
- [x] A-01.4 Nama ambigu keluar dengan exit code 4 dan daftar kandidat. `a01_4_ambiguous_names_exit_4_with_candidates`
- [x] A-01.5 `cut -f1` pada output tanpa `--json` menghasilkan daftar path yang valid. `a01_5_first_column_is_a_valid_path`
- [x] A-01.6 Di vault 10.000 catatan, `backlinks`, `links`, `resolve`, dan `read` selesai di bawah 100 ms, termasuk pengecekan cepat indeks. `cargo bench -p patinote-cli --bench cli`
- [ ] A-01.7 Hasil `patinote backlinks X` sama persis dengan panel backlinks di GUI untuk catatan yang sama.

### A-02 · Kontrak output `--json`

Agent dan skrip membaca output dengan pasti; bentuknya tidak pernah berubah diam-diam.

**Aturan dasar**

- Dengan `--json`, stdout hanya berisi satu dokumen JSON; progres dan log ke stderr.
- Setiap dokumen punya `"v"` (versi kontrak perintah itu) dan `"warnings": []` yang selalu ada.
- Field `snake_case`; path relatif ke root vault dengan `/`; waktu ISO 8601 dengan zona waktu; nomor baris mulai dari 1.
- Field tanpa nilai ditulis `null`, tidak dihilangkan.

**Bentuk catatan di semua perintah**

```json
{
  "path": "Proyek/Patinote.md",
  "title": "Patinote",
  "kind": "note",
  "aliases": ["Second Brain"],
  "tags": ["proyek/patinote", "rust"],
  "modified": "2026-11-19T15:21:00+08:00",
  "size": 4210
}
```

**Hasil panjang**

- `list` dan `search` dibatasi 50 hasil secara default; `--limit N` mengubahnya, `--limit 0` tanpa batas.
- Output selalu menyertakan `"total"` dan `"truncated"`.

**Error**

Error ke stderr, stdout kosong, exit code mengikuti A-03.

```json
{
  "v": 1,
  "error": {
    "code": "ambiguous_note",
    "message": "2 notes are named 'Rapat'",
    "candidates": ["Kantor/Rapat.md", "Pribadi/Rapat.md"]
  }
}
```

Kode error Gerbang 1: `vault_not_found`, `note_not_found`, `ambiguous_note`, `invalid_argument`, `index_locked`, `io_error`, `not_utf8`.

**Kestabilan**

- Output didefinisikan sebagai tipe Rust; JSON Schema dibuat otomatis dengan `schemars` dan disimpan di `docs/schema/`.
- Menambah field tidak menaikkan `v`; mengubah atau menghapus field wajib menaikkan `v`.

**Kriteria penerimaan**

- [x] A-02.1 Untuk setiap perintah dan setiap kasus error, output `--json` valid untuk `jq` dan cocok dengan JSON Schema di `docs/schema/`. `a02_1_json_matches_the_schema_for_every_command_and_error`
- [x] A-02.2 Snapshot test setiap perintah gagal bila field berubah atau hilang tanpa kenaikan `v`. `a02_2_snapshots_guard_every_output`
- [x] A-02.3 Stdout `--json` tidak pernah berisi teks lain, termasuk saat pengindeksan lebih dari 1 detik. `a02_3_stdout_stays_json_while_progress_goes_to_stderr`
- [x] A-02.4 `search` dengan lebih dari 50 hasil mengembalikan 50 item, `"truncated": true`, dan `"total"` yang benar. `a02_4_search_with_more_than_50_results_is_truncated`
- [x] A-02.5 Setiap path dalam output bisa langsung dipakai sebagai `<catatan>` di perintah lain. `a02_5_every_path_in_output_works_as_a_note_argument`

### A-03 · Exit code

Skrip dan agent tahu apa yang terjadi hanya dari exit code.

| Exit code | Arti | Kode error (A-02) |
| --- | --- | --- |
| 0 | Berhasil, termasuk hasil kosong | — |
| 1 | Error umum, atau peringatan dengan `--strict` | `io_error`, `not_utf8` |
| 2 | Argumen salah atau vault tidak ditentukan | `invalid_argument` |
| 3 | Tidak ditemukan | `vault_not_found`, `note_not_found` |
| 4 | Konflik atau ambigu | `ambiguous_note` |
| 5 | Indeks terkunci terlalu lama | `index_locked` |
| 130 | Dihentikan dengan `Ctrl+C` | — |

**Kasus tepi**

- Hasil kosong bukan error: `backlinks` tanpa hasil keluar dengan 0; kode 3 hanya bila catatannya sendiri tidak ada.
- Pipe yang ditutup lebih awal (`patinote list | head -5`) berhenti tanpa pesan error.
- Peringatan tidak mengubah exit code, kecuali dengan opsi global `--strict` (keluar dengan 1).
- Indeks rusak dibangun ulang otomatis (F-03) dan hanya menghasilkan peringatan.

**Kriteria penerimaan**

- [x] A-03.1 Setiap baris tabel punya test yang memicu kondisinya dan memeriksa exit code serta kode error JSON. `a03_1_each_exit_code_row`
- [x] A-03.2 `backlinks` untuk catatan tanpa backlink keluar dengan 0 dan daftar kosong. `a03_2_no_backlinks_is_exit_0_and_an_empty_list`
- [x] A-03.3 `patinote list | head -1` keluar tanpa pesan "broken pipe". `a03_3_list_into_head_ends_without_broken_pipe`
- [x] A-03.4 Vault dengan satu frontmatter rusak: `patinote index` keluar dengan 0 dan satu peringatan; dengan `--strict` keluar dengan 1. `a03_4_warnings_exit_0_and_strict_exits_1`
- [x] A-03.5 `Ctrl+C` di tengah `patinote index` keluar dengan 130 dan indeks tetap utuh. `a03_5_ctrl_c_during_index_exits_130_and_keeps_the_index_intact`

### A-04 · Server MCP read-only

Agent (Claude Code, Codex CLI, Gemini CLI) mencari, membaca, dan menelusuri vault lewat tool terstruktur, tanpa kemampuan mengubah vault.

**Menjalankan**

- `patinote mcp --vault ~/Vault`, transport stdio saja; agent menjalankan proses dan proses berhenti bersama agent. HTTP dipertimbangkan bersama API lokal di Fase 9.
- Satu server untuk satu vault, ditentukan saat dimulai.
- Contoh konfigurasi (format `mcpServers` yang umum):

```json
{
  "mcpServers": {
    "patinote": {
      "command": "patinote",
      "args": ["mcp", "--vault", "/path/to/Vault"]
    }
  }
}
```

**Tool**

| Tool | Setara CLI |
| --- | --- |
| `search_notes` | `search` |
| `read_note` | `read` |
| `list_notes` | `list` |
| `get_backlinks` | `backlinks` |
| `get_links` | `links` |
| `list_tags` | `tags` |
| `find_orphans` | `orphans` |
| `list_unresolved` | `unresolved` |
| `resolve_link` | `resolve` |
| `vault_info` | `vault info` |
| `list_conflicts` | `conflicts` |

- Hasil setiap tool identik dengan `--json` CLI (A-02), dikirim sebagai konten terstruktur sekaligus teks.
- Deskripsi tool ditulis untuk agent: kapan dipakai dan contoh argumen.
- Indeks disegarkan di setiap pemanggilan (F-04). MCP resources menyusul di Fase 3 bersama tool tulis.

**Keamanan**

- Tidak ada tool yang menulis ke vault; yang ditulis hanya cache indeks di `~/.cache`.
- Path yang keluar dari vault (`../`, path absolut) ditolak dengan `invalid_argument`.
- `read_note` dibatasi 100 KB per panggilan; catatan lebih panjang ditandai `"truncated": true` dan dibaca per potongan lewat parameter `lines`.
- Stdout hanya berisi pesan protokol MCP; log ke stderr.

**Kriteria penerimaan**

- [ ] A-04.1 MCP Inspector menampilkan ke-11 tool dengan skema argumennya.
- [x] A-04.2 Hasil setiap tool identik dengan `--json` perintah CLI yang setara untuk argumen yang sama. `a04_2_tool_results_equal_the_cli_json`
- [ ] A-04.3 Dalam sesi Claude Code sungguhan, agent mencari, membaca, dan menelusuri backlinks hanya lewat tool Patinote.
- [x] A-04.4 Hash semua file vault identik sebelum dan sesudah sesi agent yang memanggil semua tool. `a04_4_a_session_calling_every_tool_changes_nothing_in_the_vault`
- [x] A-04.5 `read_note` dengan `../../etc/passwd` ditolak dengan `invalid_argument`. `a04_5_paths_outside_the_vault_are_rejected`
- [x] A-04.6 Catatan 300 KB terbaca utuh dalam tiga potongan tanpa bagian hilang atau tumpang tindih. `a04_6_a_300_kb_note_reads_in_three_parts_without_gaps_or_overlap`
- [x] A-04.7 Selama sesi, stdout tidak pernah berisi teks di luar protokol MCP. `a04_7_stdout_only_carries_protocol_messages`

### A-05 · `AGENTS.md` di vault

Agent yang bekerja di folder vault langsung tahu cara memperlakukan vault ini: lewat Patinote dan mengikuti konvensi pengguna. File ini berbeda dari `AGENTS.md` di repo kode Patinote, yang ditujukan untuk agent yang membangun aplikasi (dibahas di Fase 0).

**Membuat**

- `patinote agents init` menulis `AGENTS.md` di root vault. Ini hanya terjadi karena pengguna menjalankan perintahnya; tidak pernah otomatis saat vault dibuka.
- Bila `AGENTS.md` sudah ada, `init` tidak menimpanya: keluar dengan exit code 4 dan menyarankan `--update`.
- `--update` hanya mengganti isi di antara penanda `<!-- patinote:begin -->` dan `<!-- patinote:end -->`.
- `--for claude,gemini` membuat `CLAUDE.md` dan `GEMINI.md` pendek yang merujuk ke `AGENTS.md`.
- `AGENTS.md` diperlakukan sebagai catatan biasa: muncul di quick switcher dan bisa diedit di Patinote.

**Isi**

| Bagian | Isi | Penulis |
| --- | --- | --- |
| Aturan utama | Cari dan baca lewat `patinote` (CLI atau MCP); jangan ganti nama atau pindahkan file langsung karena link rusak, minta pengguna melakukannya lewat Patinote sampai A-06 tersedia | Otomatis |
| Konvensi vault | Dideteksi: folder utama, 20 tag terbanyak, format nama daily note bila ada, folder lampiran, gaya link | Otomatis |
| Aturan menulis | Pertahankan frontmatter; wikilink dengan nama terpendek yang unik; jangan sentuh `.obsidian/` dan `.trash/` | Otomatis |
| Ringkasan perintah | Perintah CLI, tool MCP, dan exit code yang paling sering dipakai | Otomatis |
| Catatan pribadi | Instruksi tambahan pengguna | Pengguna |

**Kriteria penerimaan**

- [ ] A-05.1 `patinote agents init` di vault asli pengguna menghasilkan konvensi yang benar (folder, tag, gaya link).
- [x] A-05.2 Setelah pengguna menulis di bagian catatan pribadi, `--update` tidak mengubah bagian itu satu karakter pun. `a05_2_update_never_changes_the_personal_part`
- [x] A-05.3 `--for claude,gemini` membuat `CLAUDE.md` dan `GEMINI.md` yang merujuk ke `AGENTS.md`. `a05_3_for_claude_and_gemini_point_to_agents_md`
- [x] A-05.4 `init` pada vault yang sudah punya `AGENTS.md` tidak menimpa file dan keluar dengan exit code 4. `a05_4_init_never_overwrites_and_exits_4`
- [ ] A-05.5 Dalam sesi Claude Code sungguhan di folder vault, agent memakai `patinote` untuk mencari catatan, bukan `grep` atau `find`, tanpa diminta khusus.
