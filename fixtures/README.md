# Fixtures

Semua isi di folder ini **sintetis**. Tidak ada yang berasal dari vault asli pemilik, dan memang tidak boleh ada (lihat `CLAUDE.md` bagian 7).

## `vault-mockup/`

Vault kecil yang isinya sama dengan contoh di `docs/mockups/`. Dipakai untuk snapshot UI (`crates/patinote-app/tests/ui_snapshots.rs`) agar screenshot aplikasi bisa dibandingkan langsung dengan `docs/mockups/png/`.

| Kebutuhan mockup | File di vault |
| --- | --- |
| Catatan utama di 01, 02, 03, 07, 13 | `Proyek/Desain indeks Patinote.md`. Memuat semua elemen tabel F-27, sehingga juga menjadi catatan contoh untuk F-27.1 |
| Backlinks dari catatan, heading, dan properti (F-09.2) | `Proyek/Patinote.md`, `Harian/2026-11-19.md` (link ke `#Tabel inti`), properti `project` di catatan utama |
| Alias di quick switcher (05) | `Proyek/Patinote.md` (`Second Brain`), `Arsip/Skema indeks lama.md` (`Design index`) |
| Link ambigu (12) | `Proyek/Rapat.md` dan `Arsip/2025/Rapat.md`, ditautkan dari `Harian/2026-11-19.md` |
| Link ke catatan yang belum ada | `[[tantivy]]`, `[[Ide Baru]]` |
| Konflik Syncthing (15, 16) | `Rapat/Rapat 19 Nov.sync-conflict-…md`, `Harian/2026-11-18.sync-conflict-…md` |
| Gambar di mode baca (07) | `Proyek/Lampiran/diagram-indeks.png` |
| Folder yang diabaikan | `.obsidian/` |

Keadaan yang butuh file khusus tidak disimpan di sini, tetapi dibuat oleh test ke folder sementara: catatan 6 MB, file Latin-1, file tanpa izin baca, symlink loop, dan perubahan dari luar saat mengedit.

Angka di mockup (misalnya "2,143 notes" atau jumlah tag) tidak harus sama dengan vault ini. Yang dibandingkan adalah susunan, teks antarmuka, warna, dan ukuran.

Mengubah isi vault ini berarti snapshot UI perlu dibuat ulang; sebutkan di PR.
