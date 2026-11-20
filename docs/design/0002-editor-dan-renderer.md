# 0002 — Editor dan renderer

- Status: **usulan** (20 November 2026). Renderer default menunggu jawaban Q-004 di `docs/QUESTIONS.md`.
- Terkait PRD: F-05, F-06, F-28; Bagian I "Anggaran memori" butir 1 dan 3, dan tabel Performa
- Terkait: `CLAUDE.md` bagian 3 (langkah 0.6 dan daftar uji tampilan), `0003` bagian 10
- Prototipe: `experiments/editor-proto/` (dibuang setelah langkah 2.3; tidak ikut workspace utama)

## 1. Ringkasan

| Pertanyaan | Jawaban |
| --- | --- |
| Versi Iced | **0.14.0**, dikunci dengan `=0.14.0` saat masuk ke `patinote-app` |
| Widget editor | **Widget sendiri** di atas `iced::advanced`. `text_editor` bawaan hanya memenuhi butir 1 dan 7 |
| Daftar uji tampilan | Kedelapan butir bisa dipenuhi; cadangan di `0003` bagian 10 **tidak** diperlukan |
| Renderer default | Belum diputuskan: tiny-skia memenuhi target RAM dan waktu buka tetapi tidak mulus saat menggulir satu layar penuh; wgpu kebalikannya (Q-004) |
| Screenshot headless | Bisa di CI dengan tiny-skia, tetapi lewat harness sendiri, karena `iced_test` 0.14 selalu memotret di skala 2,0 |
| Font sistem | Iced 0.14 selalu memindai font sistem; bisa dibatasi dengan `FONTCONFIG_FILE` (bagian 5.1) |

## 2. Lingkungan uji

Laptop pemilik: Intel Core i5-1235U dengan Iris Xe (Alder Lake), RAM 14 GiB, Ubuntu 26.04.1 LTS, GNOME di Wayland, layar 1920×1080 dengan skala 1,25. Refresh layar terbaca 48 Hz selama pengukuran (mode hemat daya), sehingga satu frame sekitar 20,8 ms. Build release, Rust 1.100.0.

## 3. Hasil daftar uji tampilan

Setiap butir punya test otomatis di `experiments/editor-proto/tests/screenshot.rs` yang memeriksa warna piksel pada skala 1,0, 1,25, dan 2,0. Screenshot-nya disimpan sebagai artefak CI (`editor-proto-snapshots`).

| Butir | Widget bawaan `text_editor` | Widget sendiri | Bukti |
| --- | --- | --- | --- |
| 1. Warna sintaks per bagian teks, font dari byte | **Bisa** (highlighter mengatur warna dan font) | Bisa | `item1_builtin_text_editor_colors_runs_with_bundled_font` |
| 2. Latar seleksi, semua hasil `/`, hasil aktif | Tidak bisa: `highlighter::Format` hanya punya `color` dan `font`; hanya ada satu warna seleksi | **Bisa**: `Paragraph::span_bounds` memberi persegi tiap bagian teks, termasuk yang terbungkus | `item2_selection_and_search_backgrounds` |
| 3. Latar baris kursor sebaris penuh | Tidak bisa | **Bisa** | `item3_and_5_…` |
| 4. Garis bawah bergelombang | Tidak bisa | **Bisa**: digambar dengan quad kecil berpola zigzag di bawah `span_bounds`. `Span::underline` Iced hanya garis lurus | `item4_ambiguous_link_has_wavy_underline` |
| 5. Kursor blok (normal, visual) dan garis 2 px (insert) berwarna token | Tidak bisa: kursor selalu garis 1 px berwarna teks | **Bisa**: posisi dari `Paragraph::grapheme_position`, lebar dari posisi karakter berikutnya | `item3_and_5_…`, `item5_insert_cursor_is_a_line` |
| 6. Nomor baris relatif, sejajar saat soft wrap | Tidak bisa (tidak ada kolom nomor) | **Bisa**: satu `Paragraph` per baris asli, nomor di baris tampilan pertamanya | Screenshot `01-normal` (baris 8 terbungkus) |
| 7. Kolom maksimal 920 px di tengah | **Bisa** (dibungkus `container`) | Bisa | `item7_column_is_at_most_920_px_and_centered` |
| 8. Screenshot headless tiny-skia di CI | — | **Bisa**, dengan harness sendiri (bagian 5.2) | Job CI `editor-proto` |

Catatan untuk widget sendiri:

- Di renderer Iced, semua quad dalam satu layer digambar sebelum teks. Latar bagian teks memang harus di bawah teks, jadi aman. Kursor blok dan huruf gelap di atasnya digambar di layer terpisah (`Renderer::with_layer`).
- Paragraf di-cache per baris (kunci: isi, warna, dekorasi, lebar kolom) dan hanya baris di layar yang dibuat, sehingga catatan 5.000 baris tidak menambah biaya per frame. `view`, layout, dan `draw` bersama-sama di bawah 0,5 ms (p95) untuk catatan 5.000 baris.

## 4. Pengukuran

### 4.1 Cara mengukur

- **Di jendela sungguhan** (`editor-proto --bench`): aplikasi memutar tombol otomatis dari subscription `window::frames()`, yaitu 200× `j`, mengetik 178 karakter, `/indeks` lalu 30× `n`, dan seleksi visual 30 baris. Yang dicatat: waktu dari tombol sampai `draw` selesai, jarak antar-frame per skenario, waktu dari `main` sampai frame pertama, dan PSS dari `/proc/self/smaps_rollup` setelah 2 detik idle.
- **Headless** (`render_bench`): biaya satu frame penuh setelah setiap tombol, dibagi menjadi `ui` (`view`, layout, `draw`) dan `raster` (rasterisasi tiny-skia, atau render wgpu ditambah membaca balik piksel).
- **Keterbatasan:** Iced tidak memberi tahu aplikasi kapan frame selesai ditampilkan. Latensi sampai piksel muncul di layar dibaca dari jarak antar-frame. Pemeriksaan akhir dengan mengetik sungguhan tetap dilakukan pemilik (F-05.1).

### 4.2 RAM dan waktu buka

Catatan 1.000 baris, tiga kali run:

| Renderer | PSS | RSS | Sampai frame pertama |
| --- | --- | --- | --- |
| tiny-skia | 27,9–28,2 MB | 41 MB | 137–179 ms |
| wgpu (Vulkan) | 99,7–100,5 MB | 111 MB | 309–372 ms |
| wgpu (OpenGL, `WGPU_BACKEND=gl`) | 96 MB | — | 463 ms |

Dengan catatan 5.000 baris PSS-nya 29–30 MB (tiny-skia) dan 103 MB (wgpu). Target PRD: RAM idle < 100 MB dengan vault 10.000 catatan, dan buka < 300 ms. Prototipe belum memuat indeks, panel, maupun vault, jadi angka wgpu adalah batas bawahnya.

### 4.3 Kelancaran per skenario

Jarak antar-frame (p50 / p95, ms) di jendela sungguhan, catatan 5.000 baris. Satu frame = 20,8 ms pada 48 Hz.

| Skenario | tiny-skia | wgpu |
| --- | --- | --- |
| Mengetik di satu baris | 20,8 / 22,3 | 20,8 / 22,3 |
| `/` lalu `n` | 20,8 / 21,9 | 20,8 / 22,1 |
| Menahan `j` (layar ikut bergulir) | **120 / 204** | 20,8 / 22,8 |
| Seleksi visual 30 baris | **223 / 578** | 20,8 / 23,2 |

Biaya satu frame penuh secara headless:

| Renderer | Skala 1,0 | Skala 1,25 | Skala 2,0 |
| --- | --- | --- | --- |
| tiny-skia (rasterisasi) | 15,5 ms | 30,0 ms | 67 ms |
| wgpu (render + baca balik) | 13,6 ms | — | 24 ms |

Kesimpulan:

- tiny-skia lancar selama yang berubah hanya sebagian kecil layar (mengetik, mencari). Saat seluruh layar berubah, rasterisasi saja sudah 30 ms di skala 1,25, dan di jendela sungguhan menjadi 120–220 ms. Selisih sekitar 4× dibanding headless belum dijelaskan; kemungkinan dari jalur penayangan `softbuffer` di Wayland dengan skala pecahan, dan layak ditelusuri di Fase 2.
- wgpu menjaga satu frame per refresh di semua skenario, dengan biaya RAM dan waktu buka di atas target.
- Membatasi font ke font Patinote (bagian 5.1) mempercepat buka sekitar 20 ms dan tidak mengubah PSS.

## 5. Temuan lain

### 5.1 Font sistem selalu dipindai

`iced_graphics` 0.14 membuat `cosmic_text::FontSystem::new_with_fonts`, yang selalu memanggil `fontdb::Database::load_system_fonts()`. Di laptop uji ada 513 font (183 MB). Ini bertentangan dengan PRD (anggaran memori butir 3).

- `fontdb` membaca daftar folder dari `FONTCONFIG_FILE` bila variabel itu ada. File konfigurasi yang hanya berisi folder font Patinote membuat pemindaian berhenti di sana. Ini sudah diukur di bagian 4.
- Di edition 2024, `std::env::set_var` adalah `unsafe`, jadi aplikasi tidak bisa mengisinya sendiri tanpa persetujuan (`CLAUDE.md` bagian 9). Pilihannya: peluncur (`.desktop` dan `patinote open`) mengisi variabel itu, atau aplikasi menjalankan ulang dirinya sekali dengan variabel terisi.
- Akibat sampingnya: tanpa font sistem, huruf yang tidak ada di Inter dan JetBrains Mono (misalnya CJK atau emoji) tampil sebagai kotak. Perlu diputuskan di langkah 2.1: menambah font cadangan kecil, atau mengizinkan pemindaian font sistem secara lazy hanya saat ada huruf yang tidak tersedia.

### 5.2 `iced_test` 0.14 memotret di skala 2,0

`Simulator::snapshot` menulis `scale_factor = 2.0` langsung di kode. `CLAUDE.md` bagian 6 meminta skala 1,0 (ditambah 1,25 dan 2,0). Prototipe memakai harness kecil (`src/headless.rs`) yang melakukan hal yang sama dengan `Simulator` lewat API publik (`UserInterface` dan `Headless::screenshot`) dengan skala bebas. Harness ini dibawa ke `crates/patinote-app/tests/ui_snapshots.rs` di langkah 2.1; variabel `ICED_TEST_BACKEND` tetap dipakai untuk memilih backend.

### 5.3 Dependensi Iced dan lisensi

Saat Iced masuk ke `patinote-app`, `deny.toml` perlu mengizinkan Apache-2.0, Zlib, BSD-2-Clause, BSD-3-Clause, ISC, BSL-1.0, dan CC0-1.0. Semuanya permisif dan kompatibel dengan MIT. Tidak ada crate yang hanya berlisensi copyleft; `self_cell` (`Apache-2.0 OR GPL-2.0-only`) dan `r-efi` punya pilihan permisif. Fitur Iced yang dipakai: `advanced`, `crisp`, `thread-pool`, `tiny-skia`, `wgpu`, `wayland`, `x11`, tanpa fitur default lain.

## 6. Pendekatan untuk Fase 2

- Editor adalah widget sendiri di atas `iced::advanced`: satu `Paragraph` per baris asli dengan spans, latar dan garis bawah dari `span_bounds`, kursor dari `grapheme_position`, dan cache paragraf per baris di state widget.
- Logika Vim tetap modul murni tanpa tipe Iced (`vim.rs` di prototipe), lalu diganti modul lengkap F-06 yang diuji terhadap `nvim --headless`.
- Yang belum dicoba di prototipe dan harus diselesaikan di langkah 2.3: input method (IME), posisi berbasis grapheme (prototipe memakai indeks karakter), gulir yang memperhitungkan tinggi baris terbungkus, baris yang sangat panjang, klik mouse ke posisi teks (`hit_test`), undo/redo, dan aksesibilitas.

## 7. Keputusan yang menunggu

- **Q-004:** renderer default. Rekomendasi agent: keduanya ikut dalam binary dengan default tiny-skia, dan optimasi gulir layar penuh dikejar di Fase 2.
- **Font sistem** (bagian 5.1): diputuskan di langkah 2.1.
